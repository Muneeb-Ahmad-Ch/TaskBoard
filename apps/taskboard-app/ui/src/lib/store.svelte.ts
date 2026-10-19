// App-wide UI state. SQLite is the source of truth; this holds the current
// scope/view and small cached lists that are re-fetched on `data-changed`.
import type { Category } from './bindings/Category'
import type { Client } from './bindings/Client'
import type { DisplaySettings } from './bindings/DisplaySettings'
import type { HistoryState } from './bindings/HistoryState'
import type { Id } from './bindings/Id'
import type { ProjectSummary } from './bindings/ProjectSummary'
import type { SavedView } from './bindings/SavedView'
import type { Section } from './bindings/Section'
import type { SidebarCounts } from './bindings/SidebarCounts'
import type { Status } from './bindings/Status'
import type { TaskQuery } from './bindings/TaskQuery'
import type { TaskSort } from './bindings/TaskSort'
import { isoDate, plural, todayIso } from './format'
import { errorMessage, inTauri, ipc, onEvent, onIntegrityFailed } from './ipc'
import { adoptLegacySectionPrefs, clearLegacyPins, legacyPins, visibleTabs } from './layout.svelte'
import { pinsToMigrate } from './pins'
import { toast } from './toast.svelte'

export type Scope =
  | { kind: 'inbox' | 'today' | 'upcoming' | 'all' }
  | { kind: 'section' | 'client' | 'project'; id: Id }

export type View = 'table' | 'board' | 'calendar' | 'dashboard' | 'meetings'

/** Pages that are not task lists (they ignore the sidebar scope). */
export function isPage(view: View = app.view): view is 'dashboard' | 'meetings' {
  return view === 'dashboard' || view === 'meetings'
}
export type Lanes = 'none' | 'client' | 'project' | 'priority'

const OPEN: Category[] = ['todo', 'active']

const PLACE_KEY = 'taskboard.lastPlace'
const SCOPE_KINDS = ['inbox', 'today', 'upcoming', 'all', 'section', 'client', 'project']

/** Where the app was last time (per device); unknown ids fall back once data loads. */
function lastPlace(): { scope: Scope; view: View } {
  try {
    const p = JSON.parse(localStorage.getItem(PLACE_KEY) ?? 'null') as { scope?: Scope; view?: View } | null
    if (p?.scope && SCOPE_KINDS.includes(p.scope.kind) && p.view) return { scope: p.scope, view: p.view }
  } catch {
    // Storage unavailable or old format: start on Today.
  }
  return { scope: { kind: 'today' }, view: 'table' }
}

/** Remembers the current list and view for the next start. */
export function savePlace() {
  try {
    localStorage.setItem(PLACE_KEY, JSON.stringify({ scope: app.scope, view: app.view }))
  } catch {
    // per-device convenience only
  }
}

const place = lastPlace()

export const app = $state({
  scope: place.scope,
  view: place.view,
  clients: [] as Client[],
  projects: [] as ProjectSummary[],
  /** Every live status, for building board columns. */
  statuses: [] as Status[],
  lanes: 'none' as Lanes,
  /** Show subtasks as their own cards on the board (parents show their progress). */
  boardSubtasks: false,
  counts: null as SidebarCounts | null,
  history: { undo: null, redo: null } as HistoryState,
  search: '',
  showDone: false,
  sort: 'due' as TaskSort,
  descending: false,
  /** Task shown in the detail panel. */
  openTaskId: null as Id | null,
  /** Bumped on every data change; views re-query when it changes. */
  dataVersion: 0,
  paletteOpen: false,
  /** Client / project whose settings dialog is open. */
  settingsOpen: false,
  /** Set when the startup integrity check found damage. */
  integrityError: null as string | null,
  clientDialog: null as Id | null,
  projectDialog: null as Id | null,
  /** Section dialog: create (null id) or edit; and the section being deleted. */
  sectionDialog: null as { id: Id | null } | null,
  deleteSection: null as Id | null,
  /** Bumped to ask the Quick Add box to focus itself. */
  quickAddFocus: 0,
  shortcutsOpen: false,
  /** Settings section to scroll to when Settings opens (e.g. 'layout'). */
  settingsSection: null as string | null,
  /** Navigation drawer, used instead of the sidebar on narrow windows. */
  sidebarOpen: false,
  views: [] as SavedView[],
  /** Sidebar sections in order (data; visibility is a layout preference). */
  sections: [] as Section[],
  /** Pinned project ids, in order (stored in the database). */
  pinned: [] as Id[],
  /** How lists look (Settings → Views & table); assistants can change it too. */
  display: { hide_repeated_context: true } as DisplaySettings,
  /** The saved view last opened; cleared when the scope changes. */
  activeViewId: null as Id | null,
})

/** What a saved view remembers (stored as JSON in `saved_view.spec_json`). */
export type ViewSpec = {
  scope: Scope
  view: Exclude<View, 'dashboard' | 'meetings'>
  search: string
  showDone: boolean
  sort: TaskSort
  descending: boolean
  lanes: Lanes
}

