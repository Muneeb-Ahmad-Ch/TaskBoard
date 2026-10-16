<script lang="ts">
  import { Plus, Trash2, X } from '@lucide/svelte'
  import type { DayOverride } from '../lib/bindings/DayOverride'
  import type { WorkingDay } from '../lib/bindings/WorkingDay'
  import { isoDate } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  const NAMES = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday']
  let week = $state<WorkingDay[]>([])
  let overrides = $state<DayOverride[]>([])
  let newOff = $state({ date: isoDate(new Date()), note: '', kind: 'off' as 'off' | 'custom', start: '09:00', end: '13:00' })
  let saving = $state(false)

  async function load() {
    try {
      week = await ipc.workingHours()
      const from = isoDate(new Date())
      const to = isoDate(new Date(Date.now() + 365 * 86_400_000))
      overrides = await ipc.dayOverrides(from, to)
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }
  $effect(() => {
    void load()
  })

  const total = $derived(
    week.reduce((sum, d) => sum + d.intervals.reduce((s, i) => s + minutesOf(i.end) - minutesOf(i.start), 0), 0),
  )
  function minutesOf(hm: string) {
    const [h, m] = hm.split(':').map(Number)
    return (h ?? 0) * 60 + (m ?? 0)
  }

  function copyToWeekdays(from: WorkingDay) {
    for (const d of week) if (d.weekday <= 5 && d !== from) d.intervals = from.intervals.map((i) => ({ ...i }))
  }

  async function save() {
    saving = true
    try {
      await ipc.setWorkingHours(week)
      toast('Working hours saved', { action: { label: 'Undo', run: () => void ipc.undo() } })
      onclose()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      saving = false
    }
  }

  async function addOverride() {
    try {
      await ipc.setDayOverride({
        date: newOff.date,
        kind: newOff.kind,
        intervals: newOff.kind === 'custom' ? [{ start: newOff.start, end: newOff.end }] : [],
        note: newOff.note,
      })
      newOff.note = ''
      await load()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  async function removeOverride(date: string) {
    try {
      await ipc.clearDayOverride(date)
      await load()
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  const dateFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short', year: 'numeric' })
</script>

<Modal title="Working hours" {onclose} width={620}>
  <p class="mb-3 text-[13px] text-muted">Used to shade the calendar, show daily capacity and find meeting slots. Add breaks as separate intervals.</p>
  <div class="flex flex-col gap-1">
    {#each week as day (day.weekday)}
      <div class="flex min-h-9 items-start gap-3 border-b border-border py-1.5 last:border-0">
        <span class="w-24 shrink-0 pt-1.5 text-[13px] font-medium">{NAMES[day.weekday - 1]}</span>
        <div class="flex flex-1 flex-col gap-1">
          {#each day.intervals as interval, i (i)}
            <div class="flex items-center gap-1.5">
              <input type="time" class="field !h-7 w-[108px]" step="900" bind:value={interval.start} />
              <span class="text-subtle">–</span>
              <input type="time" class="field !h-7 w-[108px]" step="900" bind:value={interval.end} />
              <button class="icon" aria-label="Remove interval" onclick={() => day.intervals.splice(i, 1)}><X size={14} /></button>
            </div>
          {:else}
            <span class="pt-1.5 text-[13px] text-subtle">Not working</span>
          {/each}
        </div>
        <div class="flex shrink-0 gap-1 pt-0.5">
          <button class="btn !h-7 !px-2 text-xs" onclick={() => day.intervals.push(day.intervals.length ? { start: '14:00', end: '18:00' } : { start: '09:00', end: '17:00' })}><Plus size={13} /> Add</button>
          {#if day.weekday <= 5 && day.intervals.length}
            <button class="btn !h-7 !px-2 text-xs" title="Copy these hours to Monday–Friday" onclick={() => copyToWeekdays(day)}>Copy to weekdays</button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
  <p class="mt-2 text-xs text-muted">{Math.round((total / 60) * 10) / 10} hours per week</p>

  <h3 class="mt-5 mb-2 text-[13px] font-semibold">Days off &amp; special days</h3>
  <div class="flex flex-wrap items-end gap-2">
    <label><span class="field-label">Date</span><input type="date" class="field !w-[150px]" bind:value={newOff.date} /></label>
    <label>
      <span class="field-label">Type</span>
      <select class="field !w-[130px]" bind:value={newOff.kind}>
        <option value="off">Day off</option>
        <option value="custom">Custom hours</option>
      </select>
    </label>
    {#if newOff.kind === 'custom'}
      <label><span class="field-label">From</span><input type="time" class="field !w-[104px]" bind:value={newOff.start} /></label>
      <label><span class="field-label">To</span><input type="time" class="field !w-[104px]" bind:value={newOff.end} /></label>
    {/if}
    <label class="min-w-32 flex-1"><span class="field-label">Note</span><input class="field" placeholder="Holiday, leave…" bind:value={newOff.note} /></label>
    <button class="btn" onclick={() => void addOverride()}><Plus size={14} /> Add</button>
  </div>
  <ul class="mt-2 flex flex-col">
    {#each overrides as o (o.date)}
      <li class="flex h-8 items-center gap-3 text-[13px]">
        <span class="w-40 tabular">{dateFmt.format(new Date(`${o.date}T00:00`))}</span>
        <span class="text-muted">{o.kind === 'off' ? 'Off' : o.intervals.map((i) => `${i.start}–${i.end}`).join(', ')}</span>
        <span class="flex-1 truncate text-subtle">{o.note}</span>
        <button class="icon" aria-label="Remove" onclick={() => void removeOverride(o.date)}><Trash2 size={14} /></button>
      </li>
    {:else}
      <li class="text-[13px] text-subtle">No upcoming days off.</li>
    {/each}
  </ul>

  {#snippet footer()}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={saving} onclick={() => void save()}>Save hours</button>
  {/snippet}
</Modal>

<style>
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 6px;
    color: var(--text-subtle);
  }
  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
</style>
