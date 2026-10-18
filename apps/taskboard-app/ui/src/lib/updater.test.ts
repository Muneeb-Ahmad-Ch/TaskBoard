import { describe, expect, it, vi } from 'vitest'

vi.mock('./ipc', () => ({ inTauri: false, ipc: {}, errorMessage: String }))
vi.mock('./store.svelte', () => ({ app: {} }))
vi.mock('./toast.svelte', () => ({ toast: () => {} }))

const { CHECK_EVERY, isDue, megabytes } = await import('./updater.svelte')

describe('automatic update checks', () => {
  const now = 1_900_000_000_000
  it('are due when the last check is old', () => {
    expect(isDue(now, { auto: true, checkedAt: now - CHECK_EVERY, found: '' })).toBe(true)
    expect(isDue(now, { auto: true, checkedAt: 0, found: '' })).toBe(true)
    expect(isDue(now, { auto: true, checkedAt: now - 60_000, found: '' })).toBe(false)
  })
  it('run again at once when the last check found a version', () => {
    expect(isDue(now, { auto: true, checkedAt: now - 60_000, found: '0.6.0' })).toBe(true)
  })
  it('never run when turned off', () => {
    expect(isDue(now, { auto: false, checkedAt: 0, found: '0.6.0' })).toBe(false)
  })
})

describe('download sizes', () => {
  it('read in megabytes', () => {
    expect(megabytes(5.5 * 1_048_576)).toBe('5.5 MB')
    expect(megabytes(12 * 1_048_576)).toBe('12 MB')
  })
})
