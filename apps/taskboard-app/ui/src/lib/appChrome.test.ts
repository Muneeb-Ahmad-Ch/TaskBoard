import { describe, expect, it } from 'vitest'
import { __test } from './appChrome'

const { isBrowserShortcut } = __test
const key = (k: string, mods: Partial<KeyboardEventInit> = {}) =>
  ({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods }) as KeyboardEvent

describe('browser shortcuts are blocked in release builds', () => {
  it('blocks reload, print, save, view source, back and forward', () => {
    for (const e of [
      key('F5'),
      key('r', { ctrlKey: true }),
      key('R', { ctrlKey: true, shiftKey: true }),
      key('p', { ctrlKey: true }),
      key('s', { ctrlKey: true }),
      key('u', { ctrlKey: true }),
      key('ArrowLeft', { altKey: true }),
      key('ArrowRight', { altKey: true }),
      key('BrowserBack'),
    ]) {
      expect(isBrowserShortcut(e), `${e.key} ctrl=${e.ctrlKey} alt=${e.altKey}`).toBe(true)
    }
  })
  it('leaves the app’s own shortcuts alone', () => {
    for (const e of [
      key('k', { ctrlKey: true }),
      key('z', { ctrlKey: true }),
      key('f', { ctrlKey: true }),
      key('n'),
      key('ArrowUp', { altKey: true }),
      key('ArrowDown', { altKey: true }),
      key('ArrowLeft'),
      key('?'),
    ]) {
      expect(isBrowserShortcut(e), `${e.key}`).toBe(false)
    }
  })
})
