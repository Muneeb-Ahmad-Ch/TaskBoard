<script lang="ts">
  // Header summary for a client or project: progress, estimated time left,
  // meetings in the next 7 days and overdue tasks.
  import { AlertTriangle, CalendarClock, Clock } from '@lucide/svelte'
  import type { Id } from '../lib/bindings/Id'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import { isoDate, minutes } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { app } from '../lib/store.svelte'
  import Progress from './Progress.svelte'

  let { kind, id, done, total }: { kind: 'client' | 'project'; id: Id; done: number; total: number } = $props()

  let estimateLeft = $state(0)
  let overdue = $state(0)
  let meetings = $state<Occurrence[]>([])

  $effect(() => {
    void app.dataVersion
    if (!inTauri) return
    const scope: TaskQuery = kind === 'client' ? { client_id: id } : { project_id: id }
    const yesterday = new Date()
    yesterday.setDate(yesterday.getDate() - 1)
    const now = Date.now()
    let stale = false
    Promise.all([
      ipc.summarizeTasks({ ...scope, categories: ['todo', 'active'], top_level: true }),
      ipc.listTasks({ ...scope, categories: ['todo', 'active'], due_to: isoDate(yesterday), limit: 1 }),
      ipc.occurrences(now, now + 7 * 86_400_000),
    ])
      .then(([summary, late, occ]) => {
        if (stale) return
        estimateLeft = summary.estimate_minutes
        overdue = late.total
        meetings = occ.filter((o) => o.kind === 'meeting' && o.status !== 'cancelled' && (kind === 'client' ? o.client_id === id : o.project_id === id))
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  const pct = $derived(total ? (done / total) * 100 : 0)
  const next = $derived(meetings.length ? meetings.reduce((a, b) => (a.start_at <= b.start_at ? a : b)) : null)
  const nextFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' })
</script>

<span class="flex min-w-0 items-center gap-3 self-center text-xs text-muted">
  {#if total}
    <span class="flex shrink-0 items-center gap-2" title="{done} of {total} tasks done">
      <Progress value={pct} width={72} label name="{kind === 'client' ? 'Client' : 'Project'} progress" />
      <span class="tabular text-subtle">{done}/{total}</span>
    </span>
  {/if}
  {#if estimateLeft}
    <span class="stat max-[1180px]:hidden" title="Estimated time for open tasks">
      <Clock size={13} /><span class="tabular">{minutes(estimateLeft)} left</span>
    </span>
  {/if}
  {#if meetings.length}
    <span class="stat max-[1280px]:hidden" title={next ? `Next: ${next.title}, ${nextFmt.format(next.start_at)}` : ''}>
      <CalendarClock size={13} /><span class="tabular">{meetings.length} {meetings.length === 1 ? 'meeting' : 'meetings'} this week</span>
    </span>
  {/if}
  {#if overdue}
    <span class="stat text-danger" title="Open tasks past their due date">
      <AlertTriangle size={13} /><span class="tabular">{overdue} overdue</span>
    </span>
  {/if}
</span>

<style>
  .stat {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.3rem;
    white-space: nowrap;
  }
</style>
