import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ setZoom: async () => {} }) }))
vi.mock('./ipc', () => ({ inTauri: false }))

const { effectiveLevel, maxLevelFor, stepLevel } = await import('./zoom.svelte')

describe('zoom levels', () => {
  it('steps through the levels and stops at the ends', () => {
    expect(stepLevel(1, 1)).toBe(1.1)
    expect(stepLevel(1.1, 1)).toBe(1.25)
    expect(stepLevel(1.5, 1)).toBe(1.5)
    expect(stepLevel(1, -1)).toBe(0.9)
    expect(stepLevel(0.8, -1)).toBe(0.8)
    expect(stepLevel(1.07, 1)).toBe(1.1)
  })
  it('caps zoom so the page stays at least 720 × 480', () => {
    expect(maxLevelFor(1440, 900)).toBe(1.5)
    expect(maxLevelFor(1280, 720)).toBe(1.5)
    expect(maxLevelFor(1000, 700)).toBe(1.25)
    expect(maxLevelFor(800, 600)).toBe(1.1)
    expect(maxLevelFor(720, 480)).toBe(1)
    expect(effectiveLevel(1.5, 1000, 700)).toBe(1.25)
    expect(effectiveLevel(0.9, 1000, 700)).toBe(0.9)
  })
  it('never goes below the smallest level, even in a tiny window', () => {
    expect(maxLevelFor(300, 200)).toBe(0.8)
  })
})
