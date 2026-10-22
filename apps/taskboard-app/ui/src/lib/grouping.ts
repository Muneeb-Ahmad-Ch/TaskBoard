// Table grouping: rows into labelled groups in a meaningful order, and the
// flat header + row list the virtualized table draws. Pure functions; no I/O.
import type { Id } from './bindings/Id'
import type { Priority } from './bindings/Priority'
import type { TaskRow } from './bindings/TaskRow'
import { isoDate } from './format'

export type GroupBy = 'none' | 'status' | 'priority' | 'project' | 'client' | 'due' | 'added'

export const GROUP_LABEL: Record<GroupBy, string> = {
  none: 'No grouping',
  status: 'Status',
  priority: 'Priority',
  project: 'Project',
  client: 'Group',
  due: 'Due date',
  added: 'Date added',
}

export interface Group {
  key: string
  label: string
  /** Dot color: an OKLCH hue, "gray", or a CSS value starting with `var(`. */
  color?: string
  rows: TaskRow[]
}

export type GroupItem = { kind: 'group'; group: Group; collapsed: boolean } | { kind: 'row'; row: TaskRow }

export interface GroupContext {
  /** Board columns in order, as lower-cased names. */
  columns: string[]
  /** Project and group ids in sidebar order. */
  projects: Id[]
  clients: Id[]
  now: Date
}

const PRIORITIES: Priority[] = ['urgent', 'high', 'med', 'low', 'none']
const PRIORITY_LABEL: Record<Priority, string> = { urgent: 'Urgent', high: 'High', med: 'Medium', low: 'Low', none: 'No priority' }

const DUE_BUCKETS = ['overdue', 'earlier', 'today', 'tomorrow', 'week', 'later', 'none'] as const
const DUE_LABEL: Record<(typeof DUE_BUCKETS)[number], string> = {
  overdue: 'Overdue',
  earlier: 'Earlier',
  today: 'Today',
  tomorrow: 'Tomorrow',
  week: 'Next 7 days',
  later: 'Later',
  none: 'No date',
}

const ADDED_BUCKETS = ['today', 'yesterday', 'week', 'month', 'older'] as const
const ADDED_LABEL: Record<(typeof ADDED_BUCKETS)[number], string> = {
  today: 'Today',
  yesterday: 'Yesterday',
  week: 'Last 7 days',
  month: 'Last 30 days',
  older: 'Older',
}

function dayStart(now: Date, days: number): number {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + days).getTime()
}

function dueBucket(r: TaskRow, now: Date): (typeof DUE_BUCKETS)[number] {
  if (!r.due_date) return 'none'
  const day = (n: number) => isoDate(new Date(dayStart(now, n)))
  if (r.due_date < day(0)) return r.completed_at ? 'earlier' : 'overdue'
  if (r.due_date === day(0)) return 'today'
  if (r.due_date === day(1)) return 'tomorrow'
  return r.due_date <= day(7) ? 'week' : 'later'
}

function addedBucket(r: TaskRow, now: Date): (typeof ADDED_BUCKETS)[number] {
  if (r.created_at >= dayStart(now, 0)) return 'today'
  if (r.created_at >= dayStart(now, -1)) return 'yesterday'
  if (r.created_at >= dayStart(now, -6)) return 'week'
  return r.created_at >= dayStart(now, -29) ? 'month' : 'older'
}

/** Groups in display order; rows keep the order they came in (the list's sort). */
export function groupRows(rows: TaskRow[], by: GroupBy, ctx: GroupContext): Group[] {
  if (by === 'none') return [{ key: 'all', label: '', rows }]
  const groups = new Map<string, Group & { rank: number }>()
  const index = (list: string[], key: string) => {
    const i = list.indexOf(key)
    return i < 0 ? list.length : i
  }
  for (const r of rows) {
    let g: Omit<Group, 'rows'> & { rank: number }
    switch (by) {
      case 'status': {
        const key = r.status_name.trim().toLowerCase()
        g = { key, label: r.status_name, color: r.status_color, rank: index(ctx.columns, key) }
        break
      }
      case 'priority':
        g = { key: r.priority, label: PRIORITY_LABEL[r.priority], color: `var(--priority-${r.priority})`, rank: PRIORITIES.indexOf(r.priority) }
        break
      case 'project':
        g = {
          key: r.project_id,
          label: r.client_name ? `${r.client_name} / ${r.project_name}` : r.project_name,
          color: r.project_color,
          rank: index(ctx.projects, r.project_id),
        }
        break
      case 'client':
        g = r.client_id
          ? { key: r.client_id, label: r.client_name ?? 'Group', rank: index(ctx.clients, r.client_id) }
          : { key: 'none', label: 'No group', rank: ctx.clients.length + 1 }
        break
      case 'due': {
        const b = dueBucket(r, ctx.now)
        g = { key: b, label: DUE_LABEL[b], rank: DUE_BUCKETS.indexOf(b) }
        break
      }
      case 'added': {
        const b = addedBucket(r, ctx.now)
        g = { key: b, label: ADDED_LABEL[b], rank: ADDED_BUCKETS.indexOf(b) }
        break
      }
    }
    const existing = groups.get(g.key)
    if (existing) existing.rows.push(r)
    else groups.set(g.key, { ...g, rows: [r] })
  }
  return [...groups.values()]
    .sort((a, b) => a.rank - b.rank || a.label.localeCompare(b.label))
    .map(({ rank: _rank, ...g }) => g)
}

/** Header + rows per group (no headers when ungrouped); collapsed groups show only their header. */
export function flattenGroups(groups: Group[], collapsed: Set<string>): GroupItem[] {
  if (groups.length === 1 && groups[0].key === 'all' && !groups[0].label) return groups[0].rows.map((row) => ({ kind: 'row', row }))
  const items: GroupItem[] = []
  for (const group of groups) {
    const shut = collapsed.has(group.key)
    items.push({ kind: 'group', group, collapsed: shut })
    if (!shut) for (const row of group.rows) items.push({ kind: 'row', row })
  }
  return items
}