export function currentSpec(): ViewSpec {
  return {
    scope: app.scope,
    view: isPage(app.view) ? (visibleTabs()[0] ?? 'table') : app.view,
    search: app.search,
    showDone: app.showDone,
    sort: app.sort,
    descending: app.descending,
    lanes: app.lanes,
  }
}

function parseSpec(json: string): Partial<ViewSpec> {
  try {
    const value: unknown = JSON.parse(json)
    return value && typeof value === 'object' ? (value as Partial<ViewSpec>) : {}
  } catch {
    return {}
  }
}

export function applySavedView(v: SavedView) {
  const spec = parseSpec(v.spec_json)
  // Views saved before sections existed may point at the old "personal" list.
  const known = ['inbox', 'today', 'upcoming', 'all', 'section', 'client', 'project']
  setScope(spec.scope && known.includes(spec.scope.kind) ? spec.scope : { kind: 'all' })
  app.view = spec.view ?? 'table'
  app.search = spec.search ?? ''
  app.showDone = spec.showDone ?? false
  app.sort = spec.sort ?? 'due'
  app.descending = spec.descending ?? false
  app.lanes = spec.lanes ?? 'none'
  app.activeViewId = v.id
}

/** JSON with object keys sorted (core stores specs with sorted keys). */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, v: unknown) =>
    v && typeof v === 'object' && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : v,
  )
}

/** True when the open saved view's filters were changed since it was opened. */
export function activeViewChanged(): boolean {
  const v = app.views.find((x) => x.id === app.activeViewId)
  return !!v && canonical(parseSpec(v.spec_json)) !== canonical(currentSpec())
}

export async function saveCurrentView(name: string) {
  const v = await act(() => ipc.createSavedView(name, JSON.stringify(currentSpec())), `Saved view “${name.trim()}”`)
  if (v) app.activeViewId = v.id
}

export async function updateActiveView() {
  const id = app.activeViewId
  if (!id) return
  await act(() => ipc.updateSavedView(id, { spec_json: JSON.stringify(currentSpec()) }), 'View updated')
}

export function inboxId(): Id | null {
  return app.projects.find((p) => p.is_inbox)?.id ?? null
}

export function projectsOf(clientId: Id | null): ProjectSummary[] {
  return app.projects.filter((p) => !p.is_inbox && p.client_id === clientId)
}

/** What one group in a section is called: its item name, or "group". */
export function itemName(section: Section | undefined): string {
  return section?.item_name.trim() || 'group'
}

export function sectionById(id: Id | null | undefined): Section | undefined {
  return id ? app.sections.find((s) => s.id === id) : undefined
}

/** Groups (clients) in a section, in order. */
export function groupsIn(sectionId: Id) {
  return app.clients.filter((c) => c.section_id === sectionId)
}

/** Projects that sit directly in a section (no group). */
export function looseProjectsIn(sectionId: Id): ProjectSummary[] {
  return app.projects.filter((p) => !p.is_inbox && !p.client_id && p.section_id === sectionId)
}

export function scopeTitle(scope: Scope = app.scope): string {
  switch (scope.kind) {
    case 'inbox':
      return 'Inbox'
    case 'today':
      return 'Today'
    case 'upcoming':
      return 'Upcoming'
    case 'all':
      return 'All work'
    case 'section':
      return sectionById(scope.id)?.name ?? 'Section'
    case 'client':
      return app.clients.find((c) => c.id === scope.id)?.name ?? 'Client'
    case 'project':
      return app.projects.find((p) => p.id === scope.id)?.name ?? 'Project'
  }
}

/** The project new tasks in this scope belong to, if the scope implies one. */
export function scopeProjectId(): Id | null {
  return app.scope.kind === 'project' ? app.scope.id : null
}

export function scopeQuery(): TaskQuery {
  const q: TaskQuery = {
    sort: app.sort,
    descending: app.descending,
    categories: app.showDone ? [] : OPEN,
  }
  const s = app.search.trim()
  if (s) q.search = s
  // Lists show top-level tasks; subtasks live inside their parent. Date lists
  // and searches include subtasks (a dated subtask must surface), marked
  // with their parent in the table.
  const dated = app.scope.kind === 'today' || app.scope.kind === 'upcoming'
  if (!s && !dated) q.top_level = true
  const today = todayIso()
  switch (app.scope.kind) {
    case 'inbox': {
      const id = inboxId()
      if (id) q.project_id = id
      break
    }
    case 'today':
      q.due_to = today
      q.categories = OPEN
      break
    case 'upcoming': {
      const from = new Date()
      from.setDate(from.getDate() + 1)
      const to = new Date()
      to.setDate(to.getDate() + 7)
      q.due_from = isoDate(from)
      q.due_to = isoDate(to)
      q.categories = OPEN
      break
    }
    case 'section':
      q.section_id = app.scope.id
      break
    case 'client':
      q.client_id = app.scope.id
      break
    case 'project':
      q.project_id = app.scope.id
      break
  }
  return q
}

