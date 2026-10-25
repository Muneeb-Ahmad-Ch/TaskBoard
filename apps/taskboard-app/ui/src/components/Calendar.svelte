<script lang="ts">
  import { untrack } from 'svelte'
  import { ChevronDown, ChevronLeft, ChevronRight, Clock, Layers, Plus, Search, Square, Timer } from '@lucide/svelte'
  import type { ClientHours } from '../lib/bindings/ClientHours'
  import type { DayPlan } from '../lib/bindings/DayPlan'
  import type { LoggedTime } from '../lib/bindings/LoggedTime'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { addDays, cal, periodTitle, saveLayers, shiftAnchor, startOfDay, startOfWeek, visibleDays, type CalMode } from '../lib/cal.svelte'
  import { isTyping, isoDate, minutes, swatch } from '../lib/format'
  import { anyDialogOpen } from './Modal.svelte'
  import { errorMessage, inTauri, ipc } from '../lib/ipc'
  import { act, app, scopeQuery } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Agenda from './Agenda.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'
  import MonthGrid from './MonthGrid.svelte'
  import WeekGrid from './WeekGrid.svelte'
  import { segmented } from '../lib/segmented'

  const days = $derived(visibleDays(cal.mode, cal.anchor))
  const range = $derived({
    from: days[0].getTime(),
    to: addDays(days[days.length - 1], 1).getTime(),
    fromIso: isoDate(days[0]),
    toIso: isoDate(days[days.length - 1]),
  })
  const scopeKey = $derived(JSON.stringify(scopeQuery()))

  let occ = $state<Occurrence[]>([])
  let tasks = $state<TaskRow[]>([])
  let plans = $state<Map<string, DayPlan>>(new Map())
  let logged = $state<LoggedTime[]>([])
  let unscheduled = $state<TaskRow[]>([])
  let hours = $state<ClientHours[]>([])
  let timer = $state<LoggedTime | null>(null)
  let generation = 0

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

  /** Every task due in the range, a page at a time (one page holds at most
   *  1000), up to a cap that a calendar can still draw. */
  async function tasksIn(q: TaskQuery): Promise<TaskRow[]> {
    const rows: TaskRow[] = []
    for (let offset = 0; offset < 10_000; offset += 1000) {
      const page = await ipc.listTasks({ ...q, offset, limit: 1000 })
      rows.push(...page.rows)
      if (rows.length >= page.total || !page.rows.length) break
    }
    return rows
  }

  async function load() {
    if (!inTauri) return
    const gen = ++generation
    const r = range
    const base = JSON.parse(scopeKey)
    // Week of the anchor for the hours summary.
    const ws = startOfWeek(cal.anchor).getTime()
    const we = addDays(startOfWeek(cal.anchor), 7).getTime()
    try {
      const [o, t, p, l, u, h, tm] = await Promise.all([
        ipc.occurrences(r.from, r.to),
        tasksIn({ ...base, categories: [], due_from: r.fromIso, due_to: r.toIso, sort: 'due' }),
        cal.mode === 'agenda' ? Promise.resolve([]) : ipc.dayPlans(r.fromIso, r.toIso),
        ipc.loggedTime(r.from, r.to),
        ipc.listTasks({ ...base, due_from: undefined, due_to: undefined, undated: true, top_level: true, categories: ['todo', 'active'], sort: 'priority', limit: 60 }),
        ipc.hoursByClient(ws, we),
        ipc.runningTimer(),
      ])
      if (gen !== generation) return
      occ = o.filter(inScope)
      tasks = t
      plans = new Map(p.map((x) => [x.date, x]))
      logged = l
      unscheduled = u.rows
      hours = h
      timer = tm
    } catch (err) {
      if (gen === generation) toast(errorMessage(err), { kind: 'error' })
    }
  }

  $effect(() => {
    void range.from
    void range.to
    void scopeKey
    void cal.mode
    untrack(() => void load())
  })
  $effect(() => {
    if (!app.dataVersion) return
    untrack(() => void load())
  })

  // ---------- toolbar ----------
  const MODES: { id: CalMode; label: string; key: string }[] = [
    { id: 'month', label: 'Month', key: 'm' },
    { id: 'week', label: 'Week', key: 'w' },
    { id: 'day', label: 'Day', key: 'd' },
    { id: 'agenda', label: 'Agenda', key: 'a' },
  ]
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  function layersMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const toggle = (k: keyof typeof cal.layers) => () => {
      cal.layers[k] = !cal.layers[k]
      saveLayers()
    }
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: [
        { label: 'Tasks', checked: cal.layers.tasks, run: toggle('tasks') },
        { label: 'Meetings', checked: cal.layers.meetings, run: toggle('meetings') },
        { label: 'Time blocks', checked: cal.layers.blocks, run: toggle('blocks') },
        { label: 'Logged time', checked: cal.layers.logged, run: toggle('logged') },
      ],
    }
  }

  /** Opens the editor for a new meeting or time block, starting next hour. */
  function newEvent(kind: 'meeting' | 'block') {
    const d = new Date()
    const start = new Date(d.getFullYear(), d.getMonth(), d.getDate(), d.getHours() + 1).getTime()
    cal.draft = { kind, start, end: start + 3_600_000 }
  }

  function newMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: [
        { label: 'Meeting', hint: 'with others · link & reminder', run: () => newEvent('meeting') },
        { label: 'Time block', hint: 'reserve focus time', run: () => newEvent('block') },
      ],
    }
  }

  function onKeydown(e: KeyboardEvent) {
    // Any open dialog (Find slot, Working hours, Settings, Reminder…) keeps its keys.
    if (isTyping(e) || e.ctrlKey || e.metaKey || e.altKey || app.paletteOpen || cal.draft || anyDialogOpen()) return
    if (e.key === 'ArrowLeft') shiftAnchor(-1)
    else if (e.key === 'ArrowRight') shiftAnchor(1)
    else if (e.key === 't') cal.anchor = startOfDay(new Date())
    else {
      const m = MODES.find((x) => x.key === e.key)
      if (m) cal.mode = m.id
      else return
    }
    e.preventDefault()
  }

  // ---------- sidebar ----------
  function dragTask(e: DragEvent, t: TaskRow) {
    e.dataTransfer?.setData('application/x-taskboard-task', JSON.stringify({ id: t.id, title: t.title, estimate: t.estimate_minutes, project_id: t.project_id }))
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
  }

  let tick = $state(Date.now())
  $effect(() => {
    if (!timer) return
    const t = setInterval(() => (tick = Date.now()), 15_000)
    return () => clearInterval(t)
  })
  const hoursMax = $derived(Math.max(60, ...hours.map((h) => Math.max(h.planned_min, h.logged_min))))
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex min-h-0 flex-1 flex-col gap-2">
  <div class="flex min-h-8 shrink-0 flex-wrap items-center gap-2">
    <button class="tb" onclick={() => (cal.anchor = startOfDay(new Date()))} title="Today (T)">Today</button>
    <div class="flex">
      <button class="tb-icon" aria-label="Previous" title="Previous (←)" onclick={() => shiftAnchor(-1)}><ChevronLeft size={16} /></button>
      <button class="tb-icon" aria-label="Next" title="Next (→)" onclick={() => shiftAnchor(1)}><ChevronRight size={16} /></button>
    </div>
    <h2 class="font-display min-w-0 truncate text-[15px] font-semibold">{periodTitle()}</h2>
    <span class="flex-1"></span>
    <div class="flex rounded-md border border-border bg-surface-2 p-0.5" role="radiogroup" aria-label="Calendar view" use:segmented>
      {#each MODES as m (m.id)}
        <button role="radio" aria-checked={cal.mode === m.id} class="seg" title="{m.label} ({m.key.toUpperCase()})" onclick={() => (cal.mode = m.id)}>{m.label}</button>
      {/each}
    </div>
    <!-- Labels collapse to icons on narrow windows (names stay as tooltips). -->
    <button class="tb" title="Layers" onclick={layersMenu}><Layers size={14} /> <span class="max-[1180px]:sr-only">Layers</span></button>
    <button class="tb" title="Working hours" onclick={() => (cal.workingHoursOpen = true)}><Clock size={14} /> <span class="max-[1180px]:sr-only">Hours</span></button>
    <button class="tb" title="Find a meeting slot" onclick={() => (cal.findSlotOpen = true)}><Search size={14} /> <span class="max-[1180px]:sr-only">Find slot</span></button>
    <button class="tb primary" title="New meeting or time block" aria-haspopup="menu" onclick={newMenu}><Plus size={14} /> <span class="max-[820px]:sr-only">New</span><ChevronDown size={13} class="-mr-0.5 opacity-80" /></button>
  </div>

  <div class="flex min-h-0 flex-1 gap-3">
    {#if cal.mode === 'month'}
      <MonthGrid {days} {occ} {tasks} {plans} />
    {:else if cal.mode === 'agenda'}
      <Agenda {days} {occ} {tasks} />
    {:else}
      <WeekGrid {days} {occ} {tasks} {plans} {logged} />
    {/if}

    {#if !app.openTaskId}
      <aside class="flex w-[236px] shrink-0 flex-col gap-3 overflow-y-auto max-[1100px]:hidden" aria-label="Planning">
        {#if timer}
          <section class="box border-accent">
            <div class="flex items-center gap-2 text-[13px]">
              <Timer size={14} class="text-accent" />
              <span class="min-w-0 flex-1 truncate font-medium">{timer.title}</span>
              <span class="tabular text-xs text-muted">{minutes(Math.max(1, Math.round((tick - timer.started_at) / 60_000)))}</span>
            </div>
            <button class="tb mt-2 w-full justify-center" onclick={() => void act(() => ipc.stopTimer(), 'Timer stopped')}><Square size={12} /> Stop timer</button>
          </section>
        {/if}

        <section class="box">
          <h3 class="box-title">Hours per group this week</h3>
          {#each hours as h (h.client_id ?? 'none')}
            <div class="mb-2">
              <div class="flex items-center gap-1.5 text-[12.5px]">
                <span class="size-2 rounded-full" style="background: {swatch(h.color)}"></span>
                <span class="min-w-0 flex-1 truncate">{h.client_name}</span>
                <span class="tabular text-xs text-muted" title="Logged / planned">{minutes(h.logged_min) || '0m'} / {minutes(h.planned_min) || '0m'}</span>
              </div>
              <div class="bars">
                <span class="planned" style="width: {(h.planned_min / hoursMax) * 100}%"></span>
                <span class="logged" style="width: {(h.logged_min / hoursMax) * 100}%; background: {swatch(h.color, 0.6, 0.14)}"></span>
              </div>
            </div>
          {:else}
            <p class="text-xs text-subtle">No meetings, blocks or logged time this week.</p>
          {/each}
          {#if hours.length}<p class="text-[11px] text-subtle">Bar: logged (color) over planned (gray)</p>{/if}
        </section>

        <section class="box min-h-0">
          <h3 class="box-title">Unscheduled <span class="font-normal text-subtle">· drag onto the calendar</span></h3>
          <ul class="flex flex-col gap-0.5">
            {#each unscheduled as t (t.id)}
              <li
                class="unsched"
                draggable="true"
                ondragstart={(e) => dragTask(e, t)}
                title="Drop on a time to block time for it, or on a day to set its due date"
              >
                <span class="size-1.5 shrink-0 rounded-full" style="background: {swatch(t.project_color)}"></span>
                <button class="min-w-0 flex-1 truncate text-left" onclick={() => (app.openTaskId = t.id)}>{t.title}</button>
                {#if t.estimate_minutes}<span class="tabular text-[11px] text-subtle">{minutes(t.estimate_minutes)}</span>{/if}
              </li>
            {:else}
              <li class="text-xs text-subtle">Every open task here has a date.</li>
            {/each}
          </ul>
        </section>
      </aside>
    {/if}
  </div>
</div>

{#if menu}<Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

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
  .tb-icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--text-muted);
  }
  .tb-icon:hover {
    background: var(--hover);
    color: var(--text);
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
  .box {
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  .box-title {
    margin-bottom: 8px;
    font-size: 12px;
    font-weight: 600;
  }
  .bars {
    position: relative;
    height: 6px;
    margin-top: 3px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bars span {
    position: absolute;
    inset-block: 0;
    left: 0;
    border-radius: 99px;
  }
  .bars .planned {
    background: var(--border-strong);
  }
  .unsched {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    height: 28px;
    padding-inline: 0.4rem;
    border-radius: 6px;
    font-size: 12.5px;
    cursor: grab;
  }
  .unsched:hover {
    background: var(--hover);
  }
</style>
