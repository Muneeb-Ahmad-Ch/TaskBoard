// Calendar view state and helpers. All wall-clock math uses the system zone
// (the same zone the core uses); client zones are only used for display.
import type { SlotDay } from './bindings/SlotDay'
import type { EventDraft } from '../components/EventDialog.svelte'
import { isoDate } from './format'

export type CalMode = 'month' | 'week' | 'day' | 'agenda'

const LAYER_KEY = 'taskboard.cal.layers'
function loadLayers() {
  try {
    return { tasks: true, meetings: true, blocks: true, logged: true, ...JSON.parse(localStorage.getItem(LAYER_KEY) ?? '{}') }
  } catch {
    return { tasks: true, meetings: true, blocks: true, logged: true }
  }
}

export const cal = $state({
  mode: 'week' as CalMode,
  /** Any date inside the visible period. */
  anchor: startOfDay(new Date()),
  layers: loadLayers() as { tasks: boolean; meetings: boolean; blocks: boolean; logged: boolean },
  /** First day of the week: 1 = Monday, 0 = Sunday. */
  weekStart: 1,
  /** Open dialogs. */
  draft: null as EventDraft | null,
  workingHoursOpen: false,
  findSlotOpen: false,
})

export function saveLayers() {
  try {
    localStorage.setItem(LAYER_KEY, JSON.stringify(cal.layers))
  } catch {
    // per-device convenience only
  }
}

export const MINUTE = 60_000
export const HOUR = 60 * MINUTE

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate())
}

export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n)
}

export function startOfWeek(d: Date, weekStart = cal.weekStart): Date {
  const diff = (d.getDay() - weekStart + 7) % 7
  return addDays(startOfDay(d), -diff)
}

export function fromIso(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number)
  return new Date(y, (m ?? 1) - 1, d ?? 1)
}

export function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
}

/** The days shown by the current mode. */
export function visibleDays(mode: CalMode = cal.mode, anchor: Date = cal.anchor): Date[] {
  if (mode === 'day') return [startOfDay(anchor)]
  if (mode === 'week') {
    const s = startOfWeek(anchor)
    return Array.from({ length: 7 }, (_, i) => addDays(s, i))
  }
  if (mode === 'agenda') return Array.from({ length: 21 }, (_, i) => addDays(startOfDay(anchor), i))
  const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1)
  const s = startOfWeek(first)
  return Array.from({ length: 42 }, (_, i) => addDays(s, i))
}

export function shiftAnchor(dir: -1 | 1) {
  const a = cal.anchor
  if (cal.mode === 'month') cal.anchor = new Date(a.getFullYear(), a.getMonth() + dir, 1)
  else if (cal.mode === 'week') cal.anchor = addDays(a, 7 * dir)
  else if (cal.mode === 'agenda') cal.anchor = addDays(a, 21 * dir)
  else cal.anchor = addDays(a, dir)
}

const timeFmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })
export function fmtTime(ms: number): string {
  return timeFmt.format(ms)
}
export function fmtRange(a: number, b: number): string {
  return `${fmtTime(a)} – ${fmtTime(b)}`
}

export function periodTitle(): string {
  const days = visibleDays()
  if (cal.mode === 'month') return new Intl.DateTimeFormat(undefined, { month: 'long', year: 'numeric' }).format(cal.anchor)
  if (cal.mode === 'day')
    return new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' }).format(cal.anchor)
  const a = days[0]
  const b = days[days.length - 1]
  const md = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
  return `${md.format(a)} – ${md.format(b)}, ${b.getFullYear()}`
}

