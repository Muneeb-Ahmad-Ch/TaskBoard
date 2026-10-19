import { describe, expect, it } from 'vitest'
import { searchSettings } from './settingsIndex'

const top = (q: string) => searchSettings(q)[0]

describe('settings search', () => {
  it('finds a setting by its name', () => {
    expect(top('zoom')).toMatchObject({ page: 'appearance', label: 'Zoom' })
    expect(top('tray')).toMatchObject({ page: 'general', label: 'Keep running in the tray' })
    expect(top('backup')).toMatchObject({ page: 'data' })
  })
  it('finds it by the words people use for it', () => {
    expect(top('waiting')).toMatchObject({ page: 'columns' })
    expect(top('status')).toMatchObject({ page: 'columns' })
    expect(top('dark mode')).toMatchObject({ page: 'appearance', label: 'Theme' })
    expect(top('claude')).toMatchObject({ page: 'ai' })
    expect(top('notifications')).toMatchObject({ page: 'reminders' })
    expect(top('upgrade')).toMatchObject({ page: 'updates' })
  })
  it('ranks names above extra words, and needs every word', () => {
    const r = searchSettings('column')
    expect(r[0].label.toLowerCase()).toContain('column')
    expect(searchSettings('zoom backup')).toEqual([])
  })
  it('ignores case, accents in spacing and empty queries', () => {
    expect(top('  ZOOM ')).toMatchObject({ label: 'Zoom' })
    expect(searchSettings('   ')).toEqual([])
  })
  it('points every result at a place on its page', () => {
    for (const r of searchSettings('a')) expect(r.anchor).toMatch(/^[a-z-]+$/)
  })
})
