//! Thin command layer over `taskboard-core`. No business logic here.

use std::sync::Mutex;

use taskboard_core::{Db, DbInfo, ErrorInfo};
use tauri::{Manager, State, WebviewWindow};

/// M0: one connection behind a mutex. M1 replaces this with the writer
/// thread + read pool described in docs/ARCHITECTURE.md.
struct AppState {
    db: Mutex<Db>,
    /// True when the Mica backdrop applied; the UI uses solid surfaces otherwise.
    mica: bool,
}

type CmdResult<T> = Result<T, ErrorInfo>;

fn lock_error() -> ErrorInfo {
    ErrorInfo {
        code: "internal".into(),
        message: "database lock poisoned".into(),
    }
}

#[tauri::command]
fn db_info(state: State<'_, AppState>) -> CmdResult<DbInfo> {
    let db = state.db.lock().map_err(|_| lock_error())?;
    db.info().map_err(|e| ErrorInfo::from(&e))
}

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

fn internal(err: tauri::Error) -> ErrorInfo {
    ErrorInfo {
        code: "internal".into(),
        message: err.to_string(),
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

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db = Db::open(&data_dir.join("taskboard.db"))?;
            tracing::info!(path = %data_dir.display(), "database opened");
            let mica = app
                .get_webview_window("main")
                .is_some_and(|window| apply_backdrop(&window));
            app.manage(AppState {
                db: Mutex::new(db),
                mica,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db_info,
            has_mica,
            show_main_window
        ])
        .run(tauri::generate_context!());

    if let Err(err) = result {
        tracing::error!(%err, "taskboard failed to start");
        std::process::exit(1);
    }
}
