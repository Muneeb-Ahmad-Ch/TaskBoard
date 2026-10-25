// The only place the UI talks to Rust. Types come from generated bindings.
import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { AppSettings } from './bindings/AppSettings'
import type { BackupInfo } from './bindings/BackupInfo'
import type { ImportReport } from './bindings/ImportReport'
import type { Dashboard } from './bindings/Dashboard'
import type { ReminderQuery } from './bindings/ReminderQuery'
import type { RemindTarget } from './bindings/RemindTarget'
import type { NotesToTasks } from './bindings/NotesToTasks'
import type { RepeatingItem } from './bindings/RepeatingItem'
import type { UpcomingReminders } from './bindings/UpcomingReminders'
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
import type { Category } from './bindings/Category'
import type { DefaultColumn } from './bindings/DefaultColumn'
import type { DisplaySettings } from './bindings/DisplaySettings'
import type { ClientPatch } from './bindings/ClientPatch'
import type { DbInfo } from './bindings/DbInfo'
import type { Direction } from './bindings/Direction'
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
import type { NewSection } from './bindings/NewSection'
import type { ProjectPlace } from './bindings/ProjectPlace'
import type { SavedView } from './bindings/SavedView'
import type { Section } from './bindings/Section'
import type { SectionDelete } from './bindings/SectionDelete'
import type { SectionPatch } from './bindings/SectionPatch'
import type { Setup } from './bindings/Setup'
import type { SavedViewPatch } from './bindings/SavedViewPatch'
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
  setTaskArchived: (id: Id, archived: boolean) => invoke<Task>('set_task_archived', { id, archived }),
  bulkSetArchived: (ids: Id[], archived: boolean) => invoke<number>('bulk_set_archived', { ids, archived }),
  /** Archives the completed top-level tasks a list query matches; returns how many. */
  archiveCompleted: (query: TaskQuery) => invoke<number>('archive_completed', { query }),
  pinnedProjects: () => invoke<Id[]>('pinned_projects'),
  pinProject: (id: Id, before: Id | null = null) => invoke<Id[]>('pin_project', { id, before }),
  unpinProject: (id: Id) => invoke<Id[]>('unpin_project', { id }),
  setPinnedProjects: (ids: Id[]) => invoke<Id[]>('set_pinned_projects', { ids }),
  logUiError: (message: string, detail?: string) => invoke<void>('log_ui_error', { message, detail }),
  bulkUpdateTasks: (ids: Id[], patch: TaskPatch) =>
    invoke<number>('bulk_update_tasks', { ids, patch }),
  bulkSetDone: (ids: Id[], done: boolean) => invoke<number>('bulk_set_done', { ids, done }),
  bulkDelete: (ids: Id[]) => invoke<number>('bulk_delete', { ids }),
  createClient: (input: NewClient) => invoke<Client>('create_client', { input }),
  updateClient: (id: Id, patch: ClientPatch) => invoke<Client>('update_client', { id, patch }),
  setClientArchived: (id: Id, archived: boolean) =>
    invoke<Client>('set_client_archived', { id, archived }),
  deleteClient: (id: Id) => invoke<void>('delete_client', { id }),
  moveClient: (id: Id, direction: Direction) => invoke<boolean>('move_client', { id, direction }),
  listSections: () => invoke<Section[]>('list_sections'),
  createSection: (input: NewSection) => invoke<Section>('create_section', { input }),
  updateSection: (id: Id, patch: SectionPatch) => invoke<Section>('update_section', { id, patch }),
  moveSection: (id: Id, direction: Direction) => invoke<boolean>('move_section', { id, direction }),
  /** Drag and drop placement; `before` null = last. */
  placeSection: (id: Id, before: Id | null) => invoke<boolean>('place_section', { id, before }),
  placeClient: (id: Id, sectionId: Id, before: Id | null) => invoke<boolean>('place_client', { id, sectionId, before }),
  placeProject: (id: Id, to: ProjectPlace, before: Id | null) => invoke<boolean>('place_project', { id, to, before }),
  deleteSection: (id: Id, how: SectionDelete) => invoke<void>('delete_section', { id, how }),
  applySetup: (setup: Setup) => invoke<Section[]>('apply_setup', { setup }),
  moveProject: (id: Id, direction: Direction) => invoke<boolean>('move_project', { id, direction }),
  renderMarkdown: (source: string) => invoke<string>('render_markdown', { source }),
  savedViews: () => invoke<SavedView[]>('saved_views'),
  createSavedView: (name: string, specJson: string) =>
    invoke<SavedView>('create_saved_view', { name, specJson }),
  updateSavedView: (id: Id, patch: SavedViewPatch) => invoke<SavedView>('update_saved_view', { id, patch }),
  deleteSavedView: (id: Id) => invoke<void>('delete_saved_view', { id }),
  createProject: (input: NewProject) => invoke<Project>('create_project', { input }),
  updateProject: (id: Id, patch: ProjectPatch) => invoke<Project>('update_project', { id, patch }),
  setProjectArchived: (id: Id, archived: boolean) =>
    invoke<Project>('set_project_archived', { id, archived }),
  deleteProject: (id: Id) => invoke<void>('delete_project', { id }),

  // board columns
  allStatuses: () => invoke<Status[]>('all_statuses'),
  /** Tags in use, most used first. */
  listTags: () => invoke<string[]>('list_tags'),
  /** Reminders page: what will remind you in a window, and every repeating series. */
  upcomingReminders: (query: ReminderQuery) => invoke<UpcomingReminders>('upcoming_reminders', { query }),
  repeating: () => invoke<RepeatingItem[]>('repeating'),
  /** Reminders still to fire today (the sidebar badge). */
  remindersLeftToday: () => invoke<number>('reminders_left_today'),
  /** A task's or meeting's own reminder lead in minutes (null = the app's setting). */
  reminderLead: (target: RemindTarget) => invoke<number | null>('reminder_lead', { target }),
  setReminderLead: (target: RemindTarget, minutes: number | null) => invoke<void>('set_reminder_lead', { target, minutes }),
  /** Show a reminder again at `until` (Unix ms). */
  snoozeReminder: (key: string, until: number) => invoke<void>('snooze_reminder', { key, until }),
  /** `- [ ]` action items in notes become tasks; returns the marked notes. */
  tasksFromNotes: (notes: string, projectId: Id | null) => invoke<NotesToTasks>('tasks_from_notes', { notes, projectId }),
  exportListCsv: (query: TaskQuery, path: string) => invoke<void>('export_list_csv', { query, path }),
  /** Notes written for one occurrence of a series (null: it uses the series' notes). */
  occurrenceNotes: (id: Id, originalStart: number) => invoke<string | null>('occurrence_notes', { id, originalStart }),
  createStatus: (input: NewStatus) => invoke<Status>('create_status', { input }),
  /** Settings → Board columns: the columns new projects start with. */
  defaultColumns: () => invoke<DefaultColumn[]>('default_columns'),
  setDefaultColumns: (columns: DefaultColumn[]) => invoke<DefaultColumn[]>('set_default_columns', { columns }),
  resetDefaultColumns: () => invoke<DefaultColumn[]>('reset_default_columns'),
  projectsMissingColumns: () => invoke<number>('projects_missing_columns'),
  /** Adds the default columns each project lacks (one undo step); returns how many projects changed. */
  addMissingColumns: () => invoke<number>('add_missing_columns'),
  /** Adds a column (e.g. Waiting) to every project missing one by that name; returns how many got it. */
  addStatusEverywhere: (name: string, category: Category, color: string | null = null) =>
    invoke<number>('add_status_everywhere', { name, category, color }),
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
  /** How lists look (shared with assistants). */
  displaySettings: () => invoke<DisplaySettings>('display_settings'),
  setDisplaySettings: (settings: DisplaySettings) => invoke<DisplaySettings>('set_display_settings', { settings }),
  /** Settings → AI assistants: how to connect, and who has. */
  mcpInfo: () => invoke<McpInfo>('mcp_info'),
  mcpSelfTest: () => invoke<McpTest>('mcp_self_test'),
  setAppSettings: (settings: AppSettings) => invoke<AppSettings>('set_app_settings', { settings }),
  testNotification: () => invoke<void>('test_notification'),
  startHidden: () => invoke<boolean>('start_hidden'),

  // updates
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  /** Downloads, verifies, backs up and starts the installer (which closes and reopens the app). */
  installUpdate: (onProgress: (p: UpdateProgress) => void) => {
    const channel = new Channel<UpdateProgress>()
    channel.onmessage = onProgress
    return invoke<void>('install_update', { onProgress: channel })
  },
  /** The previous version, on the first launch after an update. */
  updatedFrom: () => invoke<string | null>('updated_from'),

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
  if (!/^(https?:\/\/|mailto:)/i.test(url)) return
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

/** An assistant that has connected over MCP (from the server's connection log). */
export interface McpConnection {
  name: string
  version: string
  last_seen: number
  connections: number
}
/** A newer release (Settings → Updates). */
export interface UpdateInfo {
  version: string
  current_version: string
  /** Unix ms. */
  date: number | null
  /** Markdown release notes. */
  notes: string | null
}

export type UpdateProgress = { event: 'downloading'; downloaded: number; total: number | null } | { event: 'installing' }

export interface McpInfo {
  command: string
  args: string[]
  db_path: string
  connections: McpConnection[]
}
export interface McpTest {
  ok: boolean
  message: string
  tools: number
}
