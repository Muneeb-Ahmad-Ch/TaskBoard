<script lang="ts">
  // Reminders: what will remind you, day by day (timed tasks, meetings, and
  // the coming repeats of repeating tasks), and every repeating series with
  // its next date. Click anything to open it.
  import { Bell, BellOff, CalendarClock, CheckSquare, ExternalLink, Plus, Repeat, Timer, Video } from '@lucide/svelte'
  import type { ReminderItem } from '../lib/bindings/ReminderItem'
  import type { ReminderKind } from '../lib/bindings/ReminderKind'
  import type { RepeatingItem } from '../lib/bindings/RepeatingItem'
  import type { UpcomingReminders } from '../lib/bindings/UpcomingReminders'
  import { addDays, cal, sameDay, startOfDay } from '../lib/cal.svelte'
  import { swatch } from '../lib/format'
  import { errorMessage, inTauri, ipc, openLink } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'
  import { segmented } from '../lib/segmented'

  const TAB_KEY = 'taskboard.reminders.tab'
  const RANGES = [
    { days: 7, label: 'Next 7 days' },
    { days: 30, label: 'Next 30 days' },
    { days: 90, label: 'Next 3 months' },
    { days: 366, label: 'Next 12 months' },
  ]
  const KIND_LABEL: Record<ReminderKind, string> = { task: 'Tasks', meeting: 'Meetings', block: 'Time blocks' }

  function savedTab(): 'upcoming' | 'repeating' {
    try {
      return localStorage.getItem(TAB_KEY) === 'repeating' ? 'repeating' : 'upcoming'
    } catch {
      return 'upcoming'
    }
  }
  let tab = $state(savedTab())
  $effect(() => {
    try {
      localStorage.setItem(TAB_KEY, tab)
    } catch {
      // per-device convenience only
    }
  })

  let days = $state(30)
  let kinds = $state<ReminderKind[]>(['task', 'meeting'])
  let upcoming = $state<UpcomingReminders | null>(null)
  let repeating = $state<RepeatingItem[]>([])
  let loaded = $state(false)
  let error = $state('')
  /** Re-renders "now" once a minute. */
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    void app.dataVersion
    const query = { days, kinds: [...kinds] }
    if (!inTauri) return
    let stale = false
    Promise.all([ipc.upcomingReminders(query), ipc.repeating()])
      .then(([u, r]) => {
        if (stale) return
        upcoming = u
        repeating = r
        error = ''
        loaded = true
      })
      .catch((err) => {
        if (!stale) error = errorMessage(err)
      })
    return () => {
      stale = true
    }
  })

  function toggleKind(k: ReminderKind) {
    const next = kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k]
    kinds = next.length ? next : [k]
  }

  // ---------- coming up, grouped by day ----------
  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'long', day: 'numeric', month: 'short' })
  const shortFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short' })
  const timeFmt = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' })

  function parseDay(iso: string): Date {
    const [y, m, d] = iso.split('-').map(Number)
    return new Date(y, m - 1, d)
  }

  function dayTitle(iso: string): string {
    const d = parseDay(iso)
    const today = startOfDay(new Date(now))
    if (sameDay(d, today)) return `Today · ${dayFmt.format(d)}`
    if (sameDay(d, addDays(today, 1))) return `Tomorrow · ${dayFmt.format(d)}`
    return dayFmt.format(d)
  }

  const groups = $derived.by(() => {
    const out: { key: string; title: string; items: ReminderItem[] }[] = []
    for (const i of upcoming?.items ?? []) {
      const key = i.overdue ? 'overdue' : i.date
      const last = out[out.length - 1]
      if (last?.key === key) last.items.push(i)
      else out.push({ key, title: i.overdue ? 'Overdue' : dayTitle(i.date), items: [i] })
    }
    return out
  })

  function remindText(i: ReminderItem): string {
    if (i.remind_at === null) return ''
    const at = new Date(i.remind_at)
    const atStart = i.remind_at === i.at
    return atStart ? `Reminds at ${timeFmt.format(at)}` : `Reminds ${Math.round((i.at - i.remind_at) / 60_000)} min before`
  }

  /** A task opens in the task panel; a meeting or block in its dialog (that occurrence). */
  function openItem(i: { kind: ReminderKind; title: string; task_id: string | null; event_id: string | null; original_start: number | null; start: number | null; end: number | null; allDay: boolean }) {
    if (i.kind === 'task') {
      if (i.task_id) app.openTaskId = i.task_id
      return
    }
    if (!i.event_id || i.start === null) return
    cal.draft = {
      id: i.event_id,
      originalStart: i.original_start ?? undefined,
      kind: i.kind === 'block' ? 'block' : 'meeting',
      start: i.start,
      end: i.end ?? i.start + 1_800_000,
      allDay: i.allDay,
      title: i.title,
    }
  }

  const openUpcoming = (i: ReminderItem) => openItem({ ...i, start: i.at, end: i.end_at, allDay: i.kind !== 'task' && i.time === null })
  const openRepeating = (r: RepeatingItem) => openItem({ ...r, start: r.next_at, end: r.next_end_at, allDay: r.all_day })

  // ---------- repeating ----------
  const repeatingGroups = $derived(
    (['task', 'meeting', 'block'] as ReminderKind[])
      .map((k) => ({ kind: k, items: repeating.filter((r) => r.kind === k) }))
      .filter((g) => g.items.length),
  )

  function nextText(r: RepeatingItem): string {
    if (!r.next_date) return 'No date yet'
    const d = parseDay(r.next_date)
    const today = startOfDay(new Date(now))
    const when = sameDay(d, today) ? 'Today' : sameDay(d, addDays(today, 1)) ? 'Tomorrow' : shortFmt.format(d)
    return r.next_time ? `${when} · ${r.next_time}` : when
  }

  async function stopRepeating(r: RepeatingItem) {
    if (!r.task_id) return
    await act(() => ipc.updateTask(r.task_id!, { recurrence_rule: null }), `“${r.title}” no longer repeats`)
  }

  const reminderSettings = () => {
    app.settingsSection = 'reminders'
    app.settingsOpen = true
  }

  const iconFor = { task: CheckSquare, meeting: Video, block: Timer }

  /** Said by screen readers after each load or filter change. */
  const status = $derived.by(() => {
    if (!loaded || !upcoming) return ''
    if (tab === 'repeating') return `${repeating.length} repeating`
    const n = upcoming.items.length
    return `${n} ${n === 1 ? 'reminder' : 'reminders'} in the ${RANGES.find((r) => r.days === days)?.label.toLowerCase() ?? ''}`
  })
