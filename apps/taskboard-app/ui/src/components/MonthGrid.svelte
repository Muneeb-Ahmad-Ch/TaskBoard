<script lang="ts">
  import { CheckCircle2, Circle } from '@lucide/svelte'
  import type { DayPlan } from '../lib/bindings/DayPlan'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskRow } from '../lib/bindings/TaskRow'
  import { cal, fmtTime, sameDay } from '../lib/cal.svelte'
  import { isoDate, swatch } from '../lib/format'
  import { act, app } from '../lib/store.svelte'
  import { ipc } from '../lib/ipc'

  let { days, occ, tasks, plans }: { days: Date[]; occ: Occurrence[]; tasks: TaskRow[]; plans: Map<string, DayPlan> } = $props()

  /** Each entry takes 24px (20px target + 4px gap, WCAG 2.5.8); fit as many as the cell allows. */
  const ENTRY_PX = 24
  let gridHeight = $state(0)
  const maxItems = $derived(Math.max(0, Math.floor((gridHeight / 6 - 34) / ENTRY_PX)))
  /** Entries shown in a cell: leave room for "+N more" when they don't all fit. */
  function visible<T>(list: T[]): T[] {
    return list.length > maxItems ? list.slice(0, Math.max(0, maxItems - 1)) : list
  }
  const weeks = $derived(Array.from({ length: Math.ceil(days.length / 7) }, (_, i) => days.slice(i * 7, i * 7 + 7)))
  // Follows the clock, so "today" moves on at midnight while the view is open.
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })
  const today = $derived(new Date(now))
  const month = $derived(cal.anchor.getMonth())
  const weekdays = $derived(days.slice(0, 7).map((d) => new Intl.DateTimeFormat(undefined, { weekday: 'short' }).format(d)))

  type Entry = { key: string; sort: number; o?: Occurrence; t?: TaskRow }

  function entries(day: Date): Entry[] {
    const s = day.getTime()
    const e = new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1).getTime()
    const iso = isoDate(day)
    const list: Entry[] = occ
      .filter((o) => o.status !== 'cancelled' && (o.kind === 'meeting' ? cal.layers.meetings : cal.layers.blocks))
      .filter((o) => o.start_at < e && o.end_at > s)
      .map((o) => ({ key: `e-${o.event_id}-${o.original_start}`, sort: o.all_day ? -1 : o.start_at, o }))
    if (cal.layers.tasks)
      for (const t of tasks)
        if (t.due_date === iso) {
          const [h, m] = (t.due_time ?? '23:59').split(':').map(Number)
          list.push({ key: `t-${t.id}`, sort: s + (h * 60 + m) * 60_000 + 1, t })
        }
    return list.sort((a, b) => a.sort - b.sort)
  }

  function open(entry: Entry) {
    if (entry.t) app.openTaskId = entry.t.id
    else if (entry.o)
      cal.draft = {
        id: entry.o.event_id,
        originalStart: entry.o.recurring ? entry.o.original_start : undefined,
        kind: entry.o.kind,
        start: entry.o.start_at,
        end: entry.o.end_at,
        allDay: entry.o.all_day,
        title: entry.o.title,
      }
  }

  // Native drag: tasks (from cells or the unscheduled list) onto another day.
  function onDragStart(e: DragEvent, t: TaskRow) {
    e.dataTransfer?.setData('application/x-taskboard-task', JSON.stringify({ id: t.id, title: t.title, estimate: t.estimate_minutes, project_id: t.project_id }))
  }
  function onDrop(e: DragEvent, day: Date) {
    e.preventDefault()
    try {
      const t = JSON.parse(e.dataTransfer?.getData('application/x-taskboard-task') ?? '')
      void act(() => ipc.updateTask(t.id, { due_date: isoDate(day) }), `Due ${isoDate(day)}`)
    } catch {
      // not a task
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-surface-2" role="grid" aria-label="Month">
  <div class="grid shrink-0 grid-cols-7 border-b border-border" role="row">
    {#each weekdays as w (w)}<div class="px-2 py-1.5 text-xs font-medium text-muted uppercase" role="columnheader">{w}</div>{/each}
  </div>
  <div class="grid min-h-0 flex-1 grid-cols-7 grid-rows-6" bind:clientHeight={gridHeight}>
    {#each weeks as week (isoDate(week[0]))}
    <div class="contents" role="row">
    {#each week as day (isoDate(day))}
      {@const list = entries(day)}
      {@const plan = plans.get(isoDate(day))}
      <div
        class="cell"
        class:outside={day.getMonth() !== month}
        class:off={plan?.day_off}
        role="gridcell"
        tabindex="-1"
        ondragover={(e) => e.dataTransfer?.types.includes('application/x-taskboard-task') && e.preventDefault()}
        ondrop={(e) => onDrop(e, day)}
        ondblclick={(e) => {
          if (e.target !== e.currentTarget) return
          const start = new Date(day.getFullYear(), day.getMonth(), day.getDate(), 10).getTime()
          cal.draft = { kind: 'meeting', start, end: start + 3_600_000 }
        }}
      >
        <div class="flex items-center gap-1.5">
          <button class="date" class:today={sameDay(day, today)} onclick={() => {
            cal.anchor = day
            cal.mode = 'day'
          }}>{day.getDate()}</button>
          {#if plan?.day_off}<span class="truncate text-[10.5px] text-subtle">{plan.note || 'Off'}</span>{/if}
        </div>
        {#each visible(list) as entry (entry.key)}
          {#if entry.o}
            <button class="entry" style="--c: {swatch(entry.o.color, 0.6, 0.14)}" class:allday={entry.o.all_day} onclick={() => open(entry)}>
              {#if !entry.o.all_day}<span class="dot"></span><span class="tabular shrink-0 whitespace-nowrap text-subtle">{fmtTime(entry.o.start_at)}</span>{/if}
              <span class="truncate">{entry.o.title}</span>
            </button>
          {:else if entry.t}
            <button class="entry task" draggable="true" ondragstart={(e) => entry.t && onDragStart(e, entry.t)} onclick={() => open(entry)}>
              {#if entry.t.completed_at}<CheckCircle2 size={11} class="shrink-0 text-success" />{:else}<Circle size={11} class="shrink-0" />{/if}
              <span class="truncate" class:line-through={!!entry.t.completed_at}>{entry.t.title}</span>
            </button>
          {/if}
        {/each}
        {#if list.length > maxItems}
          <button class="more" onclick={() => {
            cal.anchor = day
            cal.mode = 'day'
          }}>+{list.length - visible(list).length} more</button>
        {/if}
      </div>
    {/each}
    </div>
    {/each}
  </div>
</div>

<style>
  .cell {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
    padding: 4px;
    border-left: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  .cell.outside {
    background: color-mix(in oklch, var(--surface-3) 45%, transparent);
  }
  .cell.outside .date {
    color: var(--text-subtle);
  }
  .cell.off {
    background: repeating-linear-gradient(135deg, transparent 0 8px, color-mix(in oklch, var(--surface-3) 60%, transparent) 8px 16px);
  }
  .date {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 22px;
    padding-inline: 4px;
    border-radius: 99px;
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .date:hover {
    background: var(--hover);
  }
  .date.today {
    background: var(--accent);
    color: var(--on-accent);
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    flex-shrink: 0;
    height: 20px;
    padding-inline: 4px;
    border-radius: 4px;
    font-size: 11.5px;
    text-align: left;
  }
  .entry:hover {
    background: var(--hover);
  }
  .entry.allday {
    background: color-mix(in oklch, var(--c) 22%, transparent);
  }
  .entry.task {
    color: var(--text-muted);
    cursor: grab;
  }
  .dot {
    width: 6px;
    height: 6px;
    flex-shrink: 0;
    border-radius: 99px;
    background: var(--c);
  }
  .more {
    align-self: flex-start;
    flex-shrink: 0;
    height: 20px;
    padding-inline: 4px;
    font-size: 11px;
    color: var(--text-subtle);
  }
  .more:hover {
    color: var(--text);
  }
</style>