/** Side-by-side columns for overlapping timed items. */
export function layoutColumns<T extends { start: number; end: number }>(items: T[]): Map<T, { col: number; cols: number }> {
  const sorted = [...items].sort((a, b) => a.start - b.start || b.end - a.end)
  const out = new Map<T, { col: number; cols: number }>()
  let cluster: T[] = []
  let clusterEnd = -Infinity
  const columnsEnd: number[] = []
  const flush = () => {
    const cols = Math.max(1, ...cluster.map((i) => (out.get(i)?.col ?? 0) + 1))
    for (const i of cluster) {
      const v = out.get(i)
      if (v) v.cols = cols
    }
    cluster = []
    columnsEnd.length = 0
  }
  for (const item of sorted) {
    if (item.start >= clusterEnd && cluster.length) flush()
    let col = columnsEnd.findIndex((end) => end <= item.start)
    if (col < 0) col = columnsEnd.length
    columnsEnd[col] = item.end
    out.set(item, { col, cols: 1 })
    cluster.push(item)
    clusterEnd = Math.max(clusterEnd === -Infinity ? item.end : clusterEnd, item.end)
  }
  if (cluster.length) flush()
  return out
}

const DAY_CODES = ['SU', 'MO', 'TU', 'WE', 'TH', 'FR', 'SA']
const DAY_NAMES = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']

/** Common repeat choices for an event starting on `d`. */
export function repeatOptions(d: Date): { label: string; rule: string | null }[] {
  const nth = Math.ceil(d.getDate() / 7)
  const isLast = addDays(d, 7).getMonth() !== d.getMonth()
  const ordinal = ['', '1st', '2nd', '3rd', '4th', '5th'][nth]
  const code = DAY_CODES[d.getDay()]
  const name = DAY_NAMES[d.getDay()]
  return [
    { label: 'Does not repeat', rule: null },
    { label: 'Every day', rule: 'FREQ=DAILY' },
    { label: 'Every weekday (Mon–Fri)', rule: 'FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR' },
    { label: `Weekly on ${name}`, rule: `FREQ=WEEKLY;BYDAY=${code}` },
    { label: `Every 2 weeks on ${name}`, rule: `FREQ=WEEKLY;INTERVAL=2;BYDAY=${code}` },
    { label: `Monthly on day ${d.getDate()}`, rule: 'FREQ=MONTHLY' },
    isLast
      ? { label: `Monthly on the last ${name}`, rule: `FREQ=MONTHLY;BYDAY=-1${code}` }
      : { label: `Monthly on the ${ordinal} ${name}`, rule: `FREQ=MONTHLY;BYDAY=${nth}${code}` },
    { label: 'Every year', rule: 'FREQ=YEARLY' },
  ]
}

/** "Karachi (UTC+5)" — city from the IANA id plus the offset at `at`. */
export function zoneLabel(timeZone: string, at = Date.now()): string {
  const city = timeZone.split('/').pop()?.replace(/_/g, ' ') ?? timeZone
  try {
    const offset = new Intl.DateTimeFormat('en-US', { timeZone, timeZoneName: 'shortOffset' })
      .formatToParts(at)
      .find((p) => p.type === 'timeZoneName')
      ?.value?.replace('GMT', 'UTC')
    return offset ? `${city} (${offset === 'UTC' ? 'UTC' : offset})` : city
  } catch {
    return city
  }
}

/** Time in a given zone, e.g. "10:00". */
export function timeIn(ms: number, timeZone?: string): string {
  return new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit', timeZone }).format(ms)
}

/**
 * Plain-text availability for email/chat, e.g.
 * "Tue 21 Oct: 10:00–11:30, 15:00–17:00 (PKT, UTC+5)".
 */
export function availabilityText(days: SlotDay[], zones: string[]): string {
  const lines: string[] = []
  for (const zone of zones) {
    const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short', timeZone: zone })
    const byDay = new Map<string, string[]>()
    for (const d of days) {
      for (const w of d.windows) {
        const key = dayFmt.format(w.start_at)
        const list = byDay.get(key) ?? []
        list.push(`${timeIn(w.start_at, zone)}–${timeIn(w.end_at, zone)}`)
        byDay.set(key, list)
      }
    }
    if (zones.length > 1) lines.push(`${zoneLabel(zone)}:`)
    for (const [day, spans] of byDay) lines.push(`${day}: ${spans.join(', ')}${zones.length === 1 ? ` (${zoneLabel(zone)})` : ''}`)
    if (zones.length > 1) lines.push('')
  }
  return lines.join('\n').trim()
}

export function localZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone
}

export { isoDate }
