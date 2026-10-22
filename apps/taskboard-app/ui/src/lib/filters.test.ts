import { describe, expect, it } from 'vitest'
import { applyFilter, filterCount, NO_FILTER, type TaskFilter } from './filters'

// Wednesday 2026-10-21, mid-afternoon.
const now = new Date(2026, 9, 21, 15, 30)
const f = (over: Partial<TaskFilter>): TaskFilter => ({ ...NO_FILTER, ...over })

describe('applyFilter', () => {
  it('leaves the query alone with no filter', () => {
    expect(applyFilter({ sort: 'due', project_id: 'p' }, NO_FILTER, now)).toEqual({ sort: 'due', project_id: 'p' })
  })

  it('passes the lists through', () => {
    const q = applyFilter({}, f({ priorities: ['high'], statuses: ['Waiting'], tags: ['client'], projects: ['p1'] }), now)
    expect(q).toEqual({ priorities: ['high'], status_names: ['Waiting'], tags: ['client'], project_ids: ['p1'] })
  })

  it('overdue: due before today, and only open tasks', () => {
    expect(applyFilter({ categories: [] }, f({ due: 'overdue' }), now)).toEqual({ categories: ['todo', 'active'], due_to: '2026-10-20' })
  })

  it('today and next 7 days narrow an existing date window', () => {
    expect(applyFilter({}, f({ due: 'today' }), now)).toEqual({ due_from: '2026-10-21', due_to: '2026-10-21' })
    // Upcoming (tomorrow..+7) ∩ next 7 days (today..+6).
    const upcoming = { due_from: '2026-10-22', due_to: '2026-10-28' }
    expect(applyFilter(upcoming, f({ due: 'week' }), now)).toEqual({ due_from: '2026-10-22', due_to: '2026-10-27' })
  })

  it('no date / has a date', () => {
    expect(applyFilter({}, f({ due: 'none' }), now)).toEqual({ undated: true })
    expect(applyFilter({}, f({ due: 'dated' }), now)).toEqual({ dated: true })
  })

  it('added: from the start of today, 7 or 30 days back', () => {
    const day = (y: number, m: number, d: number) => new Date(y, m, d).getTime()
    expect(applyFilter({}, f({ added: 'today' }), now).created_from).toBe(day(2026, 9, 21))
    expect(applyFilter({}, f({ added: 'week' }), now).created_from).toBe(day(2026, 9, 15))
    expect(applyFilter({}, f({ added: 'month' }), now).created_from).toBe(day(2026, 8, 22))
  })
})

describe('filterCount', () => {
  it('counts each kind of filter once', () => {
    expect(filterCount(NO_FILTER)).toBe(0)
    expect(filterCount(f({ priorities: ['high', 'urgent'], tags: ['a'], due: 'today' }))).toBe(3)
  })
})
