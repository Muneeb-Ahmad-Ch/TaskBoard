import { describe, expect, it } from 'vitest'
import { repeatOptions, repeatRule } from './repeat'

describe('repeatRule', () => {
  // Wednesday 2026-10-21
  const wed = '2026-10-21'
  it('builds a rule anchored on the chosen day', () => {
    expect(repeatRule('none', wed)).toBeUndefined()
    expect(repeatRule('daily', wed)).toBe('FREQ=DAILY')
    expect(repeatRule('weekdays', wed)).toBe('FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR')
    expect(repeatRule('weekly', wed)).toBe('FREQ=WEEKLY;BYDAY=WE')
    expect(repeatRule('biweekly', wed)).toBe('FREQ=WEEKLY;INTERVAL=2;BYDAY=WE')
    expect(repeatRule('monthly', wed)).toBe('FREQ=MONTHLY;BYMONTHDAY=21')
    expect(repeatRule('monthly-last', '2026-10-31')).toBe('FREQ=MONTHLY;BYMONTHDAY=-1')
    expect(repeatRule('yearly', wed)).toBe('FREQ=YEARLY')
  })
})

describe('repeatOptions', () => {
  it('names the day in each choice', () => {
    const labels = repeatOptions('2026-10-21').map((o) => o.label)
    expect(labels).toContain('Every Wednesday')
    expect(labels).toContain('Every month on the 21st')
    expect(labels).toContain('Every year on Oct 21')
    expect(labels).not.toContain('Every month on the last day')
  })
  it('offers the last day of the month on a month end', () => {
    const opts = repeatOptions('2026-10-31')
    expect(opts.map((o) => o.value)).toContain('monthly-last')
    expect(opts.find((o) => o.value === 'monthly')?.label).toBe('Every month on the 31st (skips shorter months)')
  })
})
