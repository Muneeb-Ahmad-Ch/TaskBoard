// Display helpers. Date *logic* lives in the core; this only formats labels.

/** Today's civil date in local time, `YYYY-MM-DD`. */
export function todayIso(): string {
  const d = new Date()
  return isoDate(d)
}

export function isoDate(d: Date): string {
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${m}-${day}`
}

function parseIso(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number)
  return new Date(y, (m ?? 1) - 1, d ?? 1)
}

function dayDiff(iso: string, today = todayIso()): number {
  return Math.round((parseIso(iso).getTime() - parseIso(today).getTime()) / 86_400_000)
}

const weekday = new Intl.DateTimeFormat(undefined, { weekday: 'short' })
const monthDay = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
const fullDate = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' })

/** "Today", "Tomorrow", "Yesterday", "Thu", "Oct 22", "Oct 22, 2027". */
export function dueLabel(iso: string | null): string {
  if (!iso) return ''
  const diff = dayDiff(iso)
  if (diff === 0) return 'Today'
  if (diff === 1) return 'Tomorrow'
  if (diff === -1) return 'Yesterday'
  const date = parseIso(iso)
  if (diff > 1 && diff < 7) return weekday.format(date)
  if (date.getFullYear() !== new Date().getFullYear()) return fullDate.format(date)
  return monthDay.format(date)
}

export function isOverdue(iso: string | null, done: boolean): boolean {
  return !!iso && !done && dayDiff(iso) < 0
}

export function isDueToday(iso: string | null): boolean {
  return !!iso && dayDiff(iso) === 0
}

/** "1h 30m", "45m", "2h". */
export function minutes(total: number | null): string {
  if (total === null || total === undefined) return ''
  const h = Math.floor(total / 60)
  const m = total % 60
  if (h && m) return `${h}h ${m}m`
  if (h) return `${h}h`
  return `${m}m`
}

/** Parses "1h30m", "90", "1.5h", "45m" into minutes; null if empty, NaN if invalid. */
export function parseMinutes(input: string): number | null {
  const s = input.trim().toLowerCase().replace(/\s+/g, '')
  if (!s) return null
  if (/^\d+$/.test(s)) return Number(s)
  const dec = s.match(/^(\d+(?:\.\d+)?)h$/)
  if (dec) return Math.round(Number(dec[1]) * 60)
  const hm = s.match(/^(?:(\d+)h)?(?:(\d+)m(?:in)?)?$/)
  if (hm && (hm[1] || hm[2])) return Number(hm[1] ?? 0) * 60 + Number(hm[2] ?? 0)
  return Number.NaN
}

/** CSS color for a stored color token: an OKLCH hue string or "gray". */
export function swatch(color: string, l = 0.65, c = 0.15): string {
  return color === 'gray' ? `oklch(${l} 0 0)` : `oklch(${l} ${c} ${color})`
}

/** True when a key event comes from a text field (shortcuts should not fire). */
export function isTyping(e: Event): boolean {
  const t = e.target
  return t instanceof Element && !!t.closest('input, textarea, select, [contenteditable]')
}

/** Selectable colors for clients, projects and columns (OKLCH hues + gray). */
export const PALETTE = ['gray', '230', '262', '300', '330', '10', '45', '70', '100', '150', '190', '210']

export function plural(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? '' : 's'}`
}
