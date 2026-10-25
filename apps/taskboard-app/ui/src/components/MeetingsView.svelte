<script lang="ts">
  // All meetings in one list: upcoming (next 30 days) or past (last 30 days),
  // grouped by day, with Join links. Click a meeting to edit it.
  import { ExternalLink, MapPin, Plus, Repeat, Search, Video } from '@lucide/svelte'
  import type { Id } from '../lib/bindings/Id'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import { addDays, cal, fmtRange, newMeeting, sameDay, startOfDay } from '../lib/cal.svelte'
  import { swatch } from '../lib/format'
  import { errorMessage, ipc, openLink } from '../lib/ipc'
  import { app } from '../lib/store.svelte'
  import { segmented } from '../lib/segmented'

  const DAYS = 30
  let range = $state<'upcoming' | 'past'>('upcoming')
  let clientFilter = $state<Id | ''>('')
  let meetings = $state<Occurrence[]>([])
  let loaded = $state(false)
  let error = $state('')
  /** Re-renders "now" markers once a minute. */
  let now = $state(Date.now())

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    void app.dataVersion
    const past = range === 'past'
    const today = startOfDay(new Date())
    const from = past ? addDays(today, -DAYS).getTime() : today.getTime()
    const to = past ? Date.now() : addDays(today, DAYS + 1).getTime()
    let stale = false
    ipc
      .occurrences(from, to)
      .then((list) => {
        if (stale) return
        const only = list.filter((o) => o.kind === 'meeting' && (past ? o.end_at <= Date.now() : o.end_at > Date.now()))
        meetings = past ? only.sort((a, b) => b.start_at - a.start_at) : only.sort((a, b) => a.start_at - b.start_at)
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

  const shown = $derived(clientFilter ? meetings.filter((m) => m.client_id === clientFilter) : meetings)

  /** Meetings grouped by calendar day, in list order. */
  /** The one meeting whose Join is emphasized: live now, or the next to start. */
  const nextJoinable = $derived(
    range === 'upcoming'
      ? meetings
          .filter((m) => m.meeting_url && m.status !== 'cancelled' && m.end_at > now)
          .reduce<(typeof meetings)[number] | null>((a, b) => (a && a.start_at <= b.start_at ? a : b), null)
      : null,
  )

  const groups = $derived.by(() => {
    const out: { day: Date; items: Occurrence[] }[] = []
    for (const m of shown) {
      const day = startOfDay(new Date(m.start_at))
      const last = out[out.length - 1]
      if (last && sameDay(last.day, day)) last.items.push(m)
      else out.push({ day, items: [m] })
    }
    return out
  })

  const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'long', day: 'numeric', month: 'short' })
  function dayTitle(d: Date): string {
    const today = startOfDay(new Date())
    if (sameDay(d, today)) return `Today · ${dayFmt.format(d)}`
    if (sameDay(d, addDays(today, 1))) return `Tomorrow · ${dayFmt.format(d)}`
    if (sameDay(d, addDays(today, -1))) return `Yesterday · ${dayFmt.format(d)}`
    return dayFmt.format(d)
  }

  function open(o: Occurrence) {
    cal.draft = {
      id: o.event_id,
      originalStart: o.recurring ? o.original_start : undefined,
      kind: o.kind,
      start: o.start_at,
      end: o.end_at,
      allDay: o.all_day,
      title: o.title,
    }
  }

  const clientsWithMeetings = $derived(app.clients.filter((c) => meetings.some((m) => m.client_id === c.id)))
</script>

