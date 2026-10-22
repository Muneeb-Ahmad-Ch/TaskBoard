// The Filter menu shared by the table and the board: what can be picked, and
// how a pick narrows the list's task query. Pure functions; no I/O.
import type { Category } from './bindings/Category'
import type { Id } from './bindings/Id'
import type { Priority } from './bindings/Priority'
import type { TaskQuery } from './bindings/TaskQuery'
import { isoDate } from './format'

export type DueFilter = 'overdue' | 'today' | 'week' | 'none' | 'dated'
export type AddedFilter = 'today' | 'week' | 'month'

export interface TaskFilter {
  priorities: Priority[]
  /** Board column names (same-named columns across projects count as one). */
  statuses: string[]
  /** Any of these tags. */
  tags: string[]
  projects: Id[]
  due: DueFilter | null
  added: AddedFilter | null
}

export const NO_FILTER: TaskFilter = { priorities: [], statuses: [], tags: [], projects: [], due: null, added: null }

export const DUE_LABEL: Record<DueFilter, string> = {
  overdue: 'Overdue',
  today: 'Due today',
  week: 'Next 7 days',
  none: 'No due date',
  dated: 'Has a due date',
}

export const ADDED_LABEL: Record<AddedFilter, string> = {
  today: 'Added today',
  week: 'Last 7 days',
  month: 'Last 30 days',
}

const OPEN: Category[] = ['todo', 'active']

/** How many kinds of filter are on (a badge on the Filter button). */
export function filterCount(f: TaskFilter): number {
  return [f.priorities.length, f.statuses.length, f.tags.length, f.projects.length, f.due, f.added].filter(Boolean).length
}

function daysFrom(now: Date, days: number): Date {
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + days)
}

const later = (a: string | undefined, b: string) => (a && a > b ? a : b)
const earlier = (a: string | undefined, b: string) => (a && a < b ? a : b)

/** `q` narrowed by the filter; date picks intersect a window the list already has. */
export function applyFilter(q: TaskQuery, f: TaskFilter, now: Date): TaskQuery {
  const out: TaskQuery = { ...q }
  if (f.priorities.length) out.priorities = [...f.priorities]
  if (f.statuses.length) out.status_names = [...f.statuses]
  if (f.tags.length) out.tags = [...f.tags]
  if (f.projects.length) out.project_ids = [...f.projects]
  const today = isoDate(now)
  switch (f.due) {
    case 'overdue':
      // Overdue means not finished yet.
      out.due_to = earlier(out.due_to, isoDate(daysFrom(now, -1)))
      out.categories = out.categories?.length ? out.categories.filter((c) => OPEN.includes(c)) : [...OPEN]
      break
    case 'today':
      out.due_from = later(out.due_from, today)
      out.due_to = earlier(out.due_to, today)
      break
    case 'week':
      out.due_from = later(out.due_from, today)
      out.due_to = earlier(out.due_to, isoDate(daysFrom(now, 6)))
      break
    case 'none':
      out.undated = true
      break
    case 'dated':
      out.dated = true
      break
  }
  if (f.added) {
    const back = f.added === 'today' ? 0 : f.added === 'week' ? -6 : -29
    out.created_from = daysFrom(now, back).getTime()
  }
  return out
}
