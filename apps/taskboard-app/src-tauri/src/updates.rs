//! In-app updates: check GitHub for a newer release, download it with
//! progress, back up the data, then hand over to the installer, which closes
//! Taskboard, installs, and opens the new version.
//!
//! The updater plugin verifies every download against the public key in
//! `tauri.conf.json` and the version it was signed for, so a tampered,
//! unsigned or older package is never installed. The UI drives the update
//! through these commands; the webview gets no updater permissions itself.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use taskboard_core::{Db, ErrorInfo};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

use crate::{AppState, CmdResult, internal};

/// The setting that remembers the last version that ran, to notice updates.
const LAST_VERSION: &str = "last_run_version";

/// The update found by the last check, kept for `install_update`.
#[derive(Default)]
pub(crate) struct PendingUpdate(Mutex<Option<Update>>);

/// A newer release, for Settings → Updates.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateInfo {
    version: String,
    current_version: String,
    /// When it was published (Unix ms), if the release says.
    date: Option<i64>,
    /// Release notes (Markdown), if any.
    notes: Option<String>,
}

/// Download progress, streamed to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub(crate) enum Progress {
    #[serde(rename_all = "camelCase")]
    Downloading { downloaded: u64, total: Option<u64> },
    /// Downloaded and verified; backing up, then the installer takes over.
    Installing,
}

/// Plain-language errors; the details go to the log.
fn update_error(err: Error) -> ErrorInfo {
    let (code, message) = match &err {
        Error::Reqwest(_) | Error::Network(_) => (
            "offline",
            "Couldn't reach GitHub. Check your internet connection and try again.",
        ),
        Error::ReleaseNotFound | Error::Serialization(_) => (
            "not_found",
            "No update information was found on GitHub. Try again later.",
        ),
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => (
            "not_found",
            "The latest release has no Windows update package. Try again later.",
        ),
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion
        | Error::InvalidUpdaterFormat => (
            "signature",
            "The download didn't pass its signature check, so nothing was installed.",
        ),
        _ => ("update", "The update couldn't be installed."),
    };
    tracing::warn!(%err, code, "update failed");
    let message = if code == "update" {
        format!("{message} {err}")
    } else {
        message.to_owned()
    };
    ErrorInfo {
        code: code.into(),
        message,
    }
}

#[tauri::command]
pub(crate) async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<UpdateInfo>> {
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(update_error)?;
    let found = updater.check().await.map_err(update_error)?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        date: u.date.map(|d| d.unix_timestamp() * 1000),
        notes: u.body.clone().filter(|n| !n.trim().is_empty()),
    });
    *pending.0.lock().map_err(internal)? = found;
    Ok(info)
}

/// Downloads and verifies the update found by `check_update`, backs up the
/// database, and starts the installer. On Windows the installer closes
/// Taskboard (AI assistant connections included) and opens the new version.
#[tauri::command]
pub(crate) async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
    on_progress: Channel<Progress>,
) -> CmdResult<()> {
    let mut update = pending
        .0
        .lock()
        .map_err(internal)?
        .clone()
        .ok_or_else(|| ErrorInfo {
            code: "invalid".into(),
            message: "Check for updates first.".into(),
        })?;
    // Generous, but a stalled download must end eventually.
    update.timeout = Some(Duration::from_secs(600));

    let mut downloaded = 0u64;
    let mut sent = Instant::now();
    let _ = on_progress.send(Progress::Downloading {
        downloaded: 0,
        total: None,
    });
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                // About ten updates a second is plenty for a progress bar.
                if sent.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
                    sent = Instant::now();
                    let _ = on_progress.send(Progress::Downloading { downloaded, total });
                }
            },
            || {},
        )
        .await
        .map_err(update_error)?;
    let _ = on_progress.send(Progress::Installing);

    // A verified copy of the data before the new version opens it.
    let db = Arc::clone(&state.db);
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let db = db.lock().map_err(|_| internal("database lock poisoned"))?;
        db.backup_now("pre-update").map_err(|e| ErrorInfo {
            code: "backup".into(),
            message: format!(
                "Couldn't back up your data first, so the update was not installed: {e}"
            ),
        })?;
        Ok(())
    })
    .await
    .map_err(internal)??;

    tracing::info!(version = %update.version, "installing update");
    // The installer ends the process without the usual exit, so save the
    // window's size and place now.
    {
        use tauri_plugin_window_state::AppHandleExt;
        if let Err(err) = app.save_window_state(crate::window_state_flags()) {
            tracing::warn!(%err, "couldn't save the window state before updating");
        }
    }
    // Windows: runs the installer and exits; the installer reopens Taskboard.
    update.install(bytes).map_err(update_error)?;
    app.restart()
}

/// The version that ran before this one, when this launch is the first since
/// an update (`None` on a fresh install or a normal launch). Records the
/// current version for next time.
pub(crate) fn note_version(db: &mut Db) -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let last: String = db.setting(LAST_VERSION).unwrap_or_default();
    if last != current
        && let Err(err) = db.set_setting(LAST_VERSION, &current)
    {
        tracing::warn!(%err, "could not record the app version");
    }
    (!last.is_empty() && last != current).then_some(last)
}

#[tauri::command]
pub(crate) fn updated_from(state: State<'_, AppState>) -> Option<String> {
    state.updated_from.clone()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn notices_an_update_once() {
        let mut db = Db::open_in_memory().unwrap();
        // Fresh install: nothing to report.
        assert_eq!(note_version(&mut db), None);
        assert_eq!(note_version(&mut db), None);
        // An older version ran before this one.
        db.set_setting(LAST_VERSION, &"0.0.1").unwrap();
        assert_eq!(note_version(&mut db).as_deref(), Some("0.0.1"));
        assert_eq!(note_version(&mut db), None);
    }

    #[test]
    fn errors_read_plainly() {
        let e = update_error(Error::ReleaseNotFound);
        assert_eq!(e.code, "not_found");
        let e = update_error(Error::Network(
            "Download request failed with status: 500".into(),
        ));
        assert_eq!(e.code, "offline");
        let e = update_error(Error::MissingSignedVersion);
        assert_eq!(e.code, "signature");
        assert!(!e.message.contains("minisign"));
    }
}
