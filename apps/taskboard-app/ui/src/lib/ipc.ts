// The only place the UI talks to Rust. Types come from generated bindings.
import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { AppSettings } from './bindings/AppSettings'
import type { BackupInfo } from './bindings/BackupInfo'
import type { ImportReport } from './bindings/ImportReport'
import type { Dashboard } from './bindings/Dashboard'
import type { Notification } from './bindings/Notification'
import type { NotifySettings } from './bindings/NotifySettings'
import type { Client } from './bindings/Client'
import type { ClientHours } from './bindings/ClientHours'
import type { DayOverride } from './bindings/DayOverride'
import type { DayPlan } from './bindings/DayPlan'
import type { Event } from './bindings/Event'
import type { EventPatch } from './bindings/EventPatch'
import type { FindSlots } from './bindings/FindSlots'
import type { LoggedTime } from './bindings/LoggedTime'
import type { NewEvent } from './bindings/NewEvent'
import type { NewTimeEntry } from './bindings/NewTimeEntry'
import type { Occurrence } from './bindings/Occurrence'
import type { OccurrencePatch } from './bindings/OccurrencePatch'
import type { SlotResult } from './bindings/SlotResult'
import type { TimeEntryPatch } from './bindings/TimeEntryPatch'
import type { WorkingDay } from './bindings/WorkingDay'
import type { ClientPatch } from './bindings/ClientPatch'
import type { DbInfo } from './bindings/DbInfo'
import type { ErrorInfo } from './bindings/ErrorInfo'
import type { HistoryState } from './bindings/HistoryState'
import type { Id } from './bindings/Id'
import type { MoveTask } from './bindings/MoveTask'
import type { NewClient } from './bindings/NewClient'
import type { NewProject } from './bindings/NewProject'
import type { NewStatus } from './bindings/NewStatus'
import type { NewTask } from './bindings/NewTask'
import type { Project } from './bindings/Project'
import type { ProjectPatch } from './bindings/ProjectPatch'
import type { ProjectSummary } from './bindings/ProjectSummary'
import type { QuickAddPreview } from './bindings/QuickAddPreview'
import type { SidebarCounts } from './bindings/SidebarCounts'
import type { Status } from './bindings/Status'
import type { StatusPatch } from './bindings/StatusPatch'
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

  // board columns
  allStatuses: () => invoke<Status[]>('all_statuses'),
  createStatus: (input: NewStatus) => invoke<Status>('create_status', { input }),
  updateStatus: (id: Id, patch: StatusPatch) => invoke<Status>('update_status', { id, patch }),
  moveStatus: (id: Id, after: Id | null) => invoke<Status>('move_status', { id, after }),
  deleteStatus: (id: Id, moveTo: Id) => invoke<void>('delete_status', { id, moveTo }),

  // calendar
  occurrences: (from: number, to: number) => invoke<Occurrence[]>('occurrences', { from, to }),
  getEvent: (id: Id) => invoke<Event>('get_event', { id }),
  createEvent: (input: NewEvent) => invoke<Event>('create_event', { input }),
  updateEvent: (id: Id, patch: EventPatch) => invoke<Event>('update_event', { id, patch }),
  deleteEvent: (id: Id) => invoke<void>('delete_event', { id }),
  editOccurrence: (id: Id, originalStart: number, patch: OccurrencePatch) =>
    invoke<void>('edit_occurrence', { id, originalStart, patch }),
  splitSeries: (id: Id, originalStart: number, patch: EventPatch) =>
    invoke<Event>('split_series', { id, originalStart, patch }),
  workingHours: () => invoke<WorkingDay[]>('working_hours'),
  setWorkingHours: (week: WorkingDay[]) => invoke<WorkingDay[]>('set_working_hours', { week }),
  dayOverrides: (from: string, to: string) => invoke<DayOverride[]>('day_overrides', { from, to }),
  setDayOverride: (day: DayOverride) => invoke<void>('set_day_override', { day }),
  clearDayOverride: (date: string) => invoke<void>('clear_day_override', { date }),
  runningTimer: () => invoke<LoggedTime | null>('running_timer'),
  logTime: (input: NewTimeEntry) => invoke<Id>('log_time', { input }),
  stopTimer: () => invoke<boolean>('stop_timer'),
  updateTimeEntry: (id: Id, patch: TimeEntryPatch) => invoke<void>('update_time_entry', { id, patch }),
  deleteTimeEntry: (id: Id) => invoke<void>('delete_time_entry', { id }),
  loggedTime: (from: number, to: number) => invoke<LoggedTime[]>('logged_time', { from, to }),
  dayPlans: (from: string, to: string) => invoke<DayPlan[]>('day_plans', { from, to }),
  hoursByClient: (from: number, to: number) => invoke<ClientHours[]>('hours_by_client', { from, to }),
  findSlots: (query: FindSlots) => invoke<SlotResult>('find_slots', { query }),

  // settings, reminders, dashboard
  dashboard: () => invoke<Dashboard>('dashboard'),
  notifySettings: () => invoke<NotifySettings>('notify_settings'),
  setNotifySettings: (settings: NotifySettings) => invoke<NotifySettings>('set_notify_settings', { settings }),
  appSettings: () => invoke<AppSettings>('app_settings'),
  setAppSettings: (settings: AppSettings) => invoke<AppSettings>('set_app_settings', { settings }),
  testNotification: () => invoke<void>('test_notification'),
  startHidden: () => invoke<boolean>('start_hidden'),

  // data
  listBackups: () => invoke<BackupInfo[]>('list_backups'),
  backupNow: () => invoke<BackupInfo>('backup_now'),
  openBackupsFolder: () => invoke<void>('open_backups_folder'),
  restoreBackup: (path: string) => invoke<void>('restore_backup', { path }),
  exportData: (kind: 'json' | 'csv' | 'ics', path: string) => invoke<void>('export_data', { kind, path }),
  importData: (kind: 'json' | 'csv' | 'ics', path: string) => invoke<ImportReport>('import_data', { kind, path }),
  hasSample: () => invoke<boolean>('has_sample'),
  createSample: () => invoke<void>('create_sample'),
  removeSample: () => invoke<void>('remove_sample'),

  // window
  hasMica: () => invoke<boolean>('has_mica'),
  showMainWindow: () => invoke<void>('show_main_window'),
}

export function onEvent(name: 'data-changed' | 'quick-add', handler: () => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {})
  return listen(name, handler)
}

export function onIntegrityFailed(handler: (message: string) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {})
  return listen<string>('integrity-failed', (e) => handler(e.payload))
}

export function onReminders(handler: (list: Notification[]) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {})
  return listen<Notification[]>('reminders', (e) => handler(e.payload))
}

/** Opens http(s) links in the default browser (the webview can't navigate away). */
export async function openLink(url: string) {
  if (!/^https?:\/\//i.test(url)) return
  if (inTauri) await openUrl(url)
  else window.open(url, '_blank', 'noopener')
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