</script>

<div class="mx-auto flex min-h-0 w-full max-w-[1180px] flex-1 flex-col gap-3">
  <div class="flex min-h-8 shrink-0 flex-wrap items-center gap-2">
    <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="tablist" aria-label="Reminders" use:segmented>
      <button role="tab" id="rem-tab-upcoming" aria-controls="rem-panel" aria-selected={tab === 'upcoming'} class="seg" onclick={() => (tab = 'upcoming')}>Coming up</button>
      <button role="tab" id="rem-tab-repeating" aria-controls="rem-panel" aria-selected={tab === 'repeating'} class="seg" onclick={() => (tab = 'repeating')}>
        Repeating{#if repeating.length}<span class="tabular ml-1 text-subtle">{repeating.length}</span>{/if}
      </button>
    </div>
    {#if tab === 'upcoming'}
      <select class="h-7 rounded-md border border-border bg-surface-2 px-2 text-[12.5px]" aria-label="How far ahead" bind:value={days}>
        {#each RANGES as r (r.days)}<option value={r.days}>{r.label}</option>{/each}
      </select>
      <div class="flex gap-1" role="group" aria-label="Show">
        {#each ['task', 'meeting', 'block'] as ReminderKind[] as k (k)}
          <button class="chip" aria-pressed={kinds.includes(k)} onclick={() => toggleKind(k)}>{KIND_LABEL[k]}</button>
        {/each}
      </div>
    {/if}
    <span class="flex-1"></span>
    {#if upcoming}
      <button class="status" onclick={reminderSettings} title="Change in Settings → Reminders">
        {#if upcoming.reminders_on}
          <Bell size={13} class="shrink-0" />
          <span class="max-[1000px]:hidden">Meetings {upcoming.meeting_lead_min} min before · tasks {upcoming.task_lead_min ? `${upcoming.task_lead_min} min before` : 'at their time'}</span>
        {:else}
          <BellOff size={13} class="shrink-0 text-danger" />
          <span>Notifications are off</span>
        {/if}
      </button>
    {/if}
    <button class="tb primary" title="New reminder" onclick={() => (app.reminderDialog = true)}><Plus size={14} /> <span class="max-[820px]:sr-only">Reminder</span></button>
  </div>

  <p class="sr-only" role="status">{status}</p>
  <div class="min-h-0 flex-1 overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-2" role="tabpanel" id="rem-panel" aria-labelledby="rem-tab-{tab}">
    {#if error}
      <p class="p-6 text-danger">{error}</p>
    {:else if !loaded}
      <p class="p-6 text-muted">Loading…</p>
    {:else if tab === 'upcoming'}
      {#if !groups.length}
        <div class="grid h-full place-items-center p-8 text-center">
          <div class="max-w-[380px]">
            <CalendarClock size={28} class="mx-auto mb-2 text-subtle" />
            <p class="font-medium">No reminders in the {RANGES.find((r) => r.days === days)?.label.toLowerCase()}</p>
            <p class="text-muted">A task with a due time reminds you at that time; a repeating one shows its next dates here. Add one with “Reminder”, or type “Pay rent every month 9am” in quick add.</p>
          </div>
        </div>
      {:else}
        {#each groups as g (g.key)}
          <section aria-label={g.title}>
            <h3 class="sticky top-0 z-[1] border-b border-border bg-surface-2 px-4 py-2 text-xs font-semibold tracking-wide uppercase {g.key === 'overdue' ? 'text-danger' : 'text-muted'}">{g.title}</h3>
            <ul>
              {#each g.items as i (i.key)}
                {@const Icon = iconFor[i.kind]}
                <li class="flex items-center gap-3 border-b border-border px-4 py-2.5 last:border-b-0 hover:bg-hover" class:projected={i.projected}>
                  <span class="tabular w-[64px] shrink-0 text-[13px] {i.overdue ? 'font-medium text-danger' : 'text-muted'}">{i.time ?? 'Any time'}</span>
                  <span class="h-8 w-1 shrink-0 rounded-full" style="background: {swatch(i.color)}"></span>
                  <button class="flex min-w-0 flex-1 flex-col items-start text-left" onclick={() => openUpcoming(i)} aria-label="Open {i.title}{i.projected ? ' (the current one)' : ''}">
                    <span class="flex w-full min-w-0 items-center gap-1.5">
                      <Icon size={13} class="shrink-0 text-subtle" aria-hidden="true" />
                      <span class="truncate font-medium">{i.title}</span>
                      {#if i.repeat}<span class="badge" title={i.repeat}><Repeat size={11} class="shrink-0" /><span class="truncate">{i.repeat}</span></span>{/if}
                    </span>
                    <span class="flex w-full min-w-0 items-center gap-2 text-xs text-muted">
                      {#if i.client_name || i.project_name}<span class="truncate">{[i.client_name, i.project_name].filter(Boolean).join(' / ')}</span>{/if}
                      {#if i.projected}<span class="shrink-0 text-subtle">Next repeat · added when the one before is done</span>
                      {:else if i.remind_at !== null}<span class="flex shrink-0 items-center gap-1"><Bell size={11} />{remindText(i)}</span>{/if}
                    </span>
                  </button>
                  {#if i.meeting_url && i.end_at && i.end_at > now}
                    <button class="join" onclick={() => void openLink(i.meeting_url!)} title={i.meeting_url}><ExternalLink size={12} /> Join</button>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/each}
        {#if upcoming?.truncated}
          <p class="px-4 py-3 text-xs text-muted">Too many to show them all: this list stops at {dayTitle(upcoming.to)}. Choose a shorter range, or hide Meetings or Time blocks, to see the rest.</p>
        {/if}
      {/if}
    {:else if !repeating.length}
      <div class="grid h-full place-items-center p-8 text-center">
        <div class="max-w-[380px]">
          <Repeat size={28} class="mx-auto mb-2 text-subtle" />
          <p class="font-medium">Nothing repeats yet</p>
          <p class="text-muted">Make a reminder repeat (every day, week, month or year), or type “every month” in quick add. Repeating meetings show here too.</p>
        </div>
      </div>
    {:else}
      {#each repeatingGroups as g (g.kind)}
        <section aria-label={KIND_LABEL[g.kind]}>
          <h3 class="sticky top-0 z-[1] border-b border-border bg-surface-2 px-4 py-2 text-xs font-semibold tracking-wide text-muted uppercase">{KIND_LABEL[g.kind]} <span class="tabular font-normal text-subtle">{g.items.length}</span></h3>
          <ul>
            {#each g.items as r ((r.task_id ?? r.event_id) + r.kind)}
              <li class="flex items-center gap-3 border-b border-border px-4 py-2.5 last:border-b-0 hover:bg-hover">
                <span class="h-8 w-1 shrink-0 rounded-full" style="background: {swatch(r.color)}"></span>
                <button class="flex min-w-0 flex-1 flex-col items-start text-left" onclick={() => openRepeating(r)} aria-label="Open {r.title}">
                  <span class="truncate font-medium">{r.title}</span>
                  <span class="flex w-full min-w-0 items-center gap-2 text-xs text-muted">
                    <span class="flex min-w-0 items-center gap-1"><Repeat size={11} class="shrink-0" /><span class="truncate">{r.describe}</span></span>
                    {#if r.client_name || r.project_name}<span class="truncate text-subtle">{[r.client_name, r.project_name].filter(Boolean).join(' / ')}</span>{/if}
                  </span>
                </button>
                <span class="tabular w-[150px] shrink-0 text-right text-[13px] text-muted max-[720px]:hidden" title="Next">{nextText(r)}</span>
                {#if r.kind === 'task'}
                  <button class="tb" title="Keep this task, stop making new ones" onclick={() => void stopRepeating(r)}>Stop repeating</button>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
</div>

<style>
  .tb {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
    height: 28px;
    padding-inline: 0.65rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .tb:hover {
    color: var(--text);
  }
  .tb.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 500;
  }
  .tb.primary:hover {
    background: var(--accent-hover);
  }
  .seg {
    height: 22px;
    padding-inline: 0.6rem;
    border-radius: 4px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .seg[aria-selected='true'] {
    background: var(--surface-3);
    color: var(--text);
  }
  .chip {
    height: 26px;
    padding-inline: 0.6rem;
    border: 1px solid var(--border);
    border-radius: 99px;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .chip:hover {
    color: var(--text);
  }
  .chip[aria-pressed='true'] {
    color: var(--text);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .status {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.4rem;
    height: 28px;
    padding-inline: 0.5rem;
    border-radius: 6px;
    color: var(--text-muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .status:hover {
    color: var(--text);
    background: var(--hover);
  }
  .badge {
    display: inline-flex;
    min-width: 0;
    max-width: 240px;
    flex-shrink: 1;
    align-items: center;
    gap: 0.25rem;
    padding: 0 6px;
    border-radius: 99px;
    background: var(--surface-3);
    color: var(--text-muted);
    font-size: 11px;
  }
  /* A future repeat: there, but quieter. */
  .projected .font-medium {
    font-weight: 400;
    color: var(--text-muted);
  }
  .join {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
    height: 28px;
    padding-inline: 0.65rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
  }
  .join:hover {
    border-color: var(--border-strong);
  }
</style>
