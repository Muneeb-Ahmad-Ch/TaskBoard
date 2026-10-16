//! Thin command layer over `taskboard-core`. No business logic here.
//!
//! Every command is `async` and runs its database work on a blocking worker
//! thread (Tauri runs non-async commands on the main thread, which would stall
//! the window). Mutations emit `data-changed` so every view re-queries.

use std::sync::{Arc, Mutex};

use taskboard_core::{
    Client, ClientPatch, Db, DbInfo, ErrorInfo, HistoryState, Id, MoveTask, NewClient, NewProject,
    NewStatus, NewTask, Project, ProjectPatch, ProjectSummary, QuickAddPreview, SidebarCounts,
    Status, StatusPatch, Task, TaskPage, TaskPatch, TaskQuery, TaskSummary,
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// One connection behind a mutex, only ever locked from worker threads.
/// A dedicated writer thread + read pool can replace this if profiling asks.
struct AppState {
    db: Arc<Mutex<Db>>,
    /// True when the Mica backdrop applied; the UI uses solid surfaces otherwise.
    mica: bool,
}

type CmdResult<T> = Result<T, ErrorInfo>;

fn internal(message: impl ToString) -> ErrorInfo {
    ErrorInfo {
        code: "internal".into(),
        message: message.to_string(),
    }
}

fn today() -> jiff::civil::Date {
    jiff::Zoned::now().date()
}

async fn read<T, F>(state: State<'_, AppState>, f: F) -> CmdResult<T>
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

async fn write<T, F>(app: AppHandle, state: State<'_, AppState>, f: F) -> CmdResult<T>
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

// ---------- window ----------

#[tauri::command]
fn has_mica(state: State<'_, AppState>) -> bool {
    state.mica
}

/// The window starts hidden so the user never sees an unthemed frame;
/// the UI calls this after its first paint.
#[tauri::command]
fn show_main_window(window: WebviewWindow) -> CmdResult<()> {
    window.show().map_err(internal)?;
    window.set_focus().map_err(internal)
}

fn focus_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
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

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
        .setup(|app| {
            // TASKBOARD_DB points the app at another file (development, demos).
            let path = match std::env::var_os("TASKBOARD_DB") {
                Some(path) => std::path::PathBuf::from(path),
                None => app.path().app_data_dir()?.join("taskboard.db"),
            };
            let db = Db::open(&path)?;
            tracing::info!(path = %path.display(), "database opened");
            let mica = app
                .get_webview_window("main")
                .is_some_and(|window| apply_backdrop(&window));
            app.manage(AppState {
                db: Arc::new(Mutex::new(db)),
                mica,
            });
            // Another app may own the shortcut; Taskboard still works without it.
            if let Err(err) = app.global_shortcut().register(quick_add_shortcut()) {
                tracing::warn!(%err, "quick-add shortcut unavailable");
            }
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
            has_mica,
            show_main_window,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "taskboard failed to start");
        std::process::exit(1);
    }
}
