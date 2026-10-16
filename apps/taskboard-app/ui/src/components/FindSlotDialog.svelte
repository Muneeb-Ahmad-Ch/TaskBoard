<script lang="ts">
  import { Copy, Globe } from '@lucide/svelte'
  import type { SlotResult } from '../lib/bindings/SlotResult'
  import { availabilityText, cal, fmtRange, localZone, timeIn, zoneLabel } from '../lib/cal.svelte'
  import { isoDate } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { app } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Modal from './Modal.svelte'

  let { onclose }: { onclose: () => void } = $props()

  /** Next `n` working days from tomorrow (Mon–Fri), as the default range. */
  function workdaysAhead(n: number): [string, string] {
    const d = new Date()
    let first: Date | null = null
    let count = 0
    while (count < n) {
      d.setDate(d.getDate() + 1)
      if (d.getDay() !== 0 && d.getDay() !== 6) {
        first ??= new Date(d)
        count++
      }
    }
    return [isoDate(first ?? d), isoDate(d)]
  }
  const [defFrom, defTo] = workdaysAhead(5)

  const q = $state({
    duration: 30,
    from: defFrom,
    to: defTo,
    clientId: '',
    buffer: 10,
    earliest: '',
    latest: '',
    maxPerDay: '',
    notice: 120,
    blocksFlexible: true,
  })
  let result = $state<SlotResult | null>(null)
  let loading = $state(false)

  const client = $derived(app.clients.find((c) => c.id === q.clientId))
  const clientZone = $derived(result?.client_time_zone ?? null)
  const myZone = localZone()

  async function search() {
    loading = true
    try {
      result = await ipc.findSlots({
        from_date: q.from,
        to_date: q.to,
        duration_min: q.duration,
        buffer_min: q.buffer,
        earliest: q.earliest || undefined,
        latest: q.latest || undefined,
        blocks_flexible: q.blocksFlexible,
        max_meetings_per_day: q.maxPerDay ? Number(q.maxPerDay) : undefined,
        min_notice_min: q.notice,
        client_id: q.clientId || undefined,
      })
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    } finally {
      loading = false
    }
  }

  // Re-run whenever an option changes.
  $effect(() => {
    void JSON.stringify(q)
    const t = setTimeout(() => void search(), 150)
    return () => clearTimeout(t)
  })

  const found = $derived(result?.days.filter((d) => d.windows.length) ?? [])

  async function copy(mode: 'mine' | 'theirs' | 'both') {
    if (!result) return
    const zones = mode === 'mine' || !clientZone ? [myZone] : mode === 'theirs' ? [clientZone] : [myZone, clientZone]
    const text = availabilityText(result.days, zones)
    try {
      await navigator.clipboard.writeText(text)
      toast('Availability copied')
    } catch {
      toast('Could not copy to the clipboard', { kind: 'error' })
    }
  }

  function book(start: number, end: number) {
    cal.findSlotOpen = false
    cal.draft = { kind: 'meeting', start, end, clientId: q.clientId || null, title: client ? `${client.name} meeting` : '' }
  }

  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'long', day: 'numeric', month: 'short' })
  const DURATIONS = [15, 30, 45, 60, 90]
</script>

