// Theme + accent state. Applied before first paint (the window is shown only
// afterwards), so there is never a flash of the wrong theme.
import { getCurrentWindow } from '@tauri-apps/api/window'
import { inTauri } from './ipc'

export type ThemePref = 'system' | 'light' | 'dark'

export const ACCENTS = [
  { name: 'Violet', hue: 262 },
  { name: 'Indigo', hue: 280 },
  { name: 'Purple', hue: 305 },
  { name: 'Magenta', hue: 330 },
  { name: 'Rose', hue: 10 },
  { name: 'Orange', hue: 40 },
  { name: 'Amber', hue: 70 },
  { name: 'Lime', hue: 115 },
  { name: 'Green', hue: 150 },
  { name: 'Teal', hue: 190 },
  { name: 'Sky', hue: 220 },
  { name: 'Blue', hue: 245 },
] as const

const THEME_KEY = 'taskboard.theme'
const ACCENT_KEY = 'taskboard.accentHue'

// Per-device UI preference; moves to the settings store in M6.
function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    // Storage unavailable: the choice still applies for this session.
  }
}

function initialPref(): ThemePref {
  const saved = read(THEME_KEY)
  return saved === 'light' || saved === 'dark' ? saved : 'system'
}

function initialHue(): number {
  const saved = Number(read(ACCENT_KEY))
  return Number.isFinite(saved) && saved > 0 ? saved : ACCENTS[0].hue
}

export const theme = $state({ pref: initialPref(), accentHue: initialHue() })

export function applyTheme() {
  const root = document.documentElement
  if (theme.pref === 'system') delete root.dataset.theme
  else root.dataset.theme = theme.pref
  root.style.setProperty('--accent-h', String(theme.accentHue))
  // Keeps the native title bar and Mica tint in sync with the page.
  if (inTauri) void getCurrentWindow().setTheme(theme.pref === 'system' ? null : theme.pref)
}

export function setThemePref(pref: ThemePref) {
  theme.pref = pref
  write(THEME_KEY, pref)
  applyTheme()
}

export function setAccentHue(hue: number) {
  theme.accentHue = hue
  write(ACCENT_KEY, String(hue))
  applyTheme()
}
