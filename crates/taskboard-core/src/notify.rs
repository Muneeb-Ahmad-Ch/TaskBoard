//! Reminders (SPEC §5 Notifications): meetings shortly before they start,
//! timed tasks at their due time, and an optional morning digest. The app
//! polls `pending_notifications`, shows them, then calls `mark_notified`, so
//! each fires once — including ones missed while the app was closed, within a
//! short grace window.

use jiff::civil::{Date, Time};
use rusqlite::{OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Result, invalid};
use crate::events::{EventKind, EventStatus, local_zone};
use crate::id::Id;
use crate::model::{Category, clean_time, text};
use crate::query::TaskQuery;
use crate::storage::Db;
use crate::tx::Table;

const MINUTE: i64 = 60_000;
const NOTIFY_KEY: &str = "notify";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct NotifySettings {
    pub enabled: bool,
    /// Minutes before a meeting starts.
    pub meeting_lead_min: i64,
    /// Minutes before a timed task is due (0 = at the due time).
    pub task_lead_min: i64,
    pub digest: bool,
    /// Local `HH:MM`.
    pub digest_time: String,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            meeting_lead_min: 10,
            task_lead_min: 0,
            digest: true,
            digest_time: "09:00".into(),
        }
    }
}

/// Desktop behaviour (SPEC §14 decisions: tray-resident and autostart by default).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AppSettings {
    /// Closing the window keeps Taskboard running in the tray (so reminders fire).
    pub close_to_tray: bool,
    /// Start with Windows, minimized to the tray.
    pub autostart: bool,
    /// AI assistants connected over MCP may read and change data.
    pub ai_access: bool,
}

/// How lists look; stored with the data so assistants can change it too
/// (Settings → Views & table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct DisplaySettings {
    /// In a group's list, rows leave out the group's name; in a project's
    /// list (and the Inbox), the Group / Project column is hidden.
    pub hide_repeated_context: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            hide_repeated_context: true,
        }
    }
}

const DISPLAY_KEY: &str = "display";

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            autostart: true,
            ai_access: true,
        }
    }
}

const APP_KEY: &str = "app";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum NotificationKind {
    Meeting,
    Task,
    Digest,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Notification {
    /// Dedup key; pass back to `mark_notified`.
    pub key: String,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    /// When the meeting starts / the task is due.
    pub at: i64,
    /// Meeting link, for a Join action.
    pub url: Option<String>,
    pub task_id: Option<Id>,
    pub event_id: Option<Id>,
}

fn fmt_hm(ms: i64) -> String {
    let (tz, _) = local_zone();
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| {
            let z = t.to_zoned(tz);
            format!("{:02}:{:02}", z.hour(), z.minute())
        })
        .unwrap_or_default()
}

fn local_ms(date: Date, time: Time) -> Option<i64> {
    let (tz, _) = local_zone();
    tz.to_zoned(date.to_datetime(time))
        .ok()
        .map(|z| z.timestamp().as_millisecond())
}