<Modal title="Find a meeting slot" {onclose} width={680}>
  <div class="grid grid-cols-[1fr_auto] gap-4">
    <div class="flex flex-col gap-3">
      <div>
        <span class="field-label">Length</span>
        <div class="flex flex-wrap gap-1">
          {#each DURATIONS as d (d)}
            <button class="pill" aria-pressed={q.duration === d} onclick={() => (q.duration = d)}>{d < 60 ? `${d} min` : `${d / 60} h`.replace('1.5 h', '1½ h')}</button>
          {/each}
          <input type="number" class="field !h-7 !w-20" min="5" max="480" step="5" aria-label="Custom length in minutes" bind:value={q.duration} />
        </div>
      </div>
      <div class="grid grid-cols-3 gap-2">
        <label><span class="field-label">From</span><input type="date" class="field" bind:value={q.from} /></label>
        <label><span class="field-label">To</span><input type="date" class="field" bind:value={q.to} min={q.from} /></label>
        <label>
          <span class="field-label">With client</span>
          <select class="field" bind:value={q.clientId}>
            <option value="">Anyone</option>
            {#each app.clients as c (c.id)}<option value={c.id}>{c.name}{c.time_zone ? ` · ${c.time_zone.split('/').pop()?.replace('_', ' ')}` : ''}</option>{/each}
          </select>
        </label>
      </div>
      {#if client && !client.time_zone}
        <p class="text-xs text-warn">{client.name} has no time zone set, so only your hours are used. Set one in the client's settings.</p>
      {/if}
    </div>

    <div class="flex w-52 flex-col gap-2 text-[13px]">
      <label class="flex items-center justify-between gap-2">Buffer <span class="flex items-center gap-1"><input type="number" class="field !h-7 !w-16" min="0" max="60" step="5" bind:value={q.buffer} /> min</span></label>
      <label class="flex items-center justify-between gap-2">Not before <input type="time" class="field !h-7 !w-[104px]" bind:value={q.earliest} /></label>
      <label class="flex items-center justify-between gap-2">Not after <input type="time" class="field !h-7 !w-[104px]" bind:value={q.latest} /></label>
      <label class="flex items-center justify-between gap-2">Max meetings/day <input type="number" class="field !h-7 !w-16" min="1" max="20" placeholder="—" bind:value={q.maxPerDay} /></label>
      <label class="flex items-center justify-between gap-2">Notice <span class="flex items-center gap-1"><input type="number" class="field !h-7 !w-16" min="0" step="30" bind:value={q.notice} /> min</span></label>
      <label class="flex items-center gap-1.5"><input type="checkbox" class="accent-[var(--accent)]" bind:checked={q.blocksFlexible} /> Book over flexible blocks</label>
    </div>
  </div>

  <div class="mt-4 border-t border-border pt-3">
    <div class="mb-2 flex items-center gap-2 text-xs text-muted">
      <Globe size={13} />
      <span>You: {zoneLabel(myZone)}</span>
      {#if clientZone}<span>· {client?.name}: {zoneLabel(clientZone)}</span>{/if}
      <span class="flex-1"></span>
      {#if loading}<span>Searching…</span>{:else}<span>{found.length ? `${found.reduce((n, d) => n + d.windows.length, 0)} free windows` : 'No free time with these settings'}</span>{/if}
    </div>
    <div class="flex max-h-[38vh] flex-col gap-3 overflow-y-auto pr-1">
      {#each found as day (day.date)}
        <section>
          <h4 class="mb-1 text-[13px] font-semibold">{dayFmt.format(new Date(`${day.date}T00:00`))}</h4>
          <div class="flex flex-wrap gap-1.5">
            {#each day.slots as s (s.start_at)}
              <button class="slot" onclick={() => book(s.start_at, s.end_at)} title="Book this slot">
                <span class="font-medium">{fmtRange(s.start_at, s.end_at)}</span>
                {#if clientZone}<span class="text-[11px] text-muted">{timeIn(s.start_at, clientZone)} their time</span>{/if}
              </button>
            {/each}
          </div>
          <p class="mt-1 text-[11.5px] text-subtle">
            Free: {day.windows.map((w) => fmtRange(w.start_at, w.end_at)).join(', ')}
          </p>
        </section>
      {/each}
    </div>
  </div>

  {#snippet footer()}
    <span class="text-xs text-muted">Copy availability:</span>
    <button class="btn !h-7 text-xs" disabled={!found.length} onclick={() => void copy('mine')}><Copy size={13} /> My time</button>
    {#if clientZone}
      <button class="btn !h-7 text-xs" disabled={!found.length} onclick={() => void copy('theirs')}>Their time</button>
      <button class="btn !h-7 text-xs" disabled={!found.length} onclick={() => void copy('both')}>Both</button>
    {/if}
    <span class="flex-1"></span>
    <button class="btn" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<style>
  .pill {
    height: 28px;
    padding-inline: 0.7rem;
    border-radius: 99px;
    border: 1px solid var(--border);
  }
  .pill[aria-pressed='true'] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-soft-text);
  }
  .slot {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 4px 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-1);
    font-size: 12.5px;
  }
  .slot:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
</style>
