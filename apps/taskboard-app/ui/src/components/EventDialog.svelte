<script lang="ts" module>
  import type { Id } from '../lib/bindings/Id'
  import type { EventKind } from '../lib/bindings/EventKind'

  /** What the dialog opens with: a new draft, or an existing event/occurrence. */
  export interface EventDraft {
    id?: Id
    /** Series start of the clicked occurrence (recurring events). */
    originalStart?: number
    kind: EventKind
    start: number
    end: number
    allDay?: boolean
    title?: string
    clientId?: Id | null
    projectId?: Id | null
    taskId?: Id | null
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte'
  import { ExternalLink, Trash2 } from '@lucide/svelte'
  import type { EventPatch } from '../lib/bindings/EventPatch'
  import type { EventStatus } from '../lib/bindings/EventStatus'
  import { fromIso, HOUR, MINUTE, repeatOptions } from '../lib/cal.svelte'
  import { describeRule } from '../lib/repeat'
  import { isoDate } from '../lib/format'
  import { errorMessage, ipc, openLink } from '../lib/ipc'
  import { app, groupsIn, looseProjectsIn, projectsOf } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'
  import Notes from './Notes.svelte'
  import { segmented } from '../lib/segmented'

  let { draft, onclose }: { draft: EventDraft; onclose: () => void } = $props()
  // The calendar remounts this dialog per draft, so read it once.
  const initial = untrack(() => draft)

  const pad = (n: number) => String(n).padStart(2, '0')
  const hm = (ms: number) => {
    const d = new Date(ms)
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`
  }
  function at(date: string, time: string): number {
    const [h, m] = time.split(':').map(Number)
    const d = fromIso(date)
    d.setHours(h ?? 0, m ?? 0, 0, 0)
    return d.getTime()
  }

  // ---------- form state ----------
  const f = $state({
    kind: initial.kind,
    title: initial.title ?? '',
    date: isoDate(new Date(initial.start)),
    start: hm(initial.start),
    end: hm(initial.end),
    allDay: initial.allDay ?? false,
    endDate: isoDate(new Date(initial.allDay ? initial.end - 1 : initial.start)),
    owner: initial.projectId ? `p:${initial.projectId}` : initial.clientId ? `c:${initial.clientId}` : '',
    location: '',
    url: '',
    attendees: '',
    notes: '',
    rule: null as string | null,
    status: 'confirmed' as EventStatus,
    counts: null as boolean | null,
    flexible: initial.kind === 'block',
  })
  let recurring = $state(false)
  let recurrenceText = $state<string | null>(null)
  let scope = $state<'one' | 'following' | 'all'>('one')
  let saving = $state(false)
  const editing = !!initial.id
  /** An existing event can't be saved before its details arrive (else empty
   *  fields would overwrite its link, notes and repeat). */
  let loaded = $state(!initial.id)

  /** The series-wide fields as loaded: an occurrence can't have its own. */
  const seriesKey = () => JSON.stringify([f.kind, f.owner, f.location, f.url, f.attendees, f.rule, f.status, f.counts, f.flexible, f.allDay])
  let seriesAsLoaded = $state('')
  /** Changes that can only apply to every event in the series. */
  const seriesChanged = $derived(loaded && editing && seriesKey() !== seriesAsLoaded)

  /** Title and notes as the dialog opened (the occurrence's own, if it has them),
   *  so saving sends only what was changed. */
  let shown = { title: initial.title ?? '', notes: '' }

  // Existing event: load its full details (and one occurrence's own notes).
  $effect(() => {
    if (!initial.id) return
    const id = initial.id
    const original = initial.originalStart
    Promise.all([ipc.getEvent(id), original === undefined ? null : ipc.occurrenceNotes(id, original).catch(() => null)])
      .then(([e, ownNotes]) => {
        f.title = initial.title ?? e.title
        f.kind = e.kind
        f.owner = e.project_id ? `p:${e.project_id}` : e.client_id ? `c:${e.client_id}` : ''
        f.location = e.location
        f.url = e.meeting_url
        f.attendees = e.attendees
        f.notes = ownNotes ?? e.notes_md
        shown = { title: f.title, notes: f.notes }
        f.rule = e.recurrence_rule
        f.status = e.status
        f.counts = e.counts_as_time
        f.flexible = e.flexible
        recurring = !!e.recurrence_rule
        recurrenceText = e.recurrence_text
        seriesAsLoaded = seriesKey()
        loaded = true
      })
      .catch((err) => toast(`Couldn't load this event, so it can't be saved: ${errorMessage(err)}`, { kind: 'error' }))
  })

  const repeats = $derived(repeatOptions(fromIso(f.date)))
  const isCustomRule = $derived(!!f.rule && !repeats.some((r) => r.rule === f.rule))

  function onStartChange(value: string) {
    // Keep the duration when the start moves.
    const dur = at(f.date, f.end) - at(f.date, f.start)
    f.start = value
    f.end = hm(at(f.date, value) + Math.max(15 * MINUTE, dur))
  }

  function times(): { start_at?: number; end_at?: number; all_day_start?: string; all_day_end?: string } {
    if (f.allDay) return { all_day_start: f.date, all_day_end: f.endDate < f.date ? f.date : f.endDate }
    let s = at(f.date, f.start)
    let e = at(f.date, f.end)
    if (e <= s) e += 24 * HOUR // ends after midnight
    return { start_at: s, end_at: e }
  }

  function owner(): { client_id: Id | null; project_id: Id | null } {
    if (f.owner.startsWith('p:')) return { client_id: null, project_id: f.owner.slice(2) }
    if (f.owner.startsWith('c:')) return { client_id: f.owner.slice(2), project_id: null }
    return { client_id: null, project_id: null }
  }

  /** Local midnight of a YYYY-MM-DD date. */
  const dayStart = (iso: string) => fromIso(iso).getTime()
  /** YYYY-MM-DD shifted by whole days. */
  function shiftDay(iso: string, days: number): string {
    const d = fromIso(iso)
    d.setDate(d.getDate() + days)
    return isoDate(d)
  }
  const daysBetween = (a: string, b: string) => Math.round((dayStart(b) - dayStart(a)) / (24 * HOUR))

  async function save() {
    if (!loaded || saving) return
    if (!f.title.trim()) {
      toast('Give it a title', { kind: 'error' })
      return
    }
    saving = true
    try {
      const t = times()
      const o = owner()
      const fields: EventPatch = {
        title: f.title,
        kind: f.kind,
        client_id: o.client_id,
        project_id: o.project_id,
        location: f.location,
        meeting_url: f.url,
        attendees: f.attendees,
        notes_md: f.notes,
        status: f.status,
        flexible: f.flexible,
        ...(f.counts === null ? {} : { counts_as_time: f.counts }),
      }
      if (!initial.id) {
        await ipc.createEvent({
          kind: f.kind,
          title: f.title,
          client_id: o.client_id ?? undefined,
          project_id: o.project_id ?? undefined,
          task_id: initial.taskId ?? undefined,
          ...t,
          location: f.location,
          meeting_url: f.url,
          attendees: f.attendees,
          notes_md: f.notes,
          status: f.status,
          flexible: f.flexible,
          ...(f.counts === null ? {} : { counts_as_time: f.counts }),
          recurrence_rule: f.rule ?? undefined,
        })
        toast(f.kind === 'meeting' ? 'Meeting added' : 'Time block added')
      } else if (!recurring || initial.originalStart === undefined) {
        await ipc.updateEvent(initial.id, { ...fields, ...t, recurrence_rule: f.rule })
      } else if (scope === 'one') {
        // Only what was changed here, so the occurrence keeps following the
        // series for the rest (a time it was never moved to stays relative).
        // All-day occurrences move by whole days (start of each day).
        const start = f.allDay ? dayStart(f.date) : t.start_at
        const end = f.allDay ? dayStart(shiftDay(t.all_day_end ?? f.date, 1)) : t.end_at
        const moved = start !== initial.start || end !== initial.end
        await ipc.editOccurrence(initial.id, initial.originalStart, {
          ...(moved ? { start_at: start, end_at: end } : {}),
          ...(f.title !== shown.title ? { title: f.title } : {}),
          ...(f.notes !== shown.notes ? { notes_md: f.notes } : {}),
        })
        // Link, place, people, group, type and options belong to the series.
        if (seriesChanged) {
          const o = owner()
          await ipc.updateEvent(initial.id, {
            kind: f.kind,
            client_id: o.client_id,
            project_id: o.project_id,
            location: f.location,
            meeting_url: f.url,
            attendees: f.attendees,
            status: f.status,
            flexible: f.flexible,
            ...(f.counts === null ? {} : { counts_as_time: f.counts }),
            recurrence_rule: f.rule,
          })
        }
      } else {
        // This occurrence's own title and notes don't spread to the others
        // unless they were changed here.
        const { title: _t, notes_md: _n, ...rest } = fields
        const shared: EventPatch = {
          ...rest,
          ...(f.title !== shown.title ? { title: f.title } : {}),
          ...(f.notes !== shown.notes ? { notes_md: f.notes } : {}),
          recurrence_rule: f.rule,
        }
        if (scope === 'following') {
          await ipc.splitSeries(initial.id, initial.originalStart, { ...shared, ...t })
        } else {
          // Shift the whole series by however much the time was changed here.
          const current = await ipc.getEvent(initial.id)
          if (t.start_at !== undefined && t.end_at !== undefined && current.start_at !== null) {
            shared.start_at = current.start_at + (t.start_at - initial.start)
            shared.end_at = shared.start_at + (t.end_at - t.start_at)
          } else if (f.allDay && current.all_day_start && current.all_day_end) {
            // All-day: by whole days, keeping how many days each one spans.
            const shown = isoDate(new Date(initial.start))
            const by = daysBetween(shown, f.date)
            const span = daysBetween(f.date, t.all_day_end ?? f.date)
            shared.all_day_start = shiftDay(current.all_day_start, by)
            shared.all_day_end = shiftDay(shared.all_day_start, span)
          }
          await ipc.updateEvent(initial.id, shared)
        }
      }
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }

  async function remove() {
    if (!initial.id) return
    try {
      // "This and following" is two steps (split, then delete the new part);
      // Undo takes back both.
      let steps = 1
      if (!recurring || initial.originalStart === undefined || scope === 'all') await ipc.deleteEvent(initial.id)
      else if (scope === 'one') await ipc.editOccurrence(initial.id, initial.originalStart, { cancelled: true })
      else {
        const later = await ipc.splitSeries(initial.id, initial.originalStart, {})
        await ipc.deleteEvent(later.id)
        steps = 2
      }
      toast('Deleted', {
        action: {
          label: 'Undo',
          run: () =>
            void (async () => {
              for (let i = 0; i < steps; i++) await ipc.undo()
            })(),
        },
      })
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  const durationLabel = $derived.by(() => {
    if (f.allDay) return ''
    let m = Math.round((at(f.date, f.end) - at(f.date, f.start)) / MINUTE)
    if (m <= 0) m += 24 * 60
    const h = Math.floor(m / 60)
    return h ? `${h}h${m % 60 ? ` ${m % 60}m` : ''}` : `${m}m`
  })

  // Keep end date sane for all-day events.
  $effect(() => {
    if (f.allDay && f.endDate < f.date) f.endDate = f.date
  })

</script>

<Modal title={editing ? (f.kind === 'meeting' ? 'Edit meeting' : 'Edit time block') : f.kind === 'meeting' ? 'New meeting' : 'New time block'} {onclose} width={560}>
  <div class="flex flex-col gap-3">
    <div class="flex gap-2">
      <div class="flex rounded-md border border-border bg-surface-1 p-0.5" role="radiogroup" aria-label="Type" use:segmented>
        {#each [{ v: 'meeting', l: 'Meeting' }, { v: 'block', l: 'Time block' }] as k (k.v)}
          <button role="radio" aria-checked={f.kind === k.v} class="seg" onclick={() => {
            f.kind = k.v as EventKind
            f.flexible = f.kind === 'block'
          }}>{k.l}</button>
        {/each}
      </div>
    </div>

    <input class="field !h-10 text-[15px] font-medium" aria-label="Title" placeholder={f.kind === 'meeting' ? 'Meeting title' : 'What is this time for?'} bind:value={f.title} onkeydown={(e) => e.key === 'Enter' && void save()} />

    <div class="grid grid-cols-[1fr_auto_auto_auto] items-end gap-2">
      <label>
        <span class="field-label">{f.allDay ? 'From' : 'Date'}</span>
        <input type="date" class="field" bind:value={f.date} />
      </label>
      {#if f.allDay}
        <label class="col-span-2">
          <span class="field-label">To</span>
          <input type="date" class="field" bind:value={f.endDate} min={f.date} />
        </label>
      {:else}
        <label>
          <span class="field-label">Start</span>
          <input type="time" class="field w-[112px]" value={f.start} step="300" onchange={(e) => onStartChange(e.currentTarget.value)} />
        </label>
        <label>
          <span class="field-label">End <span class="text-subtle">{durationLabel}</span></span>
          <input type="time" class="field w-[112px]" bind:value={f.end} step="300" />
        </label>
      {/if}
      <label class="flex h-8 items-center gap-1.5 text-xs text-muted">
        <input type="checkbox" class="accent-[var(--accent)]" bind:checked={f.allDay} /> All day
      </label>
    </div>

    <div class="grid grid-cols-2 gap-2">
      <label>
        <span class="field-label">Repeat</span>
        <select class="field" value={f.rule ?? ''} onchange={(e) => (f.rule = e.currentTarget.value || null)}>
          {#each repeats as r (r.label)}<option value={r.rule ?? ''}>{r.label}</option>{/each}
          {#if isCustomRule}<option value={f.rule}>{recurrenceText ?? describeRule(f.rule ?? '')}</option>{/if}
        </select>
      </label>
      <label>
        <span class="field-label">Group / project</span>
        <select class="field" bind:value={f.owner}>
          <option value="">None</option>
          {#each app.sections as s (s.id)}
            {#each groupsIn(s.id) as c (c.id)}
              <optgroup label={app.sections.length > 1 ? `${s.name} › ${c.name}` : c.name}>
                <option value="c:{c.id}">{c.name} (no project)</option>
                {#each projectsOf(c.id) as p (p.id)}<option value="p:{p.id}">{p.name}</option>{/each}
              </optgroup>
            {/each}
            {#if looseProjectsIn(s.id).length}
              <optgroup label={s.name}>
                {#each looseProjectsIn(s.id) as p (p.id)}<option value="p:{p.id}">{p.name}</option>{/each}
              </optgroup>
            {/if}
          {/each}
        </select>
      </label>
    </div>

    {#if f.kind === 'meeting'}
      <div class="grid grid-cols-2 gap-2">
        <label>
          <span class="field-label">Meeting link</span>
          <div class="flex gap-1">
            <input class="field" placeholder="https://…" bind:value={f.url} />
            {#if /^https?:\/\//.test(f.url)}
              <button class="btn !px-2" title="Join" aria-label="Open meeting link" onclick={() => void openLink(f.url)}><ExternalLink size={14} /></button>
            {/if}
          </div>
        </label>
        <label>
          <span class="field-label">Location</span>
          <input class="field" placeholder="Office, café, phone…" bind:value={f.location} />
        </label>
      </div>
      <label>
        <span class="field-label">Attendees</span>
        <input class="field" placeholder="Names or emails" bind:value={f.attendees} />
      </label>
    {/if}

    <div>
      <span class="field-label">{f.kind === 'meeting' ? 'Agenda & notes' : 'Notes'}</span>
      <Notes
        value={f.notes}
        rows={3}
        label={f.kind === 'meeting' ? 'Agenda and notes' : 'Notes'}
        placeholder={f.kind === 'meeting' ? 'Agenda, decisions, - [ ] action items (Markdown)' : 'Notes (Markdown)'}
        onsave={(v) => (f.notes = v)}
      />
    </div>

    <div class="flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px]">
      {#if f.kind === 'meeting'}
        <label class="flex items-center gap-1.5">
          <input type="checkbox" class="accent-[var(--accent)]" checked={f.counts ?? (!!owner().client_id || !!owner().project_id)} onchange={(e) => (f.counts = e.currentTarget.checked)} />
          Counts as work time
        </label>
        <label class="flex items-center gap-1.5">
          <input type="checkbox" class="accent-[var(--accent)]" checked={f.status === 'tentative'} onchange={(e) => (f.status = e.currentTarget.checked ? 'tentative' : 'confirmed')} />
          Tentative
        </label>
      {:else}
        <label class="flex items-center gap-1.5" title="Flexible blocks can be booked over by the slot finder">
          <input type="checkbox" class="accent-[var(--accent)]" bind:checked={f.flexible} />
          Flexible (slot finder may use this time)
        </label>
      {/if}
    </div>

    {#if editing && recurring && initial.originalStart !== undefined}
      <fieldset class="rounded-md border border-border p-2.5">
        <legend class="px-1 text-xs font-medium text-muted">Apply changes to {recurrenceText ? `· ${recurrenceText}` : ''}</legend>
        <div class="flex flex-wrap gap-4 text-[13px]">
          {#each [{ v: 'one', l: 'This event' }, { v: 'following', l: 'This and following' }, { v: 'all', l: 'All events' }] as s (s.v)}
            <label class="flex items-center gap-1.5"><input type="radio" name="scope" class="accent-[var(--accent)]" value={s.v} bind:group={scope} /> {s.l}</label>
          {/each}
        </div>
        {#if scope === 'one' && seriesChanged}
          <p class="mt-2 text-xs text-muted" role="status">The link, place, attendees, group, type and options can't differ for one event; they'll be saved for every event in the series.</p>
        {/if}
      </fieldset>
    {/if}
  </div>

  {#snippet footer()}
    {#if editing}
      <button class="btn btn-danger" onclick={() => void remove()}><Trash2 size={14} /> Delete</button>
    {/if}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving || !loaded} onclick={() => void save()}>{editing ? 'Save' : 'Add'}</button>
  {/snippet}
</Modal>

<style>
  .seg {
    height: 26px;
    padding-inline: 0.7rem;
    border-radius: 5px;
    color: var(--text-muted);
  }
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
</style>
