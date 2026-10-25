//! Windows desktop integration: tray icon, close-to-tray, autostart and the
//! reminder scheduler (SPEC §5 Notifications, §14 decisions).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use taskboard_core::Db;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

/// Command-line flag the autostart entry passes, so login starts in the tray.
pub const MINIMIZED_FLAG: &str = "--minimized";

pub fn focus_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Taskboard", true, None::<&str>)?;
    let quick = MenuItem::with_id(
        app,
        "quick-add",
        "Quick add…",
        true,
        Some("Ctrl+Shift+Space"),
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Taskboard", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &quick, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Taskboard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => focus_main(app),
            "quick-add" => {
                focus_main(app);
                let _ = app.emit("quick-add", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Makes the OS autostart entry match the user's setting.
pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    // Development builds and runs on another database (demos, tests) must not
    // repoint the user's login entry at a build folder.
    if cfg!(debug_assertions) || std::env::var_os("TASKBOARD_DB").is_some() {
        return;
    }
    let launcher = app.autolaunch();
    let result = if enabled {
        // Always rewrite: the entry must point at this exe even if an older
        // install or another copy registered a different path.
        launcher.enable()
    } else if launcher.is_enabled().unwrap_or(false) {
        launcher.disable()
    } else {
        Ok(())
    };
    if let Err(err) = result {
        tracing::warn!(%err, "could not update autostart");
    }
}

/// Startup integrity check, off the critical path (SPEC §5): delayed, and on
/// its own read-only connection so it never holds the lock the UI needs. On
/// damage the UI offers to restore the latest backup.
pub fn spawn_integrity_check(app: AppHandle, path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("integrity".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(30));
            if let Err(err) = Db::check_integrity_file(&path) {
                tracing::error!(%err, "database integrity check failed");
                let _ = app.emit("integrity-failed", err.to_string());
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "integrity check not started");
    }
}

/// Polls for due reminders every 20 seconds; shows each as a Windows toast and
/// forwards it to the window (for the in-app banner with a Join button).
pub fn spawn_reminders(app: AppHandle, db: Arc<Mutex<Db>>) {
    std::thread::Builder::new()
        .name("reminders".into())
        .spawn(move || {
            // First backup check two minutes after launch, so it never competes
            // with startup for the database lock.
            let mut last_backup_check = taskboard_core::clock::now_ms() - 3_600_000 + 120_000;
            loop {
                let now = taskboard_core::clock::now_ms();
                // Automatic daily backup (checked hourly; cheap when already done).
                if now - last_backup_check > 3_600_000 {
                    last_backup_check = now;
                    if let Ok(mut db) = db.lock() {
                        match db.ensure_daily_backup() {
                            // Today's backup holds the full history; older
                            // steps can go from the live file.
                            Ok(Some(_)) => {
                                if let Err(err) = db.compact_old_history(now) {
                                    tracing::warn!(%err, "history compaction failed");
                                }
                            }
                            Ok(None) => {}
                            Err(err) => tracing::warn!(%err, "daily backup failed"),
                        }
                    }
                }
                let due = match db.lock() {
                    Ok(mut db) => match db.pending_notifications(now) {
                        Ok(list) => {
                            if !list.is_empty() {
                                let keys: Vec<String> =
                                    list.iter().map(|n| n.key.clone()).collect();
                                if let Err(err) = db.mark_notified(&keys, now) {
                                    tracing::warn!(%err, "could not record reminders");
                                }
                            }
                            list
                        }
                        Err(err) => {
                            tracing::warn!(%err, "reminder check failed");
                            Vec::new()
                        }
                    },
                    Err(_) => return,
                };
                for n in &due {
                    let body = match &n.url {
                        Some(_) => format!("{}\nOpen Taskboard to join.", n.body),
                        None => n.body.clone(),
                    };
                    if let Err(err) = app
                        .notification()
                        .builder()
                        .title(&n.title)
                        .body(body)
                        .show()
                    {
                        tracing::warn!(%err, "could not show notification");
                    }
                }
                if !due.is_empty() {
                    let _ = app.emit("reminders", &due);
                }
                std::thread::sleep(Duration::from_secs(20));
            }
        })
        .map(drop)
        .unwrap_or_else(|err| tracing::error!(%err, "reminder thread failed to start"));
}
