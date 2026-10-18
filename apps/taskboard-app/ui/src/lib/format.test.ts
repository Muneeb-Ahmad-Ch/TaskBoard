import { describe, expect, it } from 'vitest'
import { cap, dueLabel, isDueToday, isOverdue, isoDate, minutes, parseMinutes, plural, singular, todayIso } from './format'

const shift = (days: number) => {
  const d = new Date()
  d.setDate(d.getDate() + days)
  return isoDate(d)
}

describe('dates', () => {
  it('labels today, tomorrow and yesterday by name', () => {
    expect(dueLabel(todayIso())).toBe('Today')
    expect(dueLabel(shift(1))).toBe('Tomorrow')
    expect(dueLabel(shift(-1))).toBe('Yesterday')
    expect(dueLabel(null)).toBe('')
  })
  it('knows overdue and due today', () => {
    expect(isOverdue(shift(-2), false)).toBe(true)
    expect(isOverdue(shift(-2), true)).toBe(false)
    expect(isOverdue(null, false)).toBe(false)
    expect(isDueToday(todayIso())).toBe(true)
    expect(isDueToday(shift(1))).toBe(false)
  })
})

describe('minutes', () => {
  it('formats hours and minutes', () => {
    expect(minutes(90)).toBe('1h 30m')
    expect(minutes(120)).toBe('2h')
    expect(minutes(45)).toBe('45m')
    expect(minutes(null)).toBe('')
  })
  it('parses what people type', () => {
    expect(parseMinutes('1h30m')).toBe(90)
    expect(parseMinutes('90')).toBe(90)
    expect(parseMinutes('1.5h')).toBe(90)
    expect(parseMinutes('45m')).toBe(45)
    expect(parseMinutes('')).toBeNull()
    expect(parseMinutes('soon')).toBeNaN()
  })
})

describe('words', () => {
  it('pluralizes, capitalizes and singularizes section names', () => {
    expect(plural(1, 'task')).toBe('1 task')
    expect(plural(2, 'task')).toBe('2 tasks')
    expect(cap('group')).toBe('Group')
    expect(singular('Clients')).toBe('client')
    expect(singular('Teams')).toBe('team')
    expect(singular('Families')).toBe('family')
    expect(singular('Branches')).toBe('branch')
    expect(singular('Personal')).toBe('personal')
  })
})
