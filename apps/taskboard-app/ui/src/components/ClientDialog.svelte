<script lang="ts">
  import { untrack } from 'svelte'
  import type { Id } from '../lib/bindings/Id'
  import { zoneLabel } from '../lib/cal.svelte'
  import { cap, PALETTE, swatch } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, app, itemName, sectionById } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'
  import Notes from './Notes.svelte'

  let { id, onclose }: { id: Id; onclose: () => void } = $props()

  const client = untrack(() => app.clients.find((c) => c.id === id))
  const item = untrack(() => itemName(sectionById(client?.section_id)))
  const f = $state({
    name: client?.name ?? '',
    color: client?.color ?? '262',
    contact: client?.contact ?? '',
    notes: client?.notes_md ?? '',
    timeZone: client?.time_zone ?? '',
    workStart: client?.work_start ?? '09:00',
    workEnd: client?.work_end ?? '17:00',
    counts: client?.meetings_count_as_time ?? true,
    section: client?.section_id ?? '',
  })
  /** Time zone, hours and contact; open when any is set. */
  const detailsOpen = untrack(() => !!(f.timeZone || f.contact))
  let saving = $state(false)

  // Every IANA zone the WebView knows, for the picker.
  const zones: string[] = (() => {
    try {
      return Intl.supportedValuesOf('timeZone')
    } catch {
      return []
    }
  })()
  const zoneValid = $derived(!f.timeZone || zones.includes(f.timeZone))

  async function save() {
    if (!f.name.trim()) return toast(`Give the ${item} a name`, { kind: 'error' })
    if (!zoneValid) return toast(`Unknown time zone “${f.timeZone}”`, { kind: 'error' })
    saving = true
    try {
      await ipc.updateClient(id, {
        name: f.name,
        color: f.color,
        contact: f.contact,
        notes_md: f.notes,
        time_zone: f.timeZone || null,
        work_start: f.timeZone ? f.workStart : null,
        work_end: f.timeZone ? f.workEnd : null,
        meetings_count_as_time: f.counts,
        section_id: f.section && f.section !== client?.section_id ? f.section : undefined,
      })
      toast(`${cap(item)} saved`, { action: { label: 'Undo', run: () => void ipc.undo() } })
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }
</script>

<Modal title="{cap(item)} details" {onclose} width={540}>
  {#if !client}
    <p class="text-muted">This {item} no longer exists.</p>
  {:else}
    <div class="flex flex-col gap-3">
      <div class="grid grid-cols-[1fr_180px] gap-2">
        <label><span class="field-label">Name</span><input class="field" bind:value={f.name} /></label>
        <label>
          <span class="field-label">Section</span>
          <select class="field" bind:value={f.section}>
            {#each app.sections as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
          </select>
        </label>
      </div>

      <div>
        <span class="field-label">Color</span>
        <div class="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Color">
          {#each PALETTE as c (c)}
            <button role="radio" aria-checked={f.color === c} aria-label={c === 'gray' ? 'Gray' : `Hue ${c}`} class="swatch" style="--s: {swatch(c, 0.6)}" onclick={() => (f.color = c)}></button>
          {/each}
        </div>
      </div>

      <details class="details" open={detailsOpen}>
        <summary class="cursor-pointer text-[13px] font-medium">Details <span class="font-normal text-subtle">(time zone, working hours, contact)</span></summary>
        <div class="mt-3 flex flex-col gap-3">
      <div class="grid grid-cols-[1fr_auto_auto] items-end gap-2">
        <label>
          <span class="field-label">Time zone <span class="text-subtle">(for finding meeting slots)</span></span>
          <input class="field" list="tz-list" placeholder="e.g. Europe/London" bind:value={f.timeZone} class:invalid={!zoneValid} />
          <datalist id="tz-list">{#each zones as z (z)}<option value={z}></option>{/each}</datalist>
        </label>
        <label><span class="field-label">Their day starts</span><input type="time" class="field w-[110px]" bind:value={f.workStart} disabled={!f.timeZone} /></label>
        <label><span class="field-label">Ends</span><input type="time" class="field w-[110px]" bind:value={f.workEnd} disabled={!f.timeZone} /></label>
      </div>
      {#if f.timeZone && zoneValid}
        <p class="-mt-1 text-xs text-muted">{zoneLabel(f.timeZone)} · now {new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit', timeZone: f.timeZone }).format(Date.now())} there</p>
      {/if}

      <label><span class="field-label">Contact</span><input class="field" placeholder="Name, email, phone" bind:value={f.contact} /></label>
        </div>
      </details>
      <div><span class="field-label">Notes</span><Notes value={f.notes} rows={4} placeholder="Preferences, contacts, access details… (Markdown)" onsave={(v) => (f.notes = v)} /></div>
      <label class="flex items-center gap-1.5 text-[13px]">
        <input type="checkbox" class="accent-[var(--accent)]" bind:checked={f.counts} />
        Meetings with this {item} count as work time
      </label>
    </div>
  {/if}

  {#snippet footer()}
    {#if client}
      <button class="btn" onclick={() => void act(() => ipc.setClientArchived(id, true), `Archived “${client.name}”`).then(onclose)}>Archive</button>
    {/if}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving || !client} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>

<style>
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 99px;
    background: var(--s);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface-2),
      0 0 0 4px var(--s);
  }
  .invalid {
    border-color: var(--danger);
  }
</style>
