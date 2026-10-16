//! Thin command layer over `taskboard-core`. No business logic here.
//!
//! Every command is `async` and runs its database work on a blocking worker
//! thread (Tauri runs non-async commands on the main thread, which would stall
//! the window). Mutations emit `data-changed` so every view re-queries.

mod data;
mod desktop;

use std::sync::{Arc, Mutex};

use desktop::focus_main;

use taskboard_core::{
    AppSettings, Client, ClientHours, ClientPatch, Dashboard, DayOverride, DayPlan, Db, DbInfo,
    ErrorInfo, Event, EventPatch, FindSlots, HistoryState, Id, LoggedTime, MoveTask, NewClient,
    NewEvent, NewProject, NewStatus, NewTask, NewTimeEntry, NotifySettings, Occurrence,
    OccurrencePatch, Project, ProjectPatch, ProjectSummary, QuickAddPreview, SidebarCounts,
    SlotResult, Status, StatusPatch, Task, TaskPage, TaskPatch, TaskQuery, TaskSummary,
    TimeEntryPatch, WorkingDay,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// One connection behind a mutex, only ever locked from worker threads.
/// A dedicated writer thread + read pool can replace this if profiling asks.
pub(crate) struct AppState {
    pub(crate) db: Arc<Mutex<Db>>,
    /// Launched by autostart with `--minimized`: stay in the tray.
    start_hidden: bool,
    /// Process start, for the cold-start measurement (SPEC §7).
    started: std::time::Instant,
    /// True when the Mica backdrop applied; the UI uses solid surfaces otherwise.
    mica: bool,
}

pub(crate) type CmdResult<T> = Result<T, ErrorInfo>;

pub(crate) fn internal(message: impl ToString) -> ErrorInfo {
    ErrorInfo {
        code: "internal".into(),
        message: message.to_string(),
    }
}

pub(crate) fn today() -> jiff::civil::Date {
    jiff::Zoned::now().date()
}

pub(crate) async fn read<T, F>(state: State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Db) -> taskboard_core::Result<T> + Send + 'static,
{
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || {
        let guard = db.lock().map_err(|_| internal("database lock poisoned"))?;
        f(&guard).map_err(|e| ErrorInfo::from(&e))
    })
    .await
    .map_err(internal)?
}

pub(crate) async fn write<T, F>(app: AppHandle, state: State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Db) -> taskboard_core::Result<T> + Send + 'static,
{
    let db = Arc::clone(&state.db);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut guard = db.lock().map_err(|_| internal("database lock poisoned"))?;
        f(&mut guard).map_err(|e| ErrorInfo::from(&e))
    })
    .await
    .map_err(internal)?;
    if result.is_ok()
        && let Err(err) = app.emit("data-changed", ())
    {
        tracing::warn!(%err, "failed to emit data-changed");
    }
    result
}

// ---------- reads ----------

#[tauri::command]
async fn db_info(state: State<'_, AppState>) -> CmdResult<DbInfo> {
    read(state, Db::info).await
}

#[tauri::command]
async fn history_state(state: State<'_, AppState>) -> CmdResult<HistoryState> {
    read(state, Db::history_state).await
}

#[tauri::command]
async fn list_clients(
    state: State<'_, AppState>,
    include_archived: bool,
) -> CmdResult<Vec<Client>> {
    read(state, move |db| db.clients(include_archived)).await
}

#[tauri::command]
async fn list_projects(
    state: State<'_, AppState>,
    include_archived: bool,
) -> CmdResult<Vec<ProjectSummary>> {
    read(state, move |db| db.projects(include_archived)).await
}

#[tauri::command]
async fn list_statuses(state: State<'_, AppState>, project_id: Id) -> CmdResult<Vec<Status>> {
    read(state, move |db| db.statuses(project_id)).await
}

#[tauri::command]
async fn list_tasks(state: State<'_, AppState>, query: TaskQuery) -> CmdResult<TaskPage> {
    read(state, move |db| db.list_tasks(&query)).await
}