/** Projects whose tasks the current scope can show. */
export function scopeProjectIds(): Id[] {
  const s = app.scope
  const live = app.projects
  switch (s.kind) {
    case 'project':
      return [s.id]
    case 'client':
      return live.filter((p) => p.client_id === s.id).map((p) => p.id)
    case 'section':
      return live.filter((p) => !p.is_inbox && p.section_id === s.id).map((p) => p.id)
    case 'inbox':
      return live.filter((p) => p.is_inbox).map((p) => p.id)
    default:
      return live.map((p) => p.id)
  }
}

export function setScope(scope: Scope) {
  app.scope = scope
  app.openTaskId = null
  app.activeViewId = null
  // Picking a list leaves the dashboard.
  if (isPage(app.view)) app.view = visibleTabs()[0] ?? 'table'
}

/** Pins made before they moved into the database (once, then gone). */
async function migrateLegacyPins(current: Id[], projects: ProjectSummary[]) {
  const next = pinsToMigrate(legacyPins(), current, new Set(projects.map((p) => p.id)))
  if (!next) return
  try {
    app.pinned = await ipc.setPinnedProjects(next)
    clearLegacyPins()
  } catch {
    // Kept on this device; the next start tries again.
  }
}

/** Pins a project (last, or before another pinned one); pinning a pinned one moves it. */
export async function pinProject(id: Id, before: Id | null = null) {
  const next = await act(() => ipc.pinProject(id, before))
  if (next) app.pinned = next
}

export async function unpinProject(id: Id) {
  const next = await act(() => ipc.unpinProject(id))
  if (next) app.pinned = next
}

export async function refreshMeta() {
  if (!inTauri) return
  try {
    const [clients, projects, counts, history, statuses, views, sections, pinned, display] = await Promise.all([
      ipc.listClients(),
      ipc.listProjects(),
      ipc.sidebarCounts(),
      ipc.historyState(),
      ipc.allStatuses(),
      ipc.savedViews(),
      ipc.listSections(),
      ipc.pinnedProjects(),
      ipc.displaySettings().catch(() => app.display),
    ])
    app.pinned = pinned
    app.display = display
    void migrateLegacyPins(pinned, projects)
    app.sections = sections
    adoptLegacySectionPrefs(sections)
    app.views = views
    if (app.activeViewId && !views.some((v) => v.id === app.activeViewId)) app.activeViewId = null
    app.clients = clients
    app.projects = projects
    app.statuses = statuses
    app.counts = counts
    app.history = history
    // The scoped client/project may have been deleted or archived.
    const s = app.scope
    if (s.kind === 'client' && !clients.some((c) => c.id === s.id)) setScope({ kind: 'all' })
    if (s.kind === 'project' && !projects.some((p) => p.id === s.id)) setScope({ kind: 'all' })
    if (s.kind === 'section' && !sections.some((x) => x.id === s.id)) setScope({ kind: 'all' })
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}

/** Runs a mutation, reporting failures as a toast. Returns undefined on error. */
export async function act<T>(run: () => Promise<T>, done?: string): Promise<T | undefined> {
  try {
    const result = await run()
    if (done) toast(done, { action: { label: 'Undo', run: () => void undo() } })
    return result
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
    return undefined
  }
}

/** Adds the default board columns (Waiting, …) each project is missing. */
export async function addMissingColumns() {
  const n = await act(() => ipc.addMissingColumns())
  if (n === undefined) return
  if (n) toast(`Added the missing columns to ${plural(n, 'project')}`, { action: { label: 'Undo', run: () => void undo() } })
  else toast('Every project already has the default columns')
}

/** Changes how lists look (saved with the data, so assistants see it too). */
export async function setDisplay(patch: Partial<DisplaySettings>) {
  const next = { ...app.display, ...patch }
  const before = app.display
  app.display = next
  try {
    app.display = await ipc.setDisplaySettings(next)
  } catch (err) {
    app.display = before
    toast(errorMessage(err), { kind: 'error' })
  }
}

/** Archives a list's completed tasks (top-level, done or cancelled) as one undo step. */
export async function archiveCompleted(query: TaskQuery = scopeQuery()) {
  const n = await act(() => ipc.archiveCompleted(query))
  if (n === undefined) return
  if (n) toast(`Archived ${plural(n, 'completed task')}`, { action: { label: 'Undo', run: () => void undo() } })
  else toast('No completed tasks to archive here')
}

export async function undo() {
  try {
    const label = await ipc.undo()
    toast(`Undid: ${label}`, { action: { label: 'Redo', run: () => void redo() } })
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}

export async function redo() {
  try {
    const label = await ipc.redo()
    toast(`Redid: ${label}`, { action: { label: 'Undo', run: () => void undo() } })
  } catch (err) {
    toast(errorMessage(err), { kind: 'error' })
  }
}

export async function initStore() {
  await refreshMeta()
  await onEvent('data-changed', () => {
    app.dataVersion++
    void refreshMeta()
  })
  await onEvent('quick-add', () => {
    app.quickAddFocus++
  })
  await onIntegrityFailed((message) => {
    app.integrityError = message
  })
}
