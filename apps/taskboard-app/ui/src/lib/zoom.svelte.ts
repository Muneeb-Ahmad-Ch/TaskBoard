// Zoom: Ctrl + / Ctrl − / Ctrl 0, Ctrl + mouse wheel, and Settings →
// Appearance. Uses the WebView's own zoom (everything scales together, like
// a browser), remembered per device. The level is capped so the page never
// gets smaller than the layout's minimum (720 × 480 CSS pixels): zooming in
// a small window stops where the layout would break.
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { inTauri } from './ipc'

export const LEVELS = [0.8, 0.9, 1, 1.1, 1.25, 1.5] as const
const KEY = 'taskboard.zoom'
export const MIN_VIEW = { width: 720, height: 480 }

/** The highest level that keeps the page at least `MIN_VIEW` big. */
export function maxLevelFor(baseWidth: number, baseHeight: number): number {
  const fit = Math.min(baseWidth / MIN_VIEW.width, baseHeight / MIN_VIEW.height)
  const allowed = LEVELS.filter((l) => l <= fit + 1e-9)
  return allowed.length ? allowed[allowed.length - 1] : LEVELS[0]
}

/** The level actually used: the chosen one, capped for the window size. */
export function effectiveLevel(chosen: number, baseWidth: number, baseHeight: number): number {
  return Math.min(chosen, maxLevelFor(baseWidth, baseHeight))
}

/** The next level up or down from `level` (stays at the ends). */
export function stepLevel(level: number, dir: 1 | -1): number {
  const i = LEVELS.findIndex((l) => Math.abs(l - level) < 1e-9)
  const at = i < 0 ? LEVELS.indexOf(1) : i
  return LEVELS[Math.max(0, Math.min(LEVELS.length - 1, at + dir))]
}

function load(): number {
  try {
    const v = Number(localStorage.getItem(KEY))
    return LEVELS.some((l) => Math.abs(l - v) < 1e-9) ? v : 1
  } catch {
    return 1
  }
}

export const zoom = $state({
  /** What the user picked. */
  chosen: load(),
  /** What is applied right now (lower than `chosen` in a small window). */
  applied: 1,
})

/** Window size in CSS pixels at 100%, whatever zoom is applied now. */
function baseSize(): { w: number; h: number } {
  return { w: window.innerWidth * zoom.applied, h: window.innerHeight * zoom.applied }
}

let pending: Promise<void> = Promise.resolve()

export function applyZoom(): Promise<void> {
  const { w, h } = baseSize()
  const level = effectiveLevel(zoom.chosen, w, h)
  if (Math.abs(level - zoom.applied) < 1e-9) return pending
  zoom.applied = level
  if (!inTauri) {
    document.documentElement.style.zoom = String(level)
    return pending
  }
  pending = getCurrentWebview()
    .setZoom(level)
    .catch(() => {
      // Zoom unavailable: stay at 100% rather than half-applied.
      zoom.applied = 1
    })
  return pending
}

export function setZoomLevel(level: number) {
  zoom.chosen = LEVELS.reduce((best, l) => (Math.abs(l - level) < Math.abs(best - level) ? l : best), 1)
  try {
    localStorage.setItem(KEY, String(zoom.chosen))
  } catch {
    // per-device convenience only
  }
  void applyZoom()
}

export const zoomIn = () => setZoomLevel(stepLevel(zoom.chosen, 1))
export const zoomOut = () => setZoomLevel(stepLevel(zoom.chosen, -1))
export const resetZoom = () => setZoomLevel(1)

/** Keys and Ctrl + wheel; re-checks the cap when the window is resized.
 *  Resolves once the saved level is applied (before the window shows). */
export function installZoom(): Promise<void> {
  const first = applyZoom()
  window.addEventListener(
    'keydown',
    (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return
      const k = e.key
      if (k === '=' || k === '+' || k === 'Add') zoomIn()
      else if (k === '-' || k === '_' || k === 'Subtract') zoomOut()
      else if (k === '0' && !e.shiftKey) resetZoom()
      else return
      e.preventDefault()
      e.stopPropagation()
    },
    true,
  )
  let wheelAt = 0
  window.addEventListener(
    'wheel',
    (e) => {
      if (!(e.ctrlKey || e.metaKey)) return
      e.preventDefault()
      // One step per gesture burst, not one per wheel tick.
      const now = Date.now()
      if (now - wheelAt < 180) return
      wheelAt = now
      if (e.deltaY < 0) zoomIn()
      else if (e.deltaY > 0) zoomOut()
    },
    { passive: false },
  )
  let timer: ReturnType<typeof setTimeout> | undefined
  window.addEventListener('resize', () => {
    clearTimeout(timer)
    timer = setTimeout(() => void applyZoom(), 120)
  })
  return first
}
