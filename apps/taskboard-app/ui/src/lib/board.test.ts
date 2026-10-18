import { describe, expect, it } from 'vitest'
import type { Status } from './bindings/Status'
import type { TaskRow } from './bindings/TaskRow'
import { buildColumns, buildLanes, laneKey } from './board'

const status = (project_id: string, name: string, category: Status['category'], sort_key: string): Status => ({
  id: `${project_id}-${name}`,
  project_id,
  name,
  color: 'gray',
  category,
  sort_key,
  wip_limit: null,
})

const row = (over: Partial<TaskRow>): TaskRow => ({
  id: 't',
  title: 'x',
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
  ...over,
})

describe('buildColumns', () => {
  it('keeps one project’s own order', () => {
    const cols = buildColumns([status('p1', 'Later', 'todo', 'b'), status('p1', 'Now', 'active', 'a')], ['p1'])
    expect(cols.map((c) => c.name)).toEqual(['Now', 'Later'])
    expect(cols[0].status?.id).toBe('p1-Now')
  })
  it('merges same-named statuses across projects and orders by category', () => {
    const cols = buildColumns(
      [status('p1', 'Done', 'done', 'c'), status('p1', 'To Do', 'todo', 'a'), status('p2', 'to do', 'todo', 'a'), status('p2', 'Doing', 'active', 'b')],
      ['p1', 'p2'],
    )
    expect(cols.map((c) => c.name)).toEqual(['To Do', 'Doing', 'Done'])
    expect(cols[0].statusIds.get('p2')).toBe('p2-to do')
    expect(cols[0].status).toBeNull()
  })
  it('ignores statuses of projects out of scope', () => {
    expect(buildColumns([status('p9', 'X', 'todo', 'a')], ['p1'])).toEqual([])
  })
})

describe('lanes by group', () => {
  const sectionOf = (projectId: string) => (projectId === 'p2' ? { id: 's1', name: 'Personal' } : undefined)
  const rows = [
    row({ id: 'a', project_id: 'p1', client_id: 'c1', client_name: 'Zed Ltd' }),
    row({ id: 'b', project_id: 'p2', client_id: null }),
    row({ id: 'c', project_id: 'p3', client_id: 'c2', client_name: 'Acme' }),
    row({ id: 'd', project_id: 'p4', client_id: null }),
  ]
  it('keys groups by group, groupless projects by section, the rest as none', () => {
    expect(laneKey(rows[0], 'client', sectionOf)).toBe('c1')
    expect(laneKey(rows[1], 'client', sectionOf)).toBe('s:s1')
    expect(laneKey(rows[3], 'client', sectionOf)).toBe('none')
  })
  it('lists groups alphabetically, then sections, then no group', () => {
    const lanes = buildLanes(rows, 'client', sectionOf)
    expect(lanes.map((l) => l.label)).toEqual(['Acme', 'Zed Ltd', 'Personal', 'No group'])
  })
  it('has one lane when lanes are off', () => {
    expect(buildLanes(rows, 'none')).toEqual([{ key: 'all', label: '' }])
  })
})
