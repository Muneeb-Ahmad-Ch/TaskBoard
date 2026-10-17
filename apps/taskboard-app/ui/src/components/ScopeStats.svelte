<script lang="ts">
  // Header summary for a section, group or project: progress, estimated time left,
  // meetings in the next 7 days and overdue tasks.
  import { AlertTriangle, CalendarClock, Clock } from '@lucide/svelte'
  import type { Id } from '../lib/bindings/Id'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import { cap, isoDate, minutes } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { app, itemName, sectionById } from '../lib/store.svelte'
  import Progress from './Progress.svelte'

  let { kind, id, done, total }: { kind: 'section' | 'client' | 'project'; id: Id; done: number; total: number } = $props()

  let estimateLeft = $state(0)
  let overdue = $state(0)
  let meetings = $state<Occurrence[]>([])

  $effect(() => {
    void app.dataVersion
    if (!inTauri) return
    const scope: TaskQuery = kind === 'section' ? { section_id: id } : kind === 'client' ? { client_id: id } : { project_id: id }
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
        meetings = occ.filter((o) => o.kind === 'meeting' && o.status !== 'cancelled' && inScope(o))
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  function inScope(o: Occurrence): boolean {
    if (kind === 'client') return o.client_id === id
    if (kind === 'project') return o.project_id === id
    const project = o.project_id ? app.projects.find((p) => p.id === o.project_id) : undefined
    if (project) return project.section_id === id
    return !!o.client_id && app.clients.some((c) => c.id === o.client_id && c.section_id === id)
  }

  const progressName = $derived(
    kind === 'section'
      ? 'Section progress'
      : kind === 'client'
        ? `${cap(itemName(sectionById(app.clients.find((c) => c.id === id)?.section_id)))} progress`
        : 'Project progress',
  )
  const pct = $derived(total ? (done / total) * 100 : 0)
  const next = $derived(meetings.length ? meetings.reduce((a, b) => (a.start_at <= b.start_at ? a : b)) : null)
  const nextFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' })
</script>

<span class="flex min-w-0 items-center gap-3 self-center text-xs text-muted">
  {#if total}
    <span class="flex shrink-0 items-center gap-2" title="{done} of {total} tasks done">
      <Progress value={pct} width={72} label name={progressName} />
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
