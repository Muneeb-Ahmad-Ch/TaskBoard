import { describe, expect, it } from 'vitest'
import type { TaskRow } from './bindings/TaskRow'
import { flattenGroups, groupRows, type GroupBy, type GroupContext } from './grouping'

const row = (id: string, over: Partial<TaskRow> = {}): TaskRow => ({
  id,
  title: id,
  priority: 'none',
  status_id: 's',
  status_name: 'To Do',
  status_color: 'gray',
  category: 'todo',
  project_id: 'p1',
  project_name: 'Website',
  project_color: '262',
  client_id: null,
  client_name: null,
  parent_id: null,
  start_date: null,
  due_date: null,
  due_time: null,
  estimate_minutes: null,
  completed_at: null,
  sort_key: 'a0',
  subtasks_total: 0,
  subtasks_done: 0,
  tags: [],
  progress: 0,
  recurring: false,
  parent_title: null,
  archived_at: null,
  created_at: 0,
  ...over,
})

const ctx: GroupContext = {
  columns: ['backlog', 'to do', 'in progress', 'waiting', 'done'],
  projects: ['p2', 'p1'],
  clients: ['c2', 'c1'],
  now: new Date(2026, 9, 21, 15, 30),
}

const labels = (rows: TaskRow[], by: GroupBy) => groupRows(rows, by, ctx).map((g) => `${g.label}:${g.rows.map((r) => r.id).join('')}`)

describe('groupRows', () => {
  it('none: one group, rows untouched', () => {
    const rows = [row('a'), row('b')]
    expect(groupRows(rows, 'none', ctx)).toEqual([{ key: 'all', label: '', rows }])
  })

  it('status: board order, same names merged, unknown columns last', () => {
    const rows = [row('a', { status_name: 'Done' }), row('b', { status_name: 'waiting' }), row('c', { status_name: 'Waiting' }), row('d', { status_name: 'Odd' })]
    expect(labels(rows, 'status')).toEqual(['waiting:bc', 'Done:a', 'Odd:d'])
  })

  it('priority: urgent first, keeps the order rows came in', () => {
    const rows = [row('a', { priority: 'low' }), row('b', { priority: 'urgent' }), row('c', { priority: 'low' })]
    expect(labels(rows, 'priority')).toEqual(['Urgent:b', 'Low:ac'])
  })

  it('project and group follow the sidebar order; no group last', () => {
    const rows = [
      row('a', { project_id: 'p1', project_name: 'Website', client_id: 'c1', client_name: 'Acme' }),
      row('b', { project_id: 'p2', project_name: 'App', client_id: 'c2', client_name: 'Beta' }),
      row('c', { project_id: 'p3', project_name: 'Misc' }),
    ]
    expect(labels(rows, 'project')).toEqual(['Beta / App:b', 'Acme / Website:a', 'Misc:c'])
    expect(labels(rows, 'client')).toEqual(['Beta:b', 'Acme:a', 'No group:c'])
  })

  it('due: overdue, today, tomorrow, next 7 days, later, no date', () => {
    const rows = [
      row('n'),
      row('l', { due_date: '2026-12-01' }),
      row('w', { due_date: '2026-10-25' }),
      row('m', { due_date: '2026-10-22' }),
      row('t', { due_date: '2026-10-21' }),
      row('o', { due_date: '2026-10-01' }),
      row('e', { due_date: '2026-10-01', completed_at: 1 }),
    ]
    expect(labels(rows, 'due')).toEqual(['Overdue:o', 'Earlier:e', 'Today:t', 'Tomorrow:m', 'Next 7 days:w', 'Later:l', 'No date:n'])
  })

  it('added: today, yesterday, last 7 days, last 30 days, older', () => {
    const at = (d: number, h = 9) => new Date(2026, 9, d, h).getTime()
    const rows = [row('o', { created_at: new Date(2026, 8, 1).getTime() }), row('m', { created_at: at(5) }), row('w', { created_at: at(16) }), row('y', { created_at: at(20, 23) }), row('t', { created_at: at(21, 0) })]
    expect(labels(rows, 'added')).toEqual(['Today:t', 'Yesterday:y', 'Last 7 days:w', 'Last 30 days:m', 'Older:o'])
  })
})

describe('flattenGroups', () => {
  it('a header per group; collapsed groups hide their rows', () => {
    const groups = groupRows([row('a', { priority: 'high' }), row('b', { priority: 'low' })], 'priority', ctx)
    const items = flattenGroups(groups, new Set(['high']))
    expect(items.map((i) => (i.kind === 'group' ? `[${i.group.key}${i.collapsed ? '+' : ''}]` : i.row.id))).toEqual(['[high+]', '[low]', 'b'])
  })

  it('no headers when not grouped', () => {
    const groups = groupRows([row('a')], 'none', ctx)
    expect(flattenGroups(groups, new Set()).map((i) => i.kind)).toEqual(['row'])
  })
})
