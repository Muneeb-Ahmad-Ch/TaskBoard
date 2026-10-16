// The only place the UI talks to Rust. Types come from generated bindings.
import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Client } from './bindings/Client'
import type { ClientPatch } from './bindings/ClientPatch'
import type { DbInfo } from './bindings/DbInfo'
import type { ErrorInfo } from './bindings/ErrorInfo'
import type { HistoryState } from './bindings/HistoryState'
import type { Id } from './bindings/Id'
import type { MoveTask } from './bindings/MoveTask'
import type { NewClient } from './bindings/NewClient'
import type { NewProject } from './bindings/NewProject'
import type { NewTask } from './bindings/NewTask'
import type { Project } from './bindings/Project'
import type { ProjectPatch } from './bindings/ProjectPatch'
import type { ProjectSummary } from './bindings/ProjectSummary'
import type { QuickAddPreview } from './bindings/QuickAddPreview'
import type { SidebarCounts } from './bindings/SidebarCounts'
import type { Status } from './bindings/Status'
import type { Task } from './bindings/Task'
import type { TaskPage } from './bindings/TaskPage'
import type { TaskPatch } from './bindings/TaskPatch'
import type { TaskQuery } from './bindings/TaskQuery'
import type { TaskSummary } from './bindings/TaskSummary'

export const inTauri = isTauri()

export const ipc = {
  // reads
  dbInfo: () => invoke<DbInfo>('db_info'),
  historyState: () => invoke<HistoryState>('history_state'),
  listClients: (includeArchived = false) => invoke<Client[]>('list_clients', { includeArchived }),
  listProjects: (includeArchived = false) =>
    invoke<ProjectSummary[]>('list_projects', { includeArchived }),
  listStatuses: (projectId: Id) => invoke<Status[]>('list_statuses', { projectId }),
  listTasks: (query: TaskQuery) => invoke<TaskPage>('list_tasks', { query }),
  summarizeTasks: (query: TaskQuery) => invoke<TaskSummary>('summarize_tasks', { query }),
  getTask: (id: Id) => invoke<Task>('get_task', { id }),
  sidebarCounts: () => invoke<SidebarCounts>('sidebar_counts'),
  previewQuickAdd: (line: string) => invoke<QuickAddPreview>('preview_quick_add', { line }),

  // writes
  undo: () => invoke<string>('undo'),
  redo: () => invoke<string>('redo'),
  quickAdd: (line: string) => invoke<Task>('quick_add', { line }),
  createTask: (input: NewTask) => invoke<Task>('create_task', { input }),
  updateTask: (id: Id, patch: TaskPatch) => invoke<Task>('update_task', { id, patch }),
  setTaskDone: (id: Id, done: boolean) => invoke<Task>('set_task_done', { id, done }),
  moveTask: (id: Id, to: MoveTask) => invoke<Task>('move_task', { id, to }),
  deleteTask: (id: Id) => invoke<void>('delete_task', { id }),
  duplicateTask: (id: Id) => invoke<Task>('duplicate_task', { id }),
  bulkUpdateTasks: (ids: Id[], patch: TaskPatch) =>
    invoke<number>('bulk_update_tasks', { ids, patch }),
  bulkSetDone: (ids: Id[], done: boolean) => invoke<number>('bulk_set_done', { ids, done }),
  bulkDelete: (ids: Id[]) => invoke<number>('bulk_delete', { ids }),
  createClient: (input: NewClient) => invoke<Client>('create_client', { input }),
  updateClient: (id: Id, patch: ClientPatch) => invoke<Client>('update_client', { id, patch }),
  setClientArchived: (id: Id, archived: boolean) =>
    invoke<Client>('set_client_archived', { id, archived }),
  deleteClient: (id: Id) => invoke<void>('delete_client', { id }),
  createProject: (input: NewProject) => invoke<Project>('create_project', { input }),
  updateProject: (id: Id, patch: ProjectPatch) => invoke<Project>('update_project', { id, patch }),
  setProjectArchived: (id: Id, archived: boolean) =>
    invoke<Project>('set_project_archived', { id, archived }),
  deleteProject: (id: Id) => invoke<void>('delete_project', { id }),

  // window
  hasMica: () => invoke<boolean>('has_mica'),
  showMainWindow: () => invoke<void>('show_main_window'),
}

export function onEvent(name: 'data-changed' | 'quick-add', handler: () => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {})
  return listen(name, handler)
}

export function isErrorInfo(value: unknown): value is ErrorInfo {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value
}

/** Human-readable message for any thrown value. */
export function errorMessage(err: unknown): string {
  if (isErrorInfo(err)) {
    // Core messages start lowercase ("invalid input: …"); show the useful part.
    const msg = err.message.replace(/^invalid input: /, '')
    return msg.charAt(0).toUpperCase() + msg.slice(1)
  }
  return String(err)
}
