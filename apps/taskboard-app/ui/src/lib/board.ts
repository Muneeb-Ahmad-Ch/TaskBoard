// Board layout logic: merging per-project statuses into columns and grouping
// cards into swimlanes. Pure functions; no I/O.
import type { Category } from './bindings/Category'
import type { Id } from './bindings/Id'
import type { Priority } from './bindings/Priority'
import type { Status } from './bindings/Status'
import type { TaskRow } from './bindings/TaskRow'
import type { Lanes } from './store.svelte'

export interface Column {
  /** Lower-cased status name; same-named statuses across projects merge. */
  key: string
  name: string
  color: string
  category: Category
  /** project id → that project's status for this column. */
  statusIds: Map<Id, Id>
  /** The single underlying status when the board shows one project. */
  status: Status | null
}

const RANK: Record<Category, number> = { todo: 0, active: 1, done: 2, cancelled: 3 }

export function buildColumns(statuses: Status[], projectIds: Id[]): Column[] {
  const inScope = new Set(projectIds)
  const byProject = new Map<Id, Status[]>()
  for (const s of statuses) {
    if (!inScope.has(s.project_id)) continue
    const list = byProject.get(s.project_id) ?? []
    list.push(s)
    byProject.set(s.project_id, list)
  }
  const single = byProject.size === 1
  const cols = new Map<string, Column & { pos: number }>()
  for (const [projectId, list] of byProject) {
    list.sort((a, b) => (a.sort_key < b.sort_key ? -1 : 1))
    list.forEach((s, i) => {
      const key = s.name.trim().toLowerCase()
      const existing = cols.get(key)
      if (existing) {
        existing.pos = Math.min(existing.pos, i)
        existing.statusIds.set(projectId, s.id)
      } else {
        cols.set(key, {
          key,
          name: s.name,
          color: s.color,
          category: s.category,
          statusIds: new Map([[projectId, s.id]]),
          status: single ? s : null,
          pos: i,
        })
      }
    })
  }
  return [...cols.values()].sort((a, b) =>
    single ? a.pos - b.pos : RANK[a.category] - RANK[b.category] || a.pos - b.pos,
  )
}

export interface Lane {
  key: string
  label: string
  color?: string
}

const PRIORITY_ORDER: Priority[] = ['urgent', 'high', 'med', 'low', 'none']
const PRIORITY_LABEL: Record<Priority, string> = {
  urgent: 'Urgent',
  high: 'High',
  med: 'Medium',
  low: 'Low',
  none: 'No priority',
}

/** Where a project sits, for lanes of projects that are in no group. */
export type SectionOf = (projectId: Id) => { id: Id; name: string } | undefined

export function laneKey(row: TaskRow, lanes: Lanes, sectionOf?: SectionOf): string {
  switch (lanes) {
    case 'client': {
      if (row.client_id) return row.client_id
      const section = sectionOf?.(row.project_id)
      return section ? `s:${section.id}` : 'none'
    }
    case 'project':
      return row.project_id
    case 'priority':
      return row.priority
    default:
      return 'all'
  }
}

/** Lanes present in the loaded cards, in a stable, meaningful order. */
export function buildLanes(rows: TaskRow[], lanes: Lanes, sectionOf?: SectionOf): Lane[] {
  if (lanes === 'none') return [{ key: 'all', label: '' }]
  if (lanes === 'priority') {
    return PRIORITY_ORDER.map((p) => ({ key: p, label: PRIORITY_LABEL[p], color: `var(--priority-${p})` }))
  }
  const seen = new Map<string, Lane>()
  for (const r of rows) {
    const key = laneKey(r, lanes, sectionOf)
    if (seen.has(key)) continue
    if (lanes === 'client') seen.set(key, { key, label: r.client_name ?? sectionOf?.(r.project_id)?.name ?? 'No group' })
    else
      seen.set(key, {
        key,
        label: r.client_name ? `${r.client_name} / ${r.project_name}` : r.project_name,
        color: r.project_color,
      })
  }
  // Groups first, then each section's groupless projects, then anything left.
  const rank = (key: string) => (key === 'none' ? 2 : key.startsWith('s:') ? 1 : 0)
  return [...seen.values()].sort((a, b) => rank(a.key) - rank(b.key) || a.label.localeCompare(b.label))
}