#[tauri::command]
async fn summarize_tasks(state: State<'_, AppState>, query: TaskQuery) -> CmdResult<TaskSummary> {
    read(state, move |db| db.summarize_tasks(&query)).await
}

#[tauri::command]
async fn get_task(state: State<'_, AppState>, id: Id) -> CmdResult<Task> {
    read(state, move |db| db.task(id)).await
}

#[tauri::command]
async fn sidebar_counts(state: State<'_, AppState>) -> CmdResult<SidebarCounts> {
    read(state, |db| db.sidebar_counts(today())).await
}

#[tauri::command]
async fn preview_quick_add(state: State<'_, AppState>, line: String) -> CmdResult<QuickAddPreview> {
    read(state, move |db| db.preview_quick_add(&line, today())).await
}

// ---------- writes ----------

#[tauri::command]
async fn undo(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    write(app, state, Db::undo).await
}

#[tauri::command]
async fn redo(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    write(app, state, Db::redo).await
}

#[tauri::command]
async fn quick_add(app: AppHandle, state: State<'_, AppState>, line: String) -> CmdResult<Task> {
    write(app, state, move |db| db.quick_add(&line, today())).await
}

#[tauri::command]
async fn create_task(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewTask,
) -> CmdResult<Task> {
    write(app, state, move |db| db.create_task(input)).await
}

#[tauri::command]
async fn update_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: TaskPatch,
) -> CmdResult<Task> {
    write(app, state, move |db| db.update_task(id, patch)).await
}

#[tauri::command]
async fn set_task_done(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    done: bool,
) -> CmdResult<Task> {
    write(app, state, move |db| db.set_task_done(id, done)).await
}

#[tauri::command]
async fn move_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    to: MoveTask,
) -> CmdResult<Task> {
    write(app, state, move |db| db.move_task(id, to)).await
}

#[tauri::command]
async fn delete_task(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<()> {
    write(app, state, move |db| db.delete_task(id)).await
}

#[tauri::command]
async fn duplicate_task(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<Task> {
    write(app, state, move |db| db.duplicate_task(id)).await
}

/// Applies one patch to many tasks as a single undoable action per task.
/// (A combined undo group for bulk edits is a core follow-up.)
#[tauri::command]
async fn bulk_update_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<Id>,
    patch: TaskPatch,
) -> CmdResult<usize> {
    write(app, state, move |db| {
        for id in &ids {
            db.update_task(*id, patch.clone())?;
        }
        Ok(ids.len())
    })
    .await
}

#[tauri::command]
async fn bulk_set_done(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<Id>,
    done: bool,
) -> CmdResult<usize> {
    write(app, state, move |db| {
        for id in &ids {
            db.set_task_done(*id, done)?;
        }
        Ok(ids.len())
    })
    .await
}

#[tauri::command]
async fn bulk_delete(app: AppHandle, state: State<'_, AppState>, ids: Vec<Id>) -> CmdResult<usize> {
    write(app, state, move |db| {
        for id in &ids {
            db.delete_task(*id)?;
        }
        Ok(ids.len())
    })
    .await
}

#[tauri::command]
async fn create_client(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewClient,
) -> CmdResult<Client> {
    write(app, state, move |db| db.create_client(input)).await
}

#[tauri::command]
async fn update_client(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: ClientPatch,
) -> CmdResult<Client> {
    write(app, state, move |db| db.update_client(id, patch)).await
}

#[tauri::command]
async fn set_client_archived(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    archived: bool,
) -> CmdResult<Client> {
    write(app, state, move |db| db.set_client_archived(id, archived)).await
}

#[tauri::command]
async fn delete_client(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<()> {
    write(app, state, move |db| db.delete_client(id)).await
}

#[tauri::command]
async fn create_project(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewProject,
) -> CmdResult<Project> {
    write(app, state, move |db| db.create_project(input)).await
}

#[tauri::command]
async fn update_project(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: ProjectPatch,
) -> CmdResult<Project> {
    write(app, state, move |db| db.update_project(id, patch)).await
}

#[tauri::command]
async fn set_project_archived(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    archived: bool,
) -> CmdResult<Project> {
    write(app, state, move |db| db.set_project_archived(id, archived)).await
}

#[tauri::command]
async fn delete_project(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<()> {
    write(app, state, move |db| db.delete_project(id)).await
}

// ---------- board columns ----------

#[tauri::command]
async fn all_statuses(state: State<'_, AppState>) -> CmdResult<Vec<Status>> {
    read(state, Db::all_statuses).await
}

#[tauri::command]
async fn create_status(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewStatus,
) -> CmdResult<Status> {
    write(app, state, move |db| db.create_status(input)).await
}

#[tauri::command]
async fn update_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: StatusPatch,
) -> CmdResult<Status> {
    write(app, state, move |db| db.update_status(id, patch)).await
}

#[tauri::command]
async fn move_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    after: Option<Id>,
) -> CmdResult<Status> {
    write(app, state, move |db| db.move_status(id, after)).await
}

#[tauri::command]
async fn delete_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    move_to: Id,
) -> CmdResult<()> {
    write(app, state, move |db| db.delete_status(id, move_to)).await
}