<!-- A readable column on very wide windows: Join stays next to its meeting. -->
<div class="mx-auto flex min-h-0 w-full max-w-[1180px] flex-1 flex-col gap-3">
  <div class="flex min-h-8 shrink-0 flex-wrap items-center gap-2">
    <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="radiogroup" aria-label="Which meetings" use:segmented>
      <button role="radio" aria-checked={range === 'upcoming'} class="seg" onclick={() => (range = 'upcoming')}>Upcoming</button>
      <button role="radio" aria-checked={range === 'past'} class="seg" onclick={() => (range = 'past')}>Past</button>
    </div>
    {#if clientsWithMeetings.length}
      <select class="h-7 rounded-md border border-border bg-surface-2 px-2 text-[12.5px]" aria-label="Group" bind:value={clientFilter}>
        <option value="">All groups</option>
        {#each clientsWithMeetings as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
      </select>
    {/if}
    <span class="flex-1"></span>
    <button class="tb" title="Find a meeting slot" onclick={() => (cal.findSlotOpen = true)}><Search size={14} /> <span class="max-[820px]:sr-only">Find slot</span></button>
    <button class="tb primary" title="New meeting" onclick={newMeeting}><Plus size={14} /> <span class="max-[820px]:sr-only">Meeting</span></button>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-2">
    {#if error}
      <p class="p-6 text-danger">{error}</p>
    {:else if loaded && !groups.length}
      <div class="grid h-full place-items-center p-8 text-center">
        <div>
          <Video size={28} class="mx-auto mb-2 text-subtle" />
          <p class="font-medium">{range === 'upcoming' ? 'No meetings in the next 30 days' : 'No meetings in the last 30 days'}</p>
          <p class="text-muted">{range === 'upcoming' ? 'Add one, or find a slot that suits everyone.' : ''}</p>
        </div>
      </div>
    {:else}
      {#each groups as g (g.day.getTime())}
        <section aria-label={dayTitle(g.day)}>
          <h3 class="day sticky top-0 z-[1] border-b border-border bg-surface-2 px-4 py-2 text-xs font-semibold tracking-wide text-muted uppercase">{dayTitle(g.day)}</h3>
          <ul>
            {#each g.items as m (m.event_id + ':' + m.original_start)}
              {@const live = m.start_at <= now && now < m.end_at}
              {@const cancelled = m.status === 'cancelled'}
              <li class="flex items-center gap-3 border-b border-border px-4 py-2.5 last:border-b-0 hover:bg-hover">
                <span class="tabular w-[118px] shrink-0 text-[13px] {live ? 'font-semibold text-accent' : 'text-muted'}">
                  {m.all_day ? 'All day' : fmtRange(m.start_at, m.end_at)}
                </span>
                <span class="h-8 w-1 shrink-0 rounded-full" style="background: {swatch(m.color ?? 'gray')}"></span>
                <button class="flex min-w-0 flex-1 flex-col items-start text-left" onclick={() => open(m)} aria-label="Edit {m.title}">
                  <span class="flex w-full min-w-0 items-center gap-1.5">
                    <span class="truncate font-medium {cancelled ? 'text-subtle line-through' : ''}">{m.title}</span>
                    {#if m.recurring}<Repeat size={12} class="shrink-0 text-subtle" aria-label="Repeats" />{/if}
                    {#if live}<span class="shrink-0 rounded-full bg-accent-soft px-1.5 text-[11px] font-semibold text-accent-soft-text">Now</span>{/if}
                    {#if cancelled}<span class="shrink-0 text-xs text-subtle">Cancelled</span>{/if}
                    {#if m.status === 'tentative'}<span class="shrink-0 text-xs text-subtle">Tentative</span>{/if}
                  </span>
                  <span class="flex w-full min-w-0 items-center gap-2 text-xs text-muted">
                    {#if m.client_name || m.project_name}<span class="truncate">{[m.client_name, m.project_name].filter(Boolean).join(' / ')}</span>{/if}
                    {#if m.location}<span class="flex min-w-0 items-center gap-1 truncate"><MapPin size={11} class="shrink-0" />{m.location}</span>{/if}
                  </span>
                </button>
                {#if m.meeting_url && !cancelled && range === 'upcoming' && m.end_at > now}
                  <button class="join" class:next={m === nextJoinable} onclick={() => void openLink(m.meeting_url!)} title={m.meeting_url}><ExternalLink size={12} /> Join</button>
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
    align-items: center;
    gap: 0.35rem;
    height: 28px;
    padding-inline: 0.65rem;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 12.5px;
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
  .seg[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--text);
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
  .join.next {
    border-color: transparent;
    background: var(--accent);
    color: var(--on-accent);
  }
  .join.next:hover {
    filter: brightness(1.06);
  }
</style>
