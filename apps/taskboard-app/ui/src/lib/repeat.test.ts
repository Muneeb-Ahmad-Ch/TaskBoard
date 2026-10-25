import { describe, expect, it } from 'vitest'
import { repeatOptions } from './cal.svelte'
import { describeRule } from './repeat'

describe('describeRule', () => {
  it('says common rules in plain words', () => {
    expect(describeRule('FREQ=DAILY')).toBe('Every day')
    expect(describeRule('FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR')).toBe('Every weekday (Mon–Fri)')
    expect(describeRule('FREQ=WEEKLY;INTERVAL=2;BYDAY=WE')).toBe('Every 2 weeks on Wed')
    expect(describeRule('FREQ=MONTHLY;BYMONTHDAY=15')).toBe('Every month on the 15th')
    expect(describeRule('FREQ=MONTHLY;BYMONTHDAY=-1')).toBe('Every month on the last day')
    expect(describeRule('FREQ=MONTHLY;BYDAY=-1FR')).toBe('Every month on the last Fri')
    expect(describeRule('FREQ=MONTHLY;BYDAY=2TU')).toBe('Every month on the 2nd Tue')
    expect(describeRule('FREQ=YEARLY')).toBe('Every year')
    expect(describeRule('FREQ=DAILY;COUNT=5')).toBe('Every day, 5 times')
    expect(describeRule('FREQ=WEEKLY;UNTIL=20261231')).toBe('Every week, until Dec 31, 2026')
  })
  it('says repeats after completion', () => {
    expect(describeRule('AFTER:1D')).toBe('1 day after done')
    expect(describeRule('AFTER:3W')).toBe('3 weeks after done')
    expect(describeRule('AFTER:2M')).toBe('2 months after done')
  })
  it('falls back to the rule itself when it is unusual', () => {
    expect(describeRule('FREQ=HOURLY')).toBe('FREQ=HOURLY')
  })
})

describe('repeatOptions', () => {
  it('offers the last day of the month on a month end', () => {
    const opts = repeatOptions(new Date(2026, 9, 31))
    expect(opts.map((o) => o.rule)).toContain('FREQ=MONTHLY;BYMONTHDAY=-1')
    expect(opts.find((o) => o.rule === 'FREQ=MONTHLY')?.label).toBe('Monthly on day 31 (skips shorter months)')
  })
  it('keeps a plain day of the month on other days', () => {
    const opts = repeatOptions(new Date(2026, 9, 21))
    expect(opts.map((o) => o.rule)).not.toContain('FREQ=MONTHLY;BYMONTHDAY=-1')
    expect(opts.find((o) => o.rule === 'FREQ=MONTHLY')?.label).toBe('Monthly on day 21')
  })
})
