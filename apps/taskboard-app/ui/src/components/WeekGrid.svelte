<script lang="ts">
  import { CheckCircle2, Circle, Repeat, Video } from '@lucide/svelte'
  import type { DayPlan } from '../lib/bindings/DayPlan'
  import type { LoggedTime } from '../lib/bindings/LoggedTime'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { cal, fmtRange, fmtTime, layoutColumns, MINUTE, sameDay } from '../lib/cal.svelte'
  import { isoDate, minutes, swatch } from '../lib/format'
  import { errorMessage, ipc } from '../lib/ipc'
  import { act, app } from '../lib/store.svelte'
  import { toast } from '../lib/toast.svelte'
  import Menu, { type MenuEntry } from './Menu.svelte'

  let {
    days,
    occ,
    tasks,
    plans,
    logged,
  }: { days: Date[]; occ: Occurrence[]; tasks: TaskRow[]; plans: Map<string, DayPlan>; logged: LoggedTime[] } = $props()

  const PX = 48 // pixels per hour
  const SNAP = 15
  const TASK_MIN = 30

  interface Item {
    key: string
    start: number
    end: number
    o?: Occurrence
    t?: TaskRow
  }

  const pad = (n: number) => String(n).padStart(2, '0')
  const minOfDay = (ms: number) => {
    const d = new Date(ms)
    return d.getHours() * 60 + d.getMinutes()
  }
  const atMin = (day: Date, min: number) => new Date(day.getFullYear(), day.getMonth(), day.getDate(), 0, min).getTime()
  const snap = (m: number) => Math.round(m / SNAP) * SNAP
  const dayStart = (d: Date) => d.getTime()
  const dayEnd = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime()

  function visibleEvent(o: Occurrence) {
    return o.status !== 'cancelled' && (o.kind === 'meeting' ? cal.layers.meetings : cal.layers.blocks)
  }

  function timedItems(day: Date): Item[] {
    const s = dayStart(day)
    const e = dayEnd(day)
    const iso = isoDate(day)
    const items: Item[] = occ
      .filter((o) => !o.all_day && visibleEvent(o) && o.start_at < e && o.end_at > s)
      .map((o) => ({ key: `e-${o.event_id}-${o.original_start}`, start: Math.max(o.start_at, s), end: Math.min(o.end_at, e), o }))
    if (cal.layers.tasks) {
      for (const t of tasks) {
        if (t.due_date !== iso || !t.due_time) continue
        const [h, m] = t.due_time.split(':').map(Number)
        const start = atMin(day, h * 60 + m)
        items.push({ key: `t-${t.id}`, start, end: Math.min(e, start + TASK_MIN * MINUTE), t })
      }
    }
    return items
  }

  function allDayItems(day: Date): Item[] {
    const s = dayStart(day)
    const e = dayEnd(day)
    const iso = isoDate(day)
    const items: Item[] = occ
      .filter((o) => o.all_day && visibleEvent(o) && o.start_at < e && o.end_at > s)
      .map((o) => ({ key: `e-${o.event_id}-${o.original_start}`, start: o.start_at, end: o.end_at, o }))
    if (cal.layers.tasks)
      for (const t of tasks)
        if (t.due_date === iso && !t.due_time) items.push({ key: `t-${t.id}`, start: s, end: e, t })
    return items
  }

  function loggedFor(day: Date) {
    if (!cal.layers.logged) return []
    const s = dayStart(day)
    const e = dayEnd(day)
    const now = Date.now()
    return logged
      .map((l) => ({ l, start: Math.max(l.started_at, s), end: Math.min(l.ended_at ?? now, e) }))
      .filter((x) => x.end > x.start)
  }

  const top = (ms: number, day: Date) => (ms <= dayStart(day) ? 0 : (minOfDay(ms) * PX) / 60)
  const bottom = (ms: number, day: Date) => (ms >= dayEnd(day) ? 24 * PX : (minOfDay(ms) * PX) / 60)

  // ---------- capacity ----------
  function capacity(day: Date) {
    const p = plans.get(isoDate(day))
    if (!p) return null
    const used = p.meetings_min + p.blocks_min + p.due_estimate_min
    const ratio = p.available_min ? used / p.available_min : used ? 2 : 0
    const level = ratio > 1 ? 'over' : ratio > 0.8 ? 'tight' : 'ok'
    return { p, used, ratio, level }
  }

  // ---------- pointer interactions ----------
  interface Drag {
    mode: 'create' | 'move' | 'resize'
    day: number
    startMin: number
    curMin: number
    item?: Item
    grab: number
    moved: boolean
    x0: number
    y0: number
    alt: boolean
  }
  let drag = $state<Drag | null>(null)
  let scroller: HTMLDivElement | undefined = $state()
  let menu = $state<{ x: number; y: number; items: MenuEntry[] } | null>(null)

  function minAt(e: PointerEvent | DragEvent, col: HTMLElement): number {
    const r = col.getBoundingClientRect()
    return Math.max(0, Math.min(24 * 60, ((e.clientY - r.top) / PX) * 60))
  }

  function columnUnder(x: number, y: number): { index: number; el: HTMLElement } | null {
    const el = document.elementsFromPoint(x, y).find((n): n is HTMLElement => n instanceof HTMLElement && n.dataset.daycol !== undefined)
    return el ? { index: Number(el.dataset.daycol), el } : null
  }

  function begin(e: PointerEvent, mode: Drag['mode'], dayIndex: number, item?: Item) {
    if (e.button !== 0) return
    e.stopPropagation()
    const col = (e.currentTarget as HTMLElement).closest<HTMLElement>('[data-daycol]')
    if (!col) return
    const m = minAt(e, col)
    const startMin = item ? minOfDay(item.start) : snap(m - SNAP / 2)
    drag = { mode, day: dayIndex, startMin, curMin: mode === 'resize' && item ? minOfDay(item.end) || 24 * 60 : startMin, item, grab: item ? m - minOfDay(item.start) : 0, moved: false, x0: e.clientX, y0: e.clientY, alt: e.altKey }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  function onMove(e: PointerEvent) {
    if (!drag) return
    if (!drag.moved && Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0) < 4) return
    drag.moved = true
    const hit = columnUnder(e.clientX, e.clientY)
    if (!hit) return
    const m = minAt(e, hit.el)
    if (drag.mode === 'create') {
      drag.curMin = snap(m)
    } else if (drag.mode === 'move') {
      drag.day = hit.index
      const length = drag.item ? (drag.item.end - drag.item.start) / MINUTE : 60
      drag.startMin = Math.max(0, Math.min(24 * 60 - length, snap(m - drag.grab)))
    } else {
      drag.curMin = Math.max(drag.startMin + SNAP, snap(m))
    }
  }

  function onUp(e: PointerEvent) {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    const d = drag
    drag = null
    if (!d) return
    const day = days[d.day]
    if (d.mode === 'create') {
      const a = d.moved ? Math.min(d.startMin, d.curMin) : d.startMin
      const b = d.moved ? Math.max(d.startMin, d.curMin) : d.startMin + 60
      cal.draft = { kind: d.alt ? 'block' : 'meeting', start: atMin(day, a), end: atMin(day, Math.max(b, a + SNAP)) }
      return
    }
    const item = d.item
    if (!item) return
    if (!d.moved) {
      if (item.t) app.openTaskId = item.t.id
      else if (item.o) openEvent(item.o)
      return
    }
    if (item.t) {
      const due_time = `${pad(Math.floor(d.startMin / 60))}:${pad(d.startMin % 60)}`
      void act(() => ipc.updateTask(item.t!.id, { due_date: isoDate(day), due_time }), `Moved “${item.t!.title}”`)
      return
    }
    const o = item.o
    if (!o) return
    const start = d.mode === 'move' ? atMin(day, d.startMin) : o.start_at
    const end = d.mode === 'move' ? start + (o.end_at - o.start_at) : atMin(days[d.day], d.curMin)
    commitEvent(o, start, end, e.clientX, e.clientY)
  }

  function openEvent(o: Occurrence) {
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

  /** Persist a moved/resized event; recurring ones ask which occurrences. */
  function commitEvent(o: Occurrence, start: number, end: number, x: number, y: number) {
    if (!o.recurring) {
      void act(() => ipc.updateEvent(o.event_id, { start_at: start, end_at: end }), `Moved “${o.title}”`)
      return
    }
    menu = {
      x,
      y,
      items: [
        { label: 'Move this event', run: () => void act(() => ipc.editOccurrence(o.event_id, o.original_start, { start_at: start, end_at: end })) },
        { label: 'Move this and following', run: () => void act(() => ipc.splitSeries(o.event_id, o.original_start, { start_at: start, end_at: end })) },
        {
          label: 'Move all events',
          run: () =>
            void act(async () => {
              const series = await ipc.getEvent(o.event_id)
              if (series.start_at === null) return
              const delta = start - o.original_start
              await ipc.updateEvent(o.event_id, { start_at: series.start_at + delta, end_at: series.start_at + delta + (end - start) })
            }),
        },
      ],
    }
  }

  // ---------- drops from the unscheduled list ----------
  function onDragOver(e: DragEvent) {
    if (e.dataTransfer?.types.includes('application/x-taskboard-task')) {
      e.preventDefault()
      e.dataTransfer.dropEffect = 'move'
    }
  }

  function readTask(e: DragEvent): { id: string; title: string; estimate: number | null; project_id: string } | null {
    try {
      return JSON.parse(e.dataTransfer?.getData('application/x-taskboard-task') ?? '')
    } catch {
      return null
    }
  }

  /** Dropping a task on the grid books a time block for it (due date unchanged). */
  async function dropOnGrid(e: DragEvent, dayIndex: number) {
    e.preventDefault()
    const t = readTask(e)
    const col = (e.currentTarget as HTMLElement).closest<HTMLElement>('[data-daycol]')
    if (!t || !col) return
    const start = atMin(days[dayIndex], snap(minAt(e, col)))
    const length = Math.max(15, Math.min(8 * 60, t.estimate ?? 60))
    try {
      await ipc.createEvent({ kind: 'block', title: t.title, task_id: t.id, project_id: t.project_id, start_at: start, end_at: start + length * MINUTE })
      toast(`Blocked ${minutes(length)} for “${t.title}”`, { action: { label: 'Undo', run: () => void ipc.undo() } })
    } catch (err) {
      toast(errorMessage(err), { kind: 'error' })
    }
  }

  /** Dropping on the all-day row sets the task's due date. */
  function dropOnDay(e: DragEvent, day: Date) {
    e.preventDefault()
    const t = readTask(e)
    if (t) void act(() => ipc.updateTask(t.id, { due_date: isoDate(day) }), `Due ${isoDate(day)}`)
  }

  // ---------- misc ----------
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    // Start the view around the working morning.
    if (scroller) scroller.scrollTop = 7.5 * PX
  })

  const weekdayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short' })
  const hourFmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric' })
  const today = new Date()
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-surface-2">
  <!-- day headers + all-day row -->
  <!-- Same scrollbar gutter as the grid below, so columns line up. -->
  <div class="gutter grid shrink-0 border-b border-border" style="grid-template-columns: 56px repeat({days.length}, minmax(0, 1fr))">
    <div></div>
    {#each days as day (isoDate(day))}
      {@const c = capacity(day)}
      <div class="day-head" class:today={sameDay(day, today)}>
        <button class="flex items-baseline gap-1.5" onclick={() => {
          cal.anchor = day
          cal.mode = 'day'
        }}>
          <span class="text-xs text-muted uppercase">{weekdayFmt.format(day)}</span>
          <span class="date">{day.getDate()}</span>
        </button>
        {#if c?.p.day_off}
          <span class="off" title={c.p.note || 'Day off'}>{c.p.note || 'Off'}</span>
        {:else if c && c.p.available_min}
          <span
            class="cap {c.level}"
            title="{minutes(c.used)} planned of {minutes(c.p.available_min)} working · meetings {minutes(c.p.meetings_min)}, blocks {minutes(c.p.blocks_min)}, due tasks {minutes(c.p.due_estimate_min)}{c.p.logged_min ? ` · logged ${minutes(c.p.logged_min)}` : ''}"
          >
            <span style="width: {Math.min(100, c.ratio * 100)}%"></span>
          </span>
        {/if}
      </div>
    {/each}
    <div class="pr-2 text-right text-[10.5px] leading-[24px] text-subtle">all day</div>
    {#each days as day (isoDate(day))}
      <div class="allday" role="list" ondragover={onDragOver} ondrop={(e) => dropOnDay(e, day)}>
        {#each allDayItems(day) as item (item.key)}
          {#if item.o}
            <button class="chip event" style="--c: {swatch(item.o.color, 0.6, 0.14)}" onclick={() => item.o && openEvent(item.o)}>{item.o.title}</button>
          {:else if item.t}
            <button class="chip task" class:done={!!item.t.completed_at} onclick={() => item.t && (app.openTaskId = item.t.id)}>
              {#if item.t.completed_at}<CheckCircle2 size={11} />{:else}<Circle size={11} />{/if}{item.t.title}
            </button>
          {/if}
        {/each}
      </div>
    {/each}
  </div>

  <!-- time grid -->
  <div bind:this={scroller} class="gutter relative min-h-0 flex-1 overflow-y-auto">
    <div class="grid" style="grid-template-columns: 56px repeat({days.length}, minmax(0, 1fr)); height: {24 * PX}px">
      <div class="relative">
        {#each { length: 24 } as _, h (h)}
          {#if h > 0}<span class="hour-label" style="top: {h * PX}px">{hourFmt.format(new Date(2000, 0, 1, h))}</span>{/if}
        {/each}
      </div>
      {#each days as day, di (isoDate(day))}
        {@const plan = plans.get(isoDate(day))}
        {@const items = timedItems(day)}
        {@const cols = layoutColumns(items)}
        <div
          class="daycol"
          data-daycol={di}
          role="gridcell"
          tabindex="-1"
          onpointerdown={(e) => begin(e, 'create', di)}
          ondragover={onDragOver}
          ondrop={(e) => void dropOnGrid(e, di)}
        >
          <!-- working hours -->
          {#each plan?.working ?? [] as [a, b] (a)}
            <div class="working" style="top: {top(a, day)}px; height: {bottom(b, day) - top(a, day)}px"></div>
          {/each}
          {#each { length: 24 } as _, h (h)}<div class="hour-line" style="top: {h * PX}px"></div>{/each}

          <!-- logged time strip -->
          {#each loggedFor(day) as { l, start, end } (`${l.entry_id ?? l.event_id}-${start}`)}
            <div
              class="logged"
              style="top: {top(start, day)}px; height: {Math.max(2, bottom(end, day) - top(start, day))}px; --c: {swatch(l.color, 0.6, 0.14)}"
              title="Logged: {l.title} · {fmtRange(l.started_at, l.ended_at ?? Date.now())}{l.client_name ? ` · ${l.client_name}` : ''}"
            ></div>
          {/each}

          {#each items as item (item.key)}
            {@const pos = cols.get(item)}
            {@const dragging = drag?.item?.key === item.key && drag.moved}
            {@const h = Math.max(18, bottom(item.end, day) - top(item.start, day))}
            <div
              class="item"
              class:task={!!item.t}
              class:block={item.o?.kind === 'block'}
              class:tentative={item.o?.status === 'tentative'}
              class:faded={dragging}
              style="top: {top(item.start, day)}px; height: {h}px; left: calc({((pos?.col ?? 0) / (pos?.cols ?? 1)) * 100}% + 2px); width: calc({100 / (pos?.cols ?? 1)}% - 6px); --c: {swatch(item.o?.color ?? item.t?.project_color ?? 'gray', 0.6, 0.14)}"
              role="button"
              tabindex="0"
              aria-label={item.o ? `${item.o.title}, ${fmtRange(item.o.start_at, item.o.end_at)}` : item.t?.title}
              onpointerdown={(e) => begin(e, 'move', di, item)}
              onkeydown={(e) => {
                if (e.key === 'Enter') {
                  if (item.t) app.openTaskId = item.t.id
                  else if (item.o) openEvent(item.o)
                }
              }}
            >
              {#if item.o}
                <div class="flex items-center gap-1 font-medium">
                  {#if item.o.meeting_url}<Video size={11} class="shrink-0" />{/if}
                  <span class="truncate">{item.o.title}</span>
                  {#if item.o.recurring}<Repeat size={10} class="shrink-0 opacity-70" />{/if}
                </div>
                {#if h > 30}
                  <div class="truncate opacity-80">{fmtRange(item.o.start_at, item.o.end_at)}{item.o.client_name ? ` · ${item.o.client_name}` : ''}</div>
                {/if}
                <div class="resize" role="separator" aria-label="Resize" onpointerdown={(e) => begin(e, 'resize', di, item)}></div>
              {:else if item.t}
                <div class="flex items-center gap-1">
                  {#if item.t.completed_at}<CheckCircle2 size={11} class="shrink-0" />{:else}<Circle size={11} class="shrink-0" />{/if}
                  <span class="truncate" class:line-through={!!item.t.completed_at}>{fmtTime(item.start)} {item.t.title}</span>
                </div>
              {/if}
            </div>
          {/each}

          <!-- previews -->
          {#if drag && drag.moved && drag.day === di}
            {#if drag.mode === 'create'}
              {@const a = Math.min(drag.startMin, drag.curMin)}
              {@const b = Math.max(drag.startMin, drag.curMin, a + SNAP)}
              <div class="ghost" style="top: {(a * PX) / 60}px; height: {((b - a) * PX) / 60}px">
                {fmtRange(atMin(day, a), atMin(day, b))}
              </div>
            {:else if drag.mode === 'move' && drag.item}
              {@const len = (drag.item.end - drag.item.start) / MINUTE}
              <div class="ghost" style="top: {(drag.startMin * PX) / 60}px; height: {(len * PX) / 60}px">
                {drag.item.o?.title ?? drag.item.t?.title} · {fmtTime(atMin(day, drag.startMin))}
              </div>
            {:else if drag.mode === 'resize' && drag.item}
              <div class="ghost" style="top: {top(drag.item.start, day)}px; height: {(drag.curMin * PX) / 60 - top(drag.item.start, day)}px">
                until {fmtTime(atMin(day, drag.curMin))}
              </div>
            {/if}
          {/if}

          {#if sameDay(day, today)}
            <div class="now" style="top: {(minOfDay(now) * PX) / 60}px"></div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

{#if menu}
  <Menu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .gutter {
    scrollbar-gutter: stable;
  }
  div.gutter.shrink-0 {
    overflow-y: hidden;
  }
  .day-head {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px 6px;
    border-left: 1px solid var(--border);
  }
  .day-head .date {
    font-size: 17px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .day-head.today .date {
    color: var(--accent);
  }
  .cap {
    display: block;
    height: 4px;
    border-radius: 99px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .cap span {
    display: block;
    height: 100%;
    background: var(--success);
  }
  .cap.tight span {
    background: var(--warn);
  }
  .cap.over span {
    background: var(--danger);
  }
  .off {
    align-self: flex-start;
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--surface-3);
  }
  .allday {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 26px;
    padding: 2px 3px 4px;
    border-left: 1px solid var(--border);
  }
  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding-inline: 5px;
    border-radius: 4px;
    font-size: 11.5px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip.event {
    background: color-mix(in oklch, var(--c) 22%, transparent);
    color: var(--text);
  }
  .chip.task {
    color: var(--text-muted);
  }
  .chip.task:hover {
    background: var(--hover);
  }
  .chip.done {
    text-decoration: line-through;
  }
  .hour-label {
    position: absolute;
    right: 8px;
    transform: translateY(-50%);
    font-size: 10.5px;
    color: var(--text-subtle);
  }
  .daycol {
    position: relative;
    border-left: 1px solid var(--border);
    background: color-mix(in oklch, var(--surface-3) 45%, transparent);
    cursor: cell;
  }
  .working {
    position: absolute;
    inset-inline: 0;
    background: var(--surface-2);
  }
  .hour-line {
    position: absolute;
    inset-inline: 0;
    border-top: 1px solid color-mix(in oklch, var(--border) 70%, transparent);
    pointer-events: none;
  }
  .item {
    position: absolute;
    z-index: 2;
    overflow: hidden;
    padding: 2px 6px;
    border-radius: 6px;
    border-left: 3px solid var(--c);
    background: color-mix(in oklch, var(--c) 20%, var(--surface-2));
    font-size: 11.5px;
    line-height: 1.3;
    cursor: grab;
    box-shadow: 0 0 0 1px var(--surface-2);
  }
  .item.block {
    background: repeating-linear-gradient(135deg, color-mix(in oklch, var(--c) 14%, var(--surface-2)) 0 6px, color-mix(in oklch, var(--c) 6%, var(--surface-2)) 6px 12px);
  }
  .item.tentative {
    border-left-style: dashed;
    opacity: 0.8;
  }
  .item.task {
    border-left-width: 2px;
    background: var(--surface-2);
    border: 1px dashed color-mix(in oklch, var(--c) 60%, transparent);
    color: var(--text-muted);
  }
  .item.faded {
    opacity: 0.35;
  }
  .resize {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 6px;
    cursor: ns-resize;
  }
  .logged {
    position: absolute;
    right: 1px;
    width: 4px;
    border-radius: 2px;
    background: var(--c);
    opacity: 0.85;
  }
  .ghost {
    position: absolute;
    z-index: 5;
    left: 2px;
    right: 6px;
    padding: 2px 6px;
    border-radius: 6px;
    border: 1.5px solid var(--accent);
    background: color-mix(in oklch, var(--accent) 16%, transparent);
    font-size: 11.5px;
    color: var(--accent-soft-text);
    pointer-events: none;
  }
  .now {
    position: absolute;
    z-index: 4;
    inset-inline: 0;
    height: 2px;
    background: var(--danger);
    pointer-events: none;
  }
  .now::before {
    content: '';
    position: absolute;
    left: -4px;
    top: -3px;
    width: 8px;
    height: 8px;
    border-radius: 99px;
    background: var(--danger);
  }
</style>
