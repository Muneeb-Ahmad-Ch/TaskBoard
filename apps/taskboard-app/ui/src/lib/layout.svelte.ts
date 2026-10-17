// What the sidebar and the view switcher show, and in which order. A per-device
// UI preference (like the theme); hidden items stay reachable from Ctrl+K.

export type NavId = 'today' | 'upcoming' | 'inbox' | 'all' | 'meetings' | 'dashboard'
export type TabId = 'table' | 'board' | 'calendar'
/** Fixed sidebar sections (user sections are hidden by id, see `hiddenSections`). */
export type SectionId = 'savedViews'

export const NAV_LABELS: Record<NavId, string> = {
  today: 'Today',
  upcoming: 'Upcoming',
  inbox: 'Inbox',
  all: 'All work',
  meetings: 'Meetings',
  dashboard: 'Dashboard',
}
export const TAB_LABELS: Record<TabId, string> = { table: 'Table', board: 'Board', calendar: 'Calendar' }
export const SECTION_LABELS: Record<SectionId, string> = { savedViews: 'Saved views' }

/** Numbers the sidebar can show next to items; each can be turned off. */
export type CountId = 'views' | 'overdue' | 'sections' | 'projects'
export const COUNT_LABELS: Record<CountId, { name: string; hint: string }> = {
  views: { name: 'View counts', hint: 'Tasks in Today, Upcoming and Inbox; meetings left today' },
  overdue: { name: 'Overdue badge', hint: 'Red count of overdue tasks on Today' },
  sections: { name: 'Section counts', hint: 'Groups and projects next to each section name' },
  projects: { name: 'Project counts', hint: 'Open tasks next to each project' },
}

const NAV_ORDER: NavId[] = ['today', 'upcoming', 'inbox', 'all', 'meetings', 'dashboard']
const TABS: TabId[] = ['table', 'board', 'calendar']
const KEY = 'taskboard.layout'

type Stored = {
  nav: { id: NavId; visible: boolean }[]
  sections: Record<SectionId, boolean>
  /** User sections hidden from the sidebar (ids). */
  hiddenSections: string[]
  /** Section names hidden before sections were user-defined ("Clients",
   *  "Personal"); turned into ids once the sections are known. */
  legacyHidden: string[]
  tabs: Record<TabId, boolean>
  counts: Record<CountId, boolean>
  /** The colored dot before each section name. */
  sectionDots: boolean
  /** Where the Table view's add-a-task box sits. */
  quickAdd: 'top' | 'bottom'
}

function defaults(): Stored {
  return {
    nav: NAV_ORDER.map((id) => ({ id, visible: true })),
    sections: { savedViews: true },
    hiddenSections: [],
    legacyHidden: [],
    tabs: { table: true, board: true, calendar: true },
    counts: { views: true, overdue: true, sections: true, projects: true },
    sectionDots: true,
    quickAdd: 'bottom',
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
    const old = (saved.sections ?? {}) as Record<string, boolean>
    const legacyHidden = [
      ...(saved.legacyHidden ?? []),
      ...(old.clients === false ? ['Clients'] : []),
      ...(old.personal === false ? ['Personal'] : []),
    ]
    return {
      nav,
      sections: { savedViews: old.savedViews ?? true },
      hiddenSections: saved.hiddenSections ?? [],
      legacyHidden,
      tabs,
      counts: { ...base.counts, ...saved.counts },
      sectionDots: saved.sectionDots ?? true,
      quickAdd: saved.quickAdd === 'top' ? 'top' : 'bottom',
    }
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

export function setCount(id: CountId, shown: boolean) {
  layout.counts[id] = shown
  save()
}

export function setQuickAddPlacement(where: 'top' | 'bottom') {
  layout.quickAdd = where
  save()
}

export function setSectionDots(shown: boolean) {
  layout.sectionDots = shown
  save()
}

export function setSectionHidden(id: string, hidden: boolean) {
  layout.hiddenSections = hidden ? [...new Set([...layout.hiddenSections, id])] : layout.hiddenSections.filter((h) => h !== id)
  save()
}

/** Hidden "Clients" / "Personal" from before sections existed carry over. */
export function adoptLegacySectionPrefs(sections: { id: string; name: string }[]) {
  if (!layout.legacyHidden.length || !sections.length) return
  for (const name of layout.legacyHidden) {
    const match = sections.find((s) => s.name.toLowerCase() === name.toLowerCase())
    if (match && !layout.hiddenSections.includes(match.id)) layout.hiddenSections.push(match.id)
  }
  layout.legacyHidden = []
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
