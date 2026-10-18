<script lang="ts">
  // Task panel: the time blocks booked for this task (in the calendar, or by
  // an assistant's schedule_task), and a button to book another.
  import { CalendarPlus } from '@lucide/svelte'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { Task } from '../lib/bindings/Task'
  import { cal, fmtRange, MINUTE } from '../lib/cal.svelte'
  import { minutes, plural } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { app } from '../lib/store.svelte'

  let { task }: { task: Task } = $props()

  const DAY = 86_400_000
  let blocks = $state<Occurrence[]>([])
  /** Re-read every minute, so a block that ended moves out of "upcoming". */
  let now = $state(Date.now())

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 60_000)
    return () => clearInterval(t)
  })

  $effect(() => {
    void app.dataVersion
    const id = task.id
    if (!inTauri) return
    let stale = false
    const at = Date.now()
    ipc
      .occurrences(at - 30 * DAY, at + 120 * DAY)
      .then((list) => {
        if (stale) return
        blocks = list.filter((o) => o.task_id === id && o.status !== 'cancelled').sort((a, b) => a.start_at - b.start_at)
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  const upcoming = $derived(blocks.filter((b) => b.end_at > now))
  const earlier = $derived(blocks.length - upcoming.length)
  const planned = $derived(upcoming.reduce((sum, b) => sum + (b.all_day ? 0 : (b.end_at - b.start_at) / MINUTE), 0))
  const SHOWN = 5
  const day = new Intl.DateTimeFormat(undefined, { weekday: 'short', month: 'short', day: 'numeric' })

  /** A block at the next full hour, as long as the estimate (else an hour). */
  function book() {
    const s = new Date()
    s.setMinutes(0, 0, 0)
    s.setHours(s.getHours() + 1)
    const length = (task.estimate_minutes && task.estimate_minutes > 0 ? task.estimate_minutes : 60) * MINUTE
    cal.draft = { kind: 'block', start: s.getTime(), end: s.getTime() + length, title: task.title, taskId: task.id, projectId: task.project_id }
  }

  function open(o: Occurrence) {
    cal.draft = { id: o.event_id, originalStart: o.recurring ? o.original_start : undefined, kind: o.kind, start: o.start_at, end: o.end_at, allDay: o.all_day, title: o.title }
  }
</script>

<h3 class="mt-5 mb-1.5 flex items-center justify-between text-xs font-semibold tracking-wide text-subtle uppercase">
  <span>Time blocks</span>
  {#if planned}<span class="tabular font-normal normal-case">{minutes(Math.round(planned))} planned</span>{/if}
</h3>
{#if upcoming.length}
  <ul class="flex flex-col">
    {#each upcoming.slice(0, SHOWN) as b (`${b.event_id}-${b.original_start}`)}
      <li>
        <button class="flex h-8 w-full items-center gap-2 rounded-md px-1 text-left hover:bg-hover" title="Open this time block" onclick={() => open(b)}>
          <span class="h-4 w-1 shrink-0 rounded" style="background: var(--accent)"></span>
          <span class="tabular w-[92px] shrink-0 text-xs text-muted">{day.format(b.start_at)}</span>
          <span class="tabular flex-1 truncate text-xs">{b.all_day ? 'All day' : fmtRange(b.start_at, b.end_at)}</span>
          {#if b.start_at <= now && b.end_at > now}<span class="now">Now</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  {#if upcoming.length > SHOWN}<p class="px-1 text-xs text-muted">{upcoming.length - SHOWN} more in the calendar</p>{/if}
{:else}
  <p class="px-1 text-xs text-muted">No time booked for this task yet.</p>
{/if}
{#if earlier}<p class="px-1 pt-0.5 text-xs text-subtle">{plural(earlier, 'earlier block')} in the last 30 days</p>{/if}
<button class="mt-1 flex h-8 items-center gap-2 rounded-md px-1 text-muted hover:bg-hover hover:text-text" onclick={book}>
  <CalendarPlus size={16} /> Block time for this task
</button>

<style>
  .now {
    flex-shrink: 0;
    padding: 0 6px;
    border-radius: 99px;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent-soft-text);
    background: var(--accent-soft);
  }
</style>
