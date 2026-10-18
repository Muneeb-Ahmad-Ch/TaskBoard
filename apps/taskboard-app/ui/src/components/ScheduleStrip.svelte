<script lang="ts">
  // Above the task list: the meetings and time blocks coming up for the open
  // list (a group, project, section, Today…), so a client's calls and the
  // time booked for its work sit next to its tasks. Today shows the rest of
  // today; other lists the next 7 days. Collapsible; hidden when there is
  // nothing to show, except on a group or project, which offers to add one.
  import { CalendarClock, CalendarRange, ChevronRight, ExternalLink, Plus, Repeat, SquareCheck, Timer, Video } from '@lucide/svelte'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import { cal, fmtTime, startOfDay } from '../lib/cal.svelte'
  import { plural, swatch } from '../lib/format'
  import { inTauri, ipc, openLink } from '../lib/ipc'
  import { app } from '../lib/store.svelte'

  const KEY = 'taskboard.schedule.open'
  const DAY = 86_400_000
  let open = $state(loadOpen())
  let items = $state<Occurrence[]>([])
  let now = $state(Date.now())

  /** Remembered when set; otherwise open, except in short windows where the
   *  list needs the height. */
  function loadOpen(): boolean {
    let saved: string | null = null
    try {
      saved = localStorage.getItem(KEY)
    } catch {
      // per-device convenience only
    }
    return saved === null ? window.innerHeight >= 640 : saved !== 'false'
  }
  function toggle() {
    open = !open
    try {
      localStorage.setItem(KEY, String(open))
    } catch {
      // per-device convenience only
    }
  }

  const kind = $derived(app.scope.kind)
  const shown = $derived(kind !== 'inbox')
  const owned = $derived(kind === 'client' || kind === 'project')

  function inScope(o: Occurrence): boolean {
    const s = app.scope
    if (s.kind === 'client') return o.client_id === s.id
    if (s.kind === 'project') return o.project_id === s.id
    if (s.kind === 'section') {
      const project = o.project_id ? app.projects.find((p) => p.id === o.project_id) : undefined
      const group = o.client_id ? app.clients.find((c) => c.id === o.client_id) : undefined
      return (project?.section_id ?? group?.section_id) === s.id
    }
    return true
  }

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    void app.dataVersion
    void JSON.stringify(app.scope)
    if (!inTauri || !shown) return
    const from = Date.now()
    const to = kind === 'today' ? startOfDay(new Date()).getTime() + DAY : from + 7 * DAY
    let stale = false
    ipc
      .occurrences(from, to)
      .then((list) => {
        if (stale) return
        items = list.filter((o) => o.status !== 'cancelled' && o.end_at > from && inScope(o)).sort((a, b) => a.start_at - b.start_at)
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  const upcoming = $derived(items.filter((o) => o.end_at > now))
  const meetings = $derived(upcoming.filter((o) => o.kind === 'meeting').length)
  const blocks = $derived(upcoming.length - meetings)
  const summary = $derived(
    [meetings && plural(meetings, 'meeting'), blocks && plural(blocks, 'time block')].filter(Boolean).join(', ') +
      (kind === 'today' ? ' left today' : ' in the next 7 days'),
  )

  const weekday = new Intl.DateTimeFormat(undefined, { weekday: 'short' })
  function when(o: Occurrence): string {
    const today = startOfDay(new Date()).getTime()
    const start = startOfDay(new Date(o.start_at)).getTime()
    const day = start <= today ? (o.start_at < today ? 'Now' : 'Today') : start === today + DAY ? 'Tomorrow' : weekday.format(o.start_at)
    if (o.all_day) return `${day} · all day`
    if (o.start_at <= now && o.end_at > now) return `Now · until ${fmtTime(o.end_at)}`
    return `${day} · ${fmtTime(o.start_at)}`
  }

  function openEvent(o: Occurrence) {
    cal.draft = { id: o.event_id, originalStart: o.recurring ? o.original_start : undefined, kind: o.kind, start: o.start_at, end: o.end_at, allDay: o.all_day, title: o.title }
  }

  /** A new meeting or block at the next full hour, filed under this group or project. */
  function add(k: 'meeting' | 'block') {
    const s = new Date()
    s.setMinutes(0, 0, 0)
    s.setHours(s.getHours() + 1)
    const scope = app.scope
    cal.draft = {
      kind: k,
      start: s.getTime(),
      end: s.getTime() + 3_600_000,
      clientId: scope.kind === 'client' ? scope.id : null,
      projectId: scope.kind === 'project' ? scope.id : null,
    }
  }

  function openCalendar() {
    cal.mode = 'agenda'
    cal.anchor = startOfDay(new Date())
    app.view = 'calendar'
  }
</script>

{#if shown && (upcoming.length || owned)}
  <section class="strip" aria-label="Schedule">
    <div class="flex h-8 items-center gap-1 pr-1">
      <button class="head" aria-expanded={open && upcoming.length > 0} disabled={!upcoming.length} onclick={toggle}>
        {#if upcoming.length}<ChevronRight size={14} class="chev {open ? 'open' : ''}" />{/if}
        <CalendarClock size={14} class="shrink-0 text-muted" />
        <span class="font-medium">Schedule</span>
        <span class="truncate text-muted">{upcoming.length ? summary : 'Nothing booked in the next 7 days'}</span>
      </button>
      <span class="ml-auto flex shrink-0 items-center gap-0.5">
        <button class="mini" title="New meeting{owned ? ' here' : ''}" onclick={() => add('meeting')}><Plus size={13} /><span class="max-[1100px]:sr-only">Meeting</span></button>
        <button class="mini" title="New time block{owned ? ' here' : ''}" onclick={() => add('block')}><Plus size={13} /><span class="max-[1100px]:sr-only">Time block</span></button>
        <button class="mini" title="Open the calendar's agenda for this list" onclick={openCalendar}><CalendarRange size={13} /><span class="max-[1100px]:sr-only">Calendar</span></button>
      </span>
    </div>
    {#if open && upcoming.length}
      <ul class="items" aria-label="Coming up">
        {#each upcoming as o (`${o.event_id}-${o.original_start}`)}
          {@const live = o.start_at <= now && o.end_at > now}
          <li class="item" class:live>
            <span class="rail" style="background: {swatch(o.color, 0.62, 0.14)}"></span>
            <button class="body" title="{o.title}{o.client_name ? ` · ${o.client_name}` : ''}{o.project_name ? ` / ${o.project_name}` : ''}" onclick={() => openEvent(o)}>
              <span class="tabular text-[11.5px] {live ? 'font-semibold text-accent' : 'text-muted'}">{when(o)}</span>
              <span class="flex min-w-0 items-center gap-1 text-[12.5px]">
                {#if o.kind === 'meeting'}<Video size={12} class="shrink-0 text-subtle" />{:else}<Timer size={12} class="shrink-0 text-subtle" />{/if}
                <span class="truncate font-medium">{o.title}</span>
                {#if o.recurring}<Repeat size={11} class="shrink-0 text-subtle" aria-label="Repeats" />{/if}
              </span>
            </button>
            {#if o.task_id}
              <button class="side" title="Open the task this block is for" aria-label="Open task for {o.title}" onclick={() => (app.openTaskId = o.task_id)}><SquareCheck size={13} /></button>
            {:else if o.meeting_url}
              <button class="side" title="Join (opens the meeting link)" aria-label="Join {o.title}" onclick={() => void openLink(o.meeting_url)}><ExternalLink size={13} /></button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .strip {
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .head {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding-inline: 10px;
    font-size: 12.5px;
    text-align: left;
  }
  .head:disabled {
    cursor: default;
  }
  .head :global(.chev) {
    flex-shrink: 0;
    color: var(--text-subtle);
    transition: transform 120ms var(--ease-out);
  }
  .head :global(.chev.open) {
    transform: rotate(90deg);
  }
  .mini {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 24px;
    padding-inline: 6px;
    border-radius: 5px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .mini:hover {
    background: var(--hover);
    color: var(--text);
  }
  .items {
    display: flex;
    gap: 8px;
    padding: 0 10px 10px;
    overflow-x: auto;
    scrollbar-width: thin;
  }
  .item {
    position: relative;
    display: flex;
    flex: 0 0 auto;
    width: 212px;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface-1);
    overflow: hidden;
  }
  .item.live {
    border-color: color-mix(in oklch, var(--accent) 55%, var(--border));
  }
  .rail {
    width: 3px;
    flex-shrink: 0;
  }
  .body {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    padding: 6px 8px;
    text-align: left;
  }
  .body:hover,
  .side:hover {
    background: var(--hover);
  }
  .side {
    display: grid;
    place-items: center;
    width: 30px;
    flex-shrink: 0;
    color: var(--text-muted);
    border-left: 1px solid var(--border);
  }
  .side:hover {
    color: var(--text);
  }
  /* Short windows: one-line chips, so the list keeps its rows. */
  @media (max-height: 760px) {
    .body {
      flex-direction: row;
      align-items: center;
      gap: 6px;
      padding-block: 4px;
    }
    .item {
      width: 260px;
    }
  }
</style>
