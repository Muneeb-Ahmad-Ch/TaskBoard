// "Repeat" choices for a reminder, worded for the chosen day, and the rule
// (RRULE) each one stores on the task. Pure functions; no I/O.

export type RepeatChoice = 'none' | 'daily' | 'weekdays' | 'weekly' | 'biweekly' | 'monthly' | 'monthly-last' | 'yearly'

const CODES = ['SU', 'MO', 'TU', 'WE', 'TH', 'FR', 'SA']
const weekdayName = new Intl.DateTimeFormat('en', { weekday: 'long' })
const monthDay = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric' })

function parse(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number)
  return new Date(y, (m ?? 1) - 1, d ?? 1)
}

function ordinal(n: number): string {
  const tail = n % 100 >= 11 && n % 100 <= 13 ? 'th' : (['th', 'st', 'nd', 'rd'][n % 10] ?? 'th')
  return `${n}${tail}`
}

function isLastDay(d: Date): boolean {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getDate() === 1
}

/** The stored rule for a choice, anchored on `date` (YYYY-MM-DD). */
export function repeatRule(choice: RepeatChoice, date: string): string | undefined {
  const d = parse(date)
  const day = CODES[d.getDay()]
  switch (choice) {
    case 'none':
      return undefined
    case 'daily':
      return 'FREQ=DAILY'
    case 'weekdays':
      return 'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR'
    case 'weekly':
      return `FREQ=WEEKLY;BYDAY=${day}`
    case 'biweekly':
      return `FREQ=WEEKLY;INTERVAL=2;BYDAY=${day}`
    case 'monthly':
      return `FREQ=MONTHLY;BYMONTHDAY=${d.getDate()}`
    case 'monthly-last':
      return 'FREQ=MONTHLY;BYMONTHDAY=-1'
    case 'yearly':
      return 'FREQ=YEARLY'
  }
}

/** The choices for `date`, each worded for that day. */
export function repeatOptions(date: string): { value: RepeatChoice; label: string }[] {
  const d = parse(date)
  const n = d.getDate()
  const day = weekdayName.format(d)
  const opts: { value: RepeatChoice; label: string }[] = [
    { value: 'none', label: 'Does not repeat' },
    { value: 'daily', label: 'Every day' },
    { value: 'weekdays', label: 'Every weekday (Mon–Fri)' },
    { value: 'weekly', label: `Every ${day}` },
    { value: 'biweekly', label: `Every other ${day}` },
    { value: 'monthly', label: `Every month on the ${ordinal(n)}${n > 28 ? ' (skips shorter months)' : ''}` },
  ]
  if (isLastDay(d)) opts.push({ value: 'monthly-last', label: 'Every month on the last day' })
  opts.push({ value: 'yearly', label: `Every year on ${monthDay.format(d)}` })
  return opts
}