// ---------- calendar: meetings & blocks ----------

#[tauri::command]
async fn occurrences(state: State<'_, AppState>, from: i64, to: i64) -> CmdResult<Vec<Occurrence>> {
    read(state, move |db| db.occurrences(from, to)).await
}

#[tauri::command]
async fn get_event(state: State<'_, AppState>, id: Id) -> CmdResult<Event> {
    read(state, move |db| db.event(id)).await
}

#[tauri::command]
async fn create_event(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewEvent,
) -> CmdResult<Event> {
    write(app, state, move |db| db.create_event(input)).await
}

#[tauri::command]
async fn update_event(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: EventPatch,
) -> CmdResult<Event> {
    write(app, state, move |db| db.update_event(id, patch)).await
}

#[tauri::command]
async fn delete_event(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<()> {
    write(app, state, move |db| db.delete_event(id)).await
}

#[tauri::command]
async fn edit_occurrence(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    original_start: i64,
    patch: OccurrencePatch,
) -> CmdResult<()> {
    write(app, state, move |db| {
        db.edit_occurrence(id, original_start, patch)
    })
    .await
}

#[tauri::command]
async fn split_series(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    original_start: i64,
    patch: EventPatch,
) -> CmdResult<Event> {
    write(app, state, move |db| {
        db.split_series(id, original_start, patch)
    })
    .await
}

// ---------- working hours ----------

#[tauri::command]
async fn working_hours(state: State<'_, AppState>) -> CmdResult<Vec<WorkingDay>> {
    read(state, Db::working_hours).await
}

#[tauri::command]
async fn set_working_hours(
    app: AppHandle,
    state: State<'_, AppState>,
    week: Vec<WorkingDay>,
) -> CmdResult<Vec<WorkingDay>> {
    write(app, state, move |db| db.set_working_hours(week)).await
}

#[tauri::command]
async fn day_overrides(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<Vec<DayOverride>> {
    read(state, move |db| db.day_overrides(&from, &to)).await
}

#[tauri::command]
async fn set_day_override(
    app: AppHandle,
    state: State<'_, AppState>,
    day: DayOverride,
) -> CmdResult<()> {
    write(app, state, move |db| db.set_day_override(day)).await
}

#[tauri::command]
async fn clear_day_override(
    app: AppHandle,
    state: State<'_, AppState>,
    date: String,
) -> CmdResult<()> {
    write(app, state, move |db| db.clear_day_override(&date)).await
}

// ---------- time tracking & planning ----------

#[tauri::command]
async fn running_timer(state: State<'_, AppState>) -> CmdResult<Option<LoggedTime>> {
    read(state, Db::running_timer).await
}

#[tauri::command]
async fn log_time(
    app: AppHandle,
    state: State<'_, AppState>,
    input: NewTimeEntry,
) -> CmdResult<Id> {
    write(app, state, move |db| db.log_time(input)).await
}

#[tauri::command]
async fn stop_timer(app: AppHandle, state: State<'_, AppState>) -> CmdResult<bool> {
    write(app, state, Db::stop_timer).await
}

#[tauri::command]
async fn update_time_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Id,
    patch: TimeEntryPatch,
) -> CmdResult<()> {
    write(app, state, move |db| db.update_time_entry(id, patch)).await
}

#[tauri::command]
async fn delete_time_entry(app: AppHandle, state: State<'_, AppState>, id: Id) -> CmdResult<()> {
    write(app, state, move |db| db.delete_time_entry(id)).await
}

#[tauri::command]
async fn logged_time(state: State<'_, AppState>, from: i64, to: i64) -> CmdResult<Vec<LoggedTime>> {
    read(state, move |db| db.logged_time(from, to)).await
}

#[tauri::command]
async fn day_plans(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<Vec<DayPlan>> {
    read(state, move |db| db.day_plans(&from, &to)).await
}

#[tauri::command]
async fn hours_by_client(
    state: State<'_, AppState>,
    from: i64,
    to: i64,
) -> CmdResult<Vec<ClientHours>> {
    read(state, move |db| db.hours_by_client(from, to)).await
}

#[tauri::command]
async fn find_slots(state: State<'_, AppState>, query: FindSlots) -> CmdResult<SlotResult> {
    read(state, move |db| db.find_slots(&query)).await
}

// ---------- settings, reminders & dashboard ----------

#[tauri::command]
async fn dashboard(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    read(state, |db| db.dashboard(today())).await
}

#[tauri::command]
async fn notify_settings(state: State<'_, AppState>) -> CmdResult<NotifySettings> {
    read(state, Db::notify_settings).await
}

#[tauri::command]
async fn set_notify_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: NotifySettings,
) -> CmdResult<NotifySettings> {
    write(app, state, move |db| db.set_notify_settings(settings)).await
}

#[tauri::command]
async fn app_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    read(state, Db::app_settings).await
}

