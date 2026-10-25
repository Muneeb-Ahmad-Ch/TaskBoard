//! Data commands: backups, restore, export/import, sample workspace.

use std::path::{Path, PathBuf};

use taskboard_core::{BackupInfo, Db, ErrorInfo, ImportReport, TaskQuery};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::{AppState, CmdResult, internal, read, today, write};

fn err(e: taskboard_core::Error) -> ErrorInfo {
    ErrorInfo::from(&e)
}

/// Writes via a temp file + rename so a crash never leaves a half-written export.
fn write_atomic(path: &Path, contents: &str) -> CmdResult<()> {
    let tmp = path.with_extension("taskboard-tmp");
    std::fs::write(&tmp, contents).map_err(internal)?;
    std::fs::rename(&tmp, path).map_err(internal)
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    read(state, Db::list_backups).await
}

#[tauri::command]
pub async fn backup_now(state: State<'_, AppState>) -> CmdResult<BackupInfo> {
    read(state, |db| db.backup_now("manual")).await
}

#[tauri::command]
pub async fn open_backups_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    // On a worker like every other command, so a running backup can't freeze the window.
    let dir = read(state, |db| db.backup_dir()).await?;
    std::fs::create_dir_all(&dir).map_err(internal)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(internal)
}

/// Replaces the current database with a backup. A `pre-restore` backup of the
/// current data is taken first, so the restore itself can be reverted.
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    let db = std::sync::Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let mut guard = db.lock().map_err(|_| internal("database lock poisoned"))?;
        let backup = PathBuf::from(&path);
        taskboard_core::backup::verify(&backup).map_err(err)?;
        guard.backup_now("pre-restore").map_err(err)?;
        // Through the open connection: works while an assistant's server has
        // the file open, and the app stays on its file even if it fails.
        guard.restore_from(&backup).map_err(err)
    })
    .await
    .map_err(internal)??;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn export_data(state: State<'_, AppState>, kind: String, path: String) -> CmdResult<()> {
    let contents = read(state, move |db| match kind.as_str() {
        "json" => db.export_json(),
        "csv" => db.export_tasks_csv(&TaskQuery {
            include_archived: true,
            ..Default::default()
        }),
        "ics" => db.export_ics(),
        other => Err(taskboard_core::Error::InvalidInput(format!(
            "unknown export type {other}"
        ))),
    })
    .await?;
    write_atomic(Path::new(&path), &contents)
}

/// The list on screen (its scope, filters and sort) as CSV.
#[tauri::command]
pub async fn export_list_csv(
    state: State<'_, AppState>,
    query: TaskQuery,
    path: String,
) -> CmdResult<()> {
    let contents = read(state, move |db| db.export_tasks_csv(&query)).await?;
    write_atomic(Path::new(&path), &contents)
}

/// Imports a file; a `pre-import` backup is taken first.
#[tauri::command]
pub async fn import_data(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    path: String,
) -> CmdResult<ImportReport> {
    let text = std::fs::read_to_string(&path).map_err(internal)?;
    write(app, state, move |db| {
        if db.path().is_some() {
            db.backup_now("pre-import")?;
        }
        match kind.as_str() {
            "json" => db.import_json(&text),
            "csv" => db.import_tasks_csv(&text, None),
            "ics" => db.import_ics(&text),
            other => Err(taskboard_core::Error::InvalidInput(format!(
                "unknown import type {other}"
            ))),
        }
    })
    .await
}

#[tauri::command]
pub async fn has_sample(state: State<'_, AppState>) -> CmdResult<bool> {
    read(state, Db::has_sample_workspace).await
}

#[tauri::command]
pub async fn create_sample(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    write(app, state, |db| db.create_sample_workspace(today())).await
}

#[tauri::command]
pub async fn remove_sample(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    write(app, state, Db::remove_sample_workspace).await
}