impl Db {
    /// A JSON setting, or its default when unset or unreadable.
    pub fn setting<T: DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        let raw: Option<String> = self
            .conn()
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    /// Stores a JSON setting. Not an undoable action (Ctrl+Z is for your work).
    pub fn set_setting<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)?;
        let key = key.to_owned();
        self.transact_system(|tx| {
            let exists: bool = tx.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM setting WHERE key = ?1)",
                [&key],
                |r| r.get(0),
            )?;
            if exists {
                tx.update(
                    Table::Setting,
                    &[text(key.clone())],
                    &[("value", text(json.clone()))],
                )?;
            } else {
                tx.insert(
                    Table::Setting,
                    &[("key", text(key.clone())), ("value", text(json.clone()))],
                )?;
            }
            Ok(())
        })
    }

    pub fn app_settings(&self) -> Result<AppSettings> {
        self.setting(APP_KEY)
    }

    /// How lists look (shared by the app and assistants).
    pub fn display_settings(&self) -> Result<DisplaySettings> {
        self.setting(DISPLAY_KEY)
    }

    pub fn set_display_settings(&mut self, s: DisplaySettings) -> Result<DisplaySettings> {
        self.set_setting(DISPLAY_KEY, &s)?;
        Ok(s)
    }

    pub fn set_app_settings(&mut self, s: AppSettings) -> Result<AppSettings> {
        self.set_setting(APP_KEY, &s)?;
        Ok(s)
    }

    pub fn notify_settings(&self) -> Result<NotifySettings> {
        self.setting(NOTIFY_KEY)
    }

    pub fn set_notify_settings(&mut self, s: NotifySettings) -> Result<NotifySettings> {
        if !(0..=24 * 60).contains(&s.meeting_lead_min) || !(0..=24 * 60).contains(&s.task_lead_min)
        {
            return Err(invalid("reminder lead must be between 0 and 1440 minutes"));
        }
        let clean = NotifySettings {
            digest_time: clean_time(&s.digest_time)?,
            ..s
        };
        self.set_setting(NOTIFY_KEY, &clean)?;
        Ok(clean)
    }

    fn already_fired(&self, key: &str) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM notification_log WHERE key = ?1)",
            [key],
            |r| r.get(0),
        )?)
    }

    /// Reminders that are due at `now` and haven't fired yet.
    pub fn pending_notifications(&self, now: i64) -> Result<Vec<Notification>> {
        let s = self.notify_settings()?;
        if !s.enabled {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let (tz, _) = local_zone();
        let today = jiff::Timestamp::from_millisecond(now)
            .map(|t| t.to_zoned(tz.clone()).date())
            .map_err(|e| invalid(e.to_string()))?;

        // Meetings: from `lead` before the start until 5 minutes after it.
        let lead = s.meeting_lead_min * MINUTE;
        for o in self.occurrences(now - 10 * MINUTE, now + lead + MINUTE)? {
            if o.kind != EventKind::Meeting || o.status == EventStatus::Cancelled || o.all_day {
                continue;
            }
            if now < o.start_at - lead || now > o.start_at + 5 * MINUTE {
                continue;
            }
            let key = format!("ev:{}:{}", o.event_id, o.original_start);
            if self.already_fired(&key)? {
                continue;
            }
            let mins = (o.start_at - now + MINUTE - 1) / MINUTE;
            let title = if mins <= 0 {
                format!("Now: {}", o.title)
            } else {
                format!("In {mins} min: {}", o.title)
            };
            let mut body = format!("{}–{}", fmt_hm(o.start_at), fmt_hm(o.end_at));
            if let Some(c) = &o.client_name {
                body.push_str(&format!(" · {c}"));
            }
            if !o.location.is_empty() {
                body.push_str(&format!(" · {}", o.location));
            }
            out.push(Notification {
                key,
                kind: NotificationKind::Meeting,
                title,
                body,
                at: o.start_at,
                url: (!o.meeting_url.is_empty()).then(|| o.meeting_url.clone()),
                task_id: None,
                event_id: Some(o.event_id),
            });
        }

        // Timed tasks due today: from `lead` before until an hour after.
        let page = self.list_tasks(&TaskQuery {
            categories: vec![Category::Todo, Category::Active],
            due_from: Some(today.to_string()),
            due_to: Some(today.to_string()),
            limit: 500,
            ..Default::default()
        })?;
        for t in page.rows {
            let Some(time) = t.due_time.as_deref().and_then(|s| s.parse::<Time>().ok()) else {
                continue;
            };
            let Some(due) = local_ms(today, time) else {
                continue;
            };
            if now < due - s.task_lead_min * MINUTE || now > due + 60 * MINUTE {
                continue;
            }
            let key = format!(
                "task:{}:{}T{}",
                t.id,
                today,
                t.due_time.as_deref().unwrap_or("")
            );
            if self.already_fired(&key)? {
                continue;
            }
            let place = match &t.client_name {
                Some(c) => format!("{c} / {}", t.project_name),
                None => t.project_name.clone(),
            };
            out.push(Notification {
                key,
                kind: NotificationKind::Task,
                title: format!("Due {}: {}", fmt_hm(due), t.title),
                body: place,
                at: due,
                url: None,
                task_id: Some(t.id),
                event_id: None,
            });
        }

        // Morning digest, within four hours of the chosen time.
        if s.digest
            && let Some(at) = s
                .digest_time
                .parse::<Time>()
                .ok()
                .and_then(|t| local_ms(today, t))
            && now >= at
            && now < at + 4 * 60 * MINUTE
        {
            let key = format!("digest:{today}");
            if !self.already_fired(&key)? {
                let counts = self.sidebar_counts(today)?;
                let day_end =
                    local_ms(today.tomorrow().unwrap_or(today), Time::midnight()).unwrap_or(now);
                let meetings: Vec<_> = self
                    .occurrences(now, day_end.max(now + 1))?
                    .into_iter()
                    .filter(|o| {
                        o.kind == EventKind::Meeting
                            && o.status != EventStatus::Cancelled
                            && !o.all_day
                    })
                    .collect();
                let mut parts = Vec::new();
                let due_today = counts.today - counts.overdue;
                if due_today > 0 {
                    parts.push(format!("{due_today} due today"));
                }
                if counts.overdue > 0 {
                    parts.push(format!("{} overdue", counts.overdue));
                }
                match meetings.first() {
                    Some(m) => parts.push(format!(
                        "{} meeting{}, first at {}",
                        meetings.len(),
                        if meetings.len() == 1 { "" } else { "s" },
                        fmt_hm(m.start_at)
                    )),
                    None => parts.push("no meetings".into()),
                }
                if due_today > 0 || counts.overdue > 0 || !meetings.is_empty() {
                    out.push(Notification {
                        key,
                        kind: NotificationKind::Digest,
                        title: "Your day".into(),
                        body: parts.join(" · "),
                        at,
                        url: None,
                        task_id: None,
                        event_id: None,
                    });
                }
            }
        }
        out.sort_by_key(|n| n.at);
        Ok(out)
    }

    /// Records that these reminders were shown; prunes entries older than 30 days.
    pub fn mark_notified(&mut self, keys: &[String], now: i64) -> Result<()> {
        let tx = self.conn_mut().transaction()?;
        for key in keys {
            tx.execute(
                "INSERT OR IGNORE INTO notification_log (key, fired_at) VALUES (?1, ?2)",
                params![key, now],
            )?;
        }
        tx.execute(
            "DELETE FROM notification_log WHERE fired_at < ?1",
            [now - 30 * 24 * 60 * MINUTE],
        )?;
        tx.commit()?;
        Ok(())
    }
}