#[tauri::command]
async fn set_app_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    let autostart = settings.autostart;
    let saved = write(app.clone(), state, move |db| db.set_app_settings(settings)).await?;
    desktop::sync_autostart(&app, autostart);
    Ok(saved)
}

/// Shows a sample reminder so the user can check Windows notifications work.
#[tauri::command]
fn test_notification(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title("Taskboard reminders are on")
        .body("You'll see meetings and timed tasks here.")
        .show()
        .map_err(internal)
}

#[tauri::command]
fn start_hidden(state: State<'_, AppState>) -> bool {
    state.start_hidden
}

// ---------- window ----------

#[tauri::command]
fn has_mica(state: State<'_, AppState>) -> bool {
    state.mica
}

/// The window starts hidden so the user never sees an unthemed frame;
/// the UI calls this after its first paint.
#[tauri::command]
fn show_main_window(window: WebviewWindow, state: State<'_, AppState>) -> CmdResult<()> {
    tracing::info!(
        ms = state.started.elapsed().as_millis() as u64,
        "window shown (cold start to first paint)"
    );
    window.show().map_err(internal)?;
    window.set_focus().map_err(internal)
}

#[cfg(target_os = "windows")]
fn apply_backdrop(window: &WebviewWindow) -> bool {
    // Mica needs Windows 11; on Windows 10 the UI falls back to solid surfaces.
    match window_vibrancy::apply_mica(window, None) {
        Ok(()) => true,
        Err(err) => {
            tracing::info!(%err, "mica backdrop unavailable");
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_backdrop(_window: &WebviewWindow) -> bool {
    false
}

/// Ctrl+Shift+Space from anywhere: bring Taskboard forward with Quick Add focused.
fn quick_add_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

/// Local diagnostics: stdout plus daily log files (7 kept) in
/// `%APPDATA%/app.taskboard.desktop/logs`. Nothing leaves the machine.
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let filter =
        || tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let dir = std::env::var_os("APPDATA").map(|d| {
        std::path::PathBuf::from(d)
            .join("app.taskboard.desktop")
            .join("logs")
    });
    let file = dir.and_then(|dir| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("taskboard")
            .filename_suffix("log")
            .max_log_files(7)
            .build(dir)
            .ok()
    });
    let (file_layer, guard) = match file {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (
                Some(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(writer)
                        .with_filter(filter()),
                ),
                Some(guard),
            )
        }
        None => (None, None),
    };
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .with(file_layer)
        .try_init();
    guard
}

