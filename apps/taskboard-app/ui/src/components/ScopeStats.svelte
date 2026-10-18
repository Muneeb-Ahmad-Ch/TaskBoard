<script lang="ts">
  // Header summary for whatever list is open — Today, Upcoming, Inbox, All
  // work, a section, group or project: progress (% and done/total), time
  // left on open tasks, meetings coming up and overdue tasks.
  //
  // Progress counts top-level tasks, cancelled ones excluded. Today counts
  // what is still due (overdue included) plus what was finished today, so it
  // fills up as the day goes; Upcoming counts the next 7 days.
  import { AlertTriangle, CalendarClock, Clock } from '@lucide/svelte'
  import type { Occurrence } from '../lib/bindings/Occurrence'
  import type { TaskQuery } from '../lib/bindings/TaskQuery'
  import { cap, isoDate, minutes } from '../lib/format'
  import { inTauri, ipc } from '../lib/ipc'
  import { app, inboxId, itemName, sectionById, type Scope } from '../lib/store.svelte'
  import Progress from './Progress.svelte'

  let { scope }: { scope: Scope } = $props()

  let done = $state(0)
  let total = $state(0)
  let estimateLeft = $state(0)
  let overdue = $state(0)
  let meetings = $state<Occurrence[]>([])

  const OPEN: TaskQuery['categories'] = ['todo', 'active']
  const COUNTED: TaskQuery['categories'] = ['todo', 'active', 'done']

  /** The list's own filter (no status filter), or null when it has none to offer. */
  function base(s: Scope): TaskQuery | null {
    switch (s.kind) {
      case 'inbox': {
        const id = inboxId()
        return id ? { project_id: id, top_level: true } : null
      }
      case 'all':
        return { top_level: true }
      case 'section':
        return { section_id: s.id, top_level: true }
      case 'client':
        return { client_id: s.id, top_level: true }
      case 'project':
        return { project_id: s.id, top_level: true }
      default:
        return null
    }
  }

  function inScope(o: Occurrence, s: Scope): boolean {
    switch (s.kind) {
      case 'today':
      case 'upcoming':
      case 'all':
        return true
      case 'client':
        return o.client_id === s.id
      case 'project':
        return o.project_id === s.id
      case 'section': {
        const project = o.project_id ? app.projects.find((p) => p.id === o.project_id) : undefined
        if (project) return project.section_id === s.id
        return !!o.client_id && app.clients.some((c) => c.id === o.client_id && c.section_id === s.id)
      }
      default:
        return false
    }
  }

  $effect(() => {
    void app.dataVersion
    const s = scope
    if (!inTauri) return
    const now = new Date()
    const today = isoDate(now)
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
    const yesterday = new Date(now)
    yesterday.setDate(now.getDate() - 1)
    const week = new Date(now)
    week.setDate(now.getDate() + 7)
    const tomorrow = new Date(now)
    tomorrow.setDate(now.getDate() + 1)
    // Meetings ahead: the rest of today for Today, the next 7 days elsewhere.
    const meetingsTo = s.kind === 'today' ? startOfToday + 86_400_000 : now.getTime() + 7 * 86_400_000
    let stale = false

    const work = (async () => {
      if (s.kind === 'today') {
        const [left, finished] = await Promise.all([
          ipc.summarizeTasks({ due_to: today, categories: OPEN }),
          ipc.summarizeTasks({ due_to: today, categories: ['done'], completed_from: startOfToday }),
        ])
        return { done: finished.total, total: left.total + finished.total, estimate: left.estimate_minutes, overdue: 0 }
      }
      if (s.kind === 'upcoming') {
        const window = { due_from: isoDate(tomorrow), due_to: isoDate(week) }
        const [all, left] = await Promise.all([
          ipc.summarizeTasks({ ...window, categories: COUNTED }),
          ipc.summarizeTasks({ ...window, categories: OPEN }),
        ])
        return { done: all.done, total: all.total, estimate: left.estimate_minutes, overdue: 0 }
      }
      const q = base(s)
      if (!q) return { done: 0, total: 0, estimate: 0, overdue: 0 }
      const [all, left, late] = await Promise.all([
        ipc.summarizeTasks({ ...q, categories: COUNTED }),
        ipc.summarizeTasks({ ...q, categories: OPEN }),
        ipc.listTasks({ ...q, categories: OPEN, due_to: isoDate(yesterday), limit: 1 }),
      ])
      return { done: all.done, total: all.total, estimate: left.estimate_minutes, overdue: late.total }
    })()

    Promise.all([work, s.kind === 'inbox' ? Promise.resolve([]) : ipc.occurrences(now.getTime(), meetingsTo)])
      .then(([w, occ]) => {
        if (stale) return
        done = w.done
        total = w.total
        estimateLeft = w.estimate
        overdue = w.overdue
        meetings = occ.filter((o) => o.kind === 'meeting' && o.status !== 'cancelled' && inScope(o, s))
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  })

  const progressName = $derived.by(() => {
    switch (scope.kind) {
      case 'today':
        return "Today's progress"
      case 'upcoming':
        return 'Progress this week'
      case 'inbox':
        return 'Inbox progress'
      case 'all':
        return 'Progress on all work'
      case 'section':
        return 'Section progress'
      case 'client':
        return `${cap(itemName(sectionById(app.clients.find((c) => c.id === scope.id)?.section_id)))} progress`
      default:
        return 'Project progress'
    }
  })
  const pct = $derived(total ? (done / total) * 100 : 0)
  const next = $derived(meetings.length ? meetings.reduce((a, b) => (a.start_at <= b.start_at ? a : b)) : null)
  const nextFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', hour: 'numeric', minute: '2-digit' })
  const meetingWord = $derived(scope.kind === 'today' ? 'left today' : 'this week')
</script>

<span class="flex min-w-0 items-center gap-3 self-center text-xs text-muted">
  {#if total}
    <span class="flex shrink-0 items-center gap-2 @max-[300px]:hidden" title="{done} of {total} tasks done{scope.kind === 'today' ? ' (finished today, of everything due)' : ''}">
      <Progress value={pct} width={72} label name={progressName} />
      <span class="tabular text-subtle">{done}/{total}</span>
    </span>
  {/if}
  {#if estimateLeft}
    <span class="stat flex @max-[480px]:hidden" title="Estimated time for open tasks">
      <Clock size={13} /><span class="tabular">{minutes(estimateLeft)} left</span>
    </span>
  {/if}
  {#if meetings.length}
    <span class="stat flex @max-[600px]:hidden" title={next ? `Next: ${next.title}, ${nextFmt.format(next.start_at)}` : ''}>
      <CalendarClock size={13} /><span class="tabular">{meetings.length} {meetings.length === 1 ? 'meeting' : 'meetings'} {meetingWord}</span>
    </span>
  {/if}
  {#if overdue}
    <span class="stat flex text-danger @max-[360px]:hidden" title="Open tasks past their due date">
      <AlertTriangle size={13} /><span class="tabular">{overdue} overdue</span>
    </span>
  {/if}
</span>

<style>
  .stat {
    flex-shrink: 0;
    align-items: center;
    gap: 0.3rem;
    white-space: nowrap;
  }
</style>
