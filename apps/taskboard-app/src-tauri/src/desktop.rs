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

/// A window rectangle (x, y, width, height) in physical pixels.
type Rect = (i32, i32, u32, u32);

/// Where a window must go to sit fully inside `area` (a monitor's work area,
/// i.e. without the taskbar): no larger than it, and moved in from its edges.
/// `None` when it already fits.
fn fit_into(win: Rect, area: Rect) -> Option<Rect> {
    let (x, y, w, h) = win;
    let (ax, ay, aw, ah) = area;
    let (w2, h2) = (w.min(aw), h.min(ah));
    let room_x = ax + i32::try_from(aw - w2).unwrap_or(0);
    let room_y = ay + i32::try_from(ah - h2).unwrap_or(0);
    let fitted = (x.clamp(ax, room_x), y.clamp(ay, room_y), w2, h2);
    (fitted != win).then_some(fitted)
}

/// Brings the main window back inside the screen if a saved size or place
/// (from another monitor or display scale) put part of it off screen or under
/// the taskbar. Maximized windows are left to Windows.
pub fn keep_on_screen(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        return;
    }
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(pos), Ok(size)) =
        (monitor, window.outer_position(), window.outer_size())
    else {
        return;
    };
    let area = monitor.work_area();
    let target = fit_into(
        (pos.x, pos.y, size.width, size.height),
        (
            area.position.x,
            area.position.y,
            area.size.width,
            area.size.height,
        ),
    );
    if let Some((x, y, w, h)) = target {
        tracing::info!(?target, "window was off screen; moved inside");
        let _ = window.set_size(tauri::PhysicalSize::new(w, h));
        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::fit_into;

    const SCREEN: (i32, i32, u32, u32) = (0, 0, 1920, 1040);

    #[test]
    fn a_window_that_fits_stays() {
        assert_eq!(fit_into((100, 50, 1280, 800), SCREEN), None);
    }

    #[test]
    fn a_window_taller_than_the_screen_shrinks_and_moves_up() {
        assert_eq!(
            fit_into((0, 0, 1920, 1120), SCREEN),
            Some((0, 0, 1920, 1040))
        );
        assert_eq!(
            fit_into((200, 400, 1280, 800), SCREEN),
            Some((200, 240, 1280, 800))
        );
    }

    #[test]
    fn a_window_on_a_monitor_that_is_gone_comes_back() {
        assert_eq!(
            fit_into((2500, 100, 1280, 800), SCREEN),
            Some((640, 100, 1280, 800))
        );
        assert_eq!(
            fit_into((-1500, -30, 1280, 800), SCREEN),
            Some((0, 0, 1280, 800))
        );
    }

    #[test]
    fn a_second_monitor_s_work_area_counts_from_its_corner() {
        let right = (1920, 0, 2560, 1400);
        assert_eq!(fit_into((2000, 100, 1280, 800), right), None);
        assert_eq!(
            fit_into((4000, 100, 1280, 800), right),
            Some((3200, 100, 1280, 800))
        );
    }
}