pub fn run() {
    let started = std::time::Instant::now();
    let _log_guard = init_logging();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![desktop::MINIMIZED_FLAG]),
        ))
        .on_window_event(|window, event| {
            // Closing hides to the tray (so reminders keep firing) unless turned off.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window
                    .app_handle()
                    .try_state::<AppState>()
                    .and_then(|s| s.db.lock().ok().and_then(|db| db.app_settings().ok()))
                    .is_none_or(|s| s.close_to_tray);
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if *shortcut == quick_add_shortcut() && event.state() == ShortcutState::Pressed
                    {
                        focus_main(app);
                        let _ = app.emit("quick-add", ());
                    }
                })
                .build(),
        )
        .setup(move |app| {
            // TASKBOARD_DB points the app at another file (development, demos).
            let path = match std::env::var_os("TASKBOARD_DB") {
                Some(path) => std::path::PathBuf::from(path),
                None => app.path().app_data_dir()?.join("taskboard.db"),
            };
            let db = Db::open_deferred_check(&path)?;
            tracing::info!(
                path = %path.display(),
                ms = started.elapsed().as_millis() as u64,
                "database opened"
            );
            let mica = app
                .get_webview_window("main")
                .is_some_and(|window| apply_backdrop(&window));
            let settings = db.app_settings().unwrap_or_default();
            let db = Arc::new(Mutex::new(db));
            let start_hidden = std::env::args().any(|a| a == desktop::MINIMIZED_FLAG);
            app.manage(AppState {
                db: Arc::clone(&db),
                start_hidden,
                started,
                mica,
            });
            if let Err(err) = desktop::build_tray(app.handle()) {
                tracing::warn!(%err, "tray icon unavailable");
            }
            desktop::sync_autostart(app.handle(), settings.autostart);
            desktop::spawn_integrity_check(app.handle().clone(), path);
            desktop::spawn_reminders(app.handle().clone(), db);
            // Another app may own the shortcut; Taskboard still works without it.
            if let Err(err) = app.global_shortcut().register(quick_add_shortcut()) {
                tracing::warn!(%err, "quick-add shortcut unavailable");
            }
            tracing::info!(ms = started.elapsed().as_millis() as u64, "setup done");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_info,
            history_state,
            list_clients,
            list_projects,
            list_statuses,
            list_tasks,
            summarize_tasks,
            get_task,
            sidebar_counts,
            preview_quick_add,
            undo,
            redo,
            quick_add,
            create_task,
            update_task,
            set_task_done,
            move_task,
            delete_task,
            duplicate_task,
            bulk_update_tasks,
            bulk_set_done,
            bulk_delete,
            create_client,
            update_client,
            set_client_archived,
            delete_client,
            create_project,
            update_project,
            set_project_archived,
            delete_project,
            all_statuses,
            create_status,
            update_status,
            move_status,
            delete_status,
            occurrences,
            get_event,
            create_event,
            update_event,
            delete_event,
            edit_occurrence,
            split_series,
            working_hours,
            set_working_hours,
            day_overrides,
            set_day_override,
            clear_day_override,
            running_timer,
            log_time,
            stop_timer,
            update_time_entry,
            delete_time_entry,
            logged_time,
            day_plans,
            hours_by_client,
            find_slots,
            has_mica,
            data::list_backups,
            data::backup_now,
            data::open_backups_folder,
            data::restore_backup,
            data::export_data,
            data::import_data,
            data::has_sample,
            data::create_sample,
            data::remove_sample,
            dashboard,
            notify_settings,
            set_notify_settings,
            app_settings,
            set_app_settings,
            test_notification,
            start_hidden,
            show_main_window,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "taskboard failed to start");
        std::process::exit(1);
    }
}
