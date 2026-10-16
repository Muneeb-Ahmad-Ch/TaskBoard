// App-wide UI state. SQLite is the source of truth; this holds the current
// scope/view and small cached lists that are re-fetched on `data-changed`.
import type { Category } from './bindings/Category'
import type { Client } from './bindings/Client'
import type { HistoryState } from './bindings/HistoryState'
import type { Id } from './bindings/Id'
import type { ProjectSummary } from './bindings/ProjectSummary'
import type { SidebarCounts } from './bindings/SidebarCounts'
import type { Status } from './bindings/Status'
import type { TaskQuery } from './bindings/TaskQuery'
import type { TaskSort } from './bindings/TaskSort'
import { isoDate, todayIso } from './format'
import { errorMessage, inTauri, ipc, onEvent } from './ipc'
import { toast } from './toast.svelte'

export type Scope =
  | { kind: 'inbox' | 'today' | 'upcoming' | 'all' | 'personal' }
  | { kind: 'client' | 'project'; id: Id }

export type View = 'table' | 'board' | 'calendar'
export type Lanes = 'none' | 'client' | 'project' | 'priority'

const OPEN: Category[] = ['todo', 'active']

export const app = $state({
  scope: { kind: 'today' } as Scope,
  view: 'table' as View,
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
  clientDialog: null as Id | null,
  projectDialog: null as Id | null,
  /** Bumped to ask the Quick Add box to focus itself. */
  quickAddFocus: 0,
})

export function inboxId(): Id | null {
  return app.projects.find((p) => p.is_inbox)?.id ?? null
}

export function projectsOf(clientId: Id | null): ProjectSummary[] {
  return app.projects.filter((p) => !p.is_inbox && p.client_id === clientId)
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
    case 'personal':
      return 'Personal'
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
    case 'personal':
      q.personal = true
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
    case 'personal':
      return live.filter((p) => !p.client_id && !p.is_inbox).map((p) => p.id)
    case 'inbox':
      return live.filter((p) => p.is_inbox).map((p) => p.id)
    default:
      return live.map((p) => p.id)
  }
}

export function setScope(scope: Scope) {
  app.scope = scope
  app.openTaskId = null
}

export async function refreshMeta() {
  if (!inTauri) return
  try {
    const [clients, projects, counts, history, statuses] = await Promise.all([
      ipc.listClients(),
      ipc.listProjects(),
      ipc.sidebarCounts(),
      ipc.historyState(),
      ipc.allStatuses(),
    ])
    app.clients = clients
    app.projects = projects
    app.statuses = statuses
    app.counts = counts
    app.history = history
    // The scoped client/project may have been deleted or archived.
    const s = app.scope
    if (s.kind === 'client' && !clients.some((c) => c.id === s.id)) setScope({ kind: 'all' })
    if (s.kind === 'project' && !projects.some((p) => p.id === s.id)) setScope({ kind: 'all' })
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
}
