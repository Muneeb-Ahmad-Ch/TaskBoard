// A stored repeat rule (RRULE, or AFTER:<n><D|W|M>) in plain words, for any
// rule the Repeat menus don't list ("Every month on the 15th", not
// "FREQ=MONTHLY;BYMONTHDAY=15"). Pure functions; no I/O.

const DAY_NAMES: Record<string, string> = { MO: 'Mon', TU: 'Tue', WE: 'Wed', TH: 'Thu', FR: 'Fri', SA: 'Sat', SU: 'Sun' }
const UNITS: Record<string, string> = { DAILY: 'day', WEEKLY: 'week', MONTHLY: 'month', YEARLY: 'year' }
const untilFmt = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric', year: 'numeric' })

function ordinal(n: number): string {
  const tail = n % 100 >= 11 && n % 100 <= 13 ? 'th' : (['th', 'st', 'nd', 'rd'][n % 10] ?? 'th')
  return `${n}${tail}`
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`
}

/** One BYDAY item ("MO", "2TU", "-1FR") in words, or null if it isn't one. */
function dayItem(item: string): string | null {
  const m = item.match(/^(-?\d)?([A-Z]{2})$/)
  const name = m && DAY_NAMES[m[2]]
  if (!m || !name) return null
  if (!m[1]) return name
  const n = Number(m[1])
  return n === -1 ? `last ${name}` : `${ordinal(n)} ${name}`
}

export function describeRule(rule: string): string {
  const after = rule.match(/^AFTER:(\d+)([DWM])$/i)
  if (after) {
    const unit = { D: 'day', W: 'week', M: 'month' }[after[2].toUpperCase() as 'D' | 'W' | 'M']
    return `${plural(Number(after[1]), unit)} after done`
  }
  const parts = new Map(
    rule
      .replace(/^RRULE:/, '')
      .split(';')
      .map((p) => p.split('=') as [string, string]),
  )
  const unit = UNITS[parts.get('FREQ') ?? '']
  if (!unit) return rule
  const interval = Number(parts.get('INTERVAL') ?? 1)
  const byday = parts.get('BYDAY')?.split(',') ?? []
  if (unit === 'week' && interval === 1 && byday.join(',') === 'MO,TU,WE,TH,FR') return withEnd('Every weekday (Mon–Fri)', parts)
  let text = interval > 1 ? `Every ${interval} ${unit}s` : `Every ${unit}`
  if (byday.length) {
    const days = byday.map(dayItem)
    if (days.some((d) => d === null)) return rule
    text += unit === 'month' ? ` on the ${days.join(', ')}` : ` on ${days.join(', ')}`
  }
  const bymonthday = parts.get('BYMONTHDAY')
  if (bymonthday) {
    const days = bymonthday.split(',').map(Number)
    if (days.some((d) => !Number.isInteger(d) || d === 0)) return rule
    text += ` on the ${days.map((d) => (d === -1 ? 'last day' : ordinal(d))).join(', ')}`
  }
  return withEnd(text, parts)
}

function withEnd(text: string, parts: Map<string, string>): string {
  const count = parts.get('COUNT')
  if (count) return `${text}, ${plural(Number(count), 'time')}`
  const until = parts.get('UNTIL')?.slice(0, 8)
  if (until && /^\d{8}$/.test(until)) {
    const d = new Date(Number(until.slice(0, 4)), Number(until.slice(4, 6)) - 1, Number(until.slice(6, 8)))
    return `${text}, until ${untilFmt.format(d)}`
  }
  return text
}
