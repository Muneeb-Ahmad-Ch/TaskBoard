// What the sidebar and the view switcher show, and in which order. A per-device
// UI preference (like the theme); hidden items stay reachable from Ctrl+K.

export type NavId = 'today' | 'upcoming' | 'inbox' | 'all' | 'meetings' | 'dashboard'
export type TabId = 'table' | 'board' | 'calendar'
export type SectionId = 'personal' | 'savedViews'

export const NAV_LABELS: Record<NavId, string> = {
  today: 'Today',
  upcoming: 'Upcoming',
  inbox: 'Inbox',
  all: 'All work',
  meetings: 'Meetings',
  dashboard: 'Dashboard',
}
export const TAB_LABELS: Record<TabId, string> = { table: 'Table', board: 'Board', calendar: 'Calendar' }
export const SECTION_LABELS: Record<SectionId, string> = { personal: 'Personal projects', savedViews: 'Saved views' }

const NAV_ORDER: NavId[] = ['today', 'upcoming', 'inbox', 'all', 'meetings', 'dashboard']
const TABS: TabId[] = ['table', 'board', 'calendar']
const KEY = 'taskboard.layout'

type Stored = {
  nav: { id: NavId; visible: boolean }[]
  sections: Record<SectionId, boolean>
  tabs: Record<TabId, boolean>
}

function defaults(): Stored {
  return {
    nav: NAV_ORDER.map((id) => ({ id, visible: true })),
    sections: { personal: true, savedViews: true },
    tabs: { table: true, board: true, calendar: true },
  }
}

/** Reads the saved layout, keeping its order and adding items new in this version. */
function load(): Stored {
  const base = defaults()
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null') as Partial<Stored> | null
    if (!saved) return base
    const known = (saved.nav ?? []).filter((n) => NAV_ORDER.includes(n.id))
    const seen = new Set(known.map((n) => n.id))
    const nav = [...known, ...NAV_ORDER.filter((id) => !seen.has(id)).map((id) => ({ id, visible: true }))]
    const tabs = { ...base.tabs, ...saved.tabs }
    if (!TABS.some((t) => tabs[t])) tabs.table = true
    return { nav, sections: { ...base.sections, ...saved.sections }, tabs }
  } catch {
    return base
  }
}

export const layout = $state<Stored>(load())

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(layout))
  } catch {
    // Storage unavailable: the change still applies for this session.
  }
}

export function setNavVisible(id: NavId, visible: boolean) {
  const item = layout.nav.find((n) => n.id === id)
  if (item) item.visible = visible
  save()
}

export function moveNav(id: NavId, dir: -1 | 1) {
  const i = layout.nav.findIndex((n) => n.id === id)
  const j = i + dir
  if (i < 0 || j < 0 || j >= layout.nav.length) return
  const [item] = layout.nav.splice(i, 1)
  layout.nav.splice(j, 0, item)
  save()
}

export function setSection(id: SectionId, visible: boolean) {
  layout.sections[id] = visible
  save()
}

/** Shows or hides a view tab; the last visible one cannot be hidden. */
export function setTab(id: TabId, visible: boolean): boolean {
  if (!visible && TABS.filter((t) => layout.tabs[t]).length <= 1) return false
  layout.tabs[id] = visible
  save()
  return true
}

export function visibleTabs(): TabId[] {
  return TABS.filter((t) => layout.tabs[t])
}

export function resetLayout() {
  Object.assign(layout, defaults())
  save()
}
