//! Everything else: dashboard, undo / redo, saved views, reminders, backups.
//!
//! Deliberately absent: app settings (start with Windows, tray, AI access —
//! an assistant must not switch its own access on), restoring backups and
//! importing, which replace data wholesale.

use serde_json::{Map, Value, json};
use taskboard_core::{Db, NotifySettings, SavedViewPatch};

use super::{Args, BOOL, ID, STR, Tool, ToolResult, err, id_arg, str_arg, to_value, today};

const SCOPE: &str =
    r#"{"type":"string","enum":["today","upcoming","inbox","all","section","group","project"]}"#;
const VIEW: &str = r#"{"type":"string","enum":["table","board","calendar"]}"#;
const SORT: &str =
    r#"{"type":"string","enum":["manual","due","priority","created","updated","title"]}"#;
const LANES: &str = r#"{"type":"string","enum":["none","group","project","priority"],"description":"Board swimlanes"}"#;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "get_dashboard",
        description: "Progress numbers: done this week, due today, overdue, next 7 days, meetings today, logged time, streak, per-group and per-project summaries.",
        fields: &[],
    },
    Tool {
        name: "undo",
        description: "Undoes the last change made in Taskboard (by anyone) and says what it was.",
        fields: &[],
    },
    Tool {
        name: "redo",
        description: "Redoes the last undone change.",
        fields: &[],
    },
    Tool {
        name: "list_saved_views",
        description: "Saved views (named filters in the sidebar) with what each shows.",
        fields: &[],
    },
    Tool {
        name: "add_saved_view",
        description: "Saves a view for the sidebar: which list (scope, plus scope_id for a section, group or project), how it is shown and sorted, and an optional search.",
        fields: &[
            ("name", STR, true),
            ("scope", SCOPE, true),
            ("scope_id", ID, false),
            ("view", VIEW, false),
            ("search", STR, false),
            ("show_done", BOOL, false),
            ("sort", SORT, false),
            ("descending", BOOL, false),
            ("lanes", LANES, false),
        ],
    },
    Tool {
        name: "update_saved_view",
        description: "Renames a saved view or changes what it shows (only the fields given change).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("scope", SCOPE, false),
            ("scope_id", ID, false),
            ("view", VIEW, false),
            ("search", STR, false),
            ("show_done", BOOL, false),
            ("sort", SORT, false),
            ("descending", BOOL, false),
            ("lanes", LANES, false),
        ],
    },
    Tool {
        name: "delete_saved_view",
        description: "Deletes a saved view (the tasks it showed are untouched).",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "get_reminder_settings",
        description: "Reminder settings: on/off, minutes before meetings and timed tasks, daily digest.",
        fields: &[],
    },
    Tool {
        name: "update_reminder_settings",
        description: "Changes reminder settings (only the fields given).",
        fields: &[
            ("enabled", BOOL, false),
            (
                "meeting_lead_min",
                r#"{"type":"integer","description":"0-1440"}"#,
                false,
            ),
            (
                "task_lead_min",
                r#"{"type":"integer","description":"0-1440"}"#,
                false,
            ),
            (
                "digest",
                r#"{"type":"boolean","description":"A morning summary notification"}"#,
                false,
            ),
            (
                "digest_time",
                r#"{"type":"string","description":"HH:MM"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "backup_now",
        description: "Makes a backup of the whole database now (kept with the daily ones).",
        fields: &[],
    },
];

pub(super) fn call(db: &mut Db, name: &str, args: &Args) -> ToolResult {
    match name {
        "get_dashboard" => to_value(db.dashboard(today()).map_err(err)?),
        "undo" => Ok(json!({ "undid": db.undo().map_err(err)? })),
        "redo" => Ok(json!({ "redid": db.redo().map_err(err)? })),
        "list_saved_views" => {
            let views = db.saved_views().map_err(err)?;
            Ok(json!({ "views": views.iter().map(|v| json!({
                "id": v.id, "name": v.name,
                "spec": serde_json::from_str::<Value>(&v.spec_json).unwrap_or(Value::Null),
            })).collect::<Vec<_>>() }))
        }
        "add_saved_view" => {
            let spec = view_spec(db, Map::new(), args)?;
            let name = str_arg(args, "name").unwrap_or("");
            to_value(db.create_saved_view(name, &spec.to_string()).map_err(err)?)
        }
        "update_saved_view" => {
            let id = id_arg(args, "id")?;
            let current = db.saved_view(id).map_err(err)?;
            let base = serde_json::from_str::<Value>(&current.spec_json)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            let spec_changes = [
                "scope",
                "scope_id",
                "view",
                "search",
                "show_done",
                "sort",
                "descending",
                "lanes",
            ];
            let spec_json = if spec_changes.iter().any(|k| args.contains_key(*k)) {
                Some(view_spec(db, base, args)?.to_string())
            } else {
                None
            };
            to_value(
                db.update_saved_view(
                    id,
                    SavedViewPatch {
                        name: str_arg(args, "name").map(str::to_owned),
                        spec_json,
                    },
                )
                .map_err(err)?,
            )
        }
        "delete_saved_view" => {
            db.delete_saved_view(id_arg(args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "get_reminder_settings" => to_value(db.notify_settings().map_err(err)?),
        "update_reminder_settings" => {
            let mut current = serde_json::to_value(db.notify_settings().map_err(err)?)
                .map_err(|e| e.to_string())?;
            if let Value::Object(m) = &mut current {
                for (k, v) in args {
                    m.insert(k.clone(), v.clone());
                }
            }
            let next: NotifySettings =
                serde_json::from_value(current).map_err(|e| format!("invalid arguments: {e}"))?;
            to_value(db.set_notify_settings(next).map_err(err)?)
        }
        "backup_now" => to_value(db.backup_now("manual").map_err(err)?),
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

/// A saved view's spec as the app stores it (`ViewSpec` in the UI), with the
/// given fields applied over `base`.
fn view_spec(db: &Db, mut base: Map<String, Value>, args: &Args) -> Result<Value, String> {
    let defaults = [
        ("view", json!("table")),
        ("search", json!("")),
        ("showDone", json!(false)),
        ("sort", json!("due")),
        ("descending", json!(false)),
        ("lanes", json!("none")),
    ];
    for (k, v) in defaults {
        base.entry(k).or_insert(v);
    }
    if let Some(scope) = str_arg(args, "scope") {
        let id = args.get("scope_id").filter(|v| !v.is_null());
        let kind = match scope {
            "today" | "upcoming" | "inbox" | "all" => {
                if id.is_some() {
                    return Err(format!("scope {scope} takes no scope_id"));
                }
                base.insert("scope".into(), json!({ "kind": scope }));
                None
            }
            "section" => Some("section"),
            "group" => Some("client"),
            "project" => Some("project"),
            other => return Err(format!("unknown scope {other:?}")),
        };
        if let Some(kind) = kind {
            let raw =
                id.ok_or_else(|| format!("scope {scope} needs scope_id (the {scope}'s id)"))?;
            let sid = super::parse_id("scope_id", raw)?;
            let exists = match kind {
                "section" => db.section(sid).is_ok(),
                "client" => db.client(sid).is_ok(),
                _ => db.project(sid).is_ok(),
            };
            if !exists {
                return Err(format!("no {scope} with scope_id {sid}"));
            }
            base.insert("scope".into(), json!({ "kind": kind, "id": sid }));
        }
    } else if args.contains_key("scope_id") {
        return Err("scope_id needs scope".to_owned());
    }
    if !base.contains_key("scope") {
        base.insert("scope".into(), json!({ "kind": "all" }));
    }
    for (arg, key) in [("view", "view"), ("search", "search"), ("sort", "sort")] {
        if let Some(v) = str_arg(args, arg) {
            base.insert(key.into(), json!(v));
        }
    }
    for (arg, key) in [("show_done", "showDone"), ("descending", "descending")] {
        if let Some(v) = args.get(arg).and_then(Value::as_bool) {
            base.insert(key.into(), json!(v));
        }
    }
    if let Some(l) = str_arg(args, "lanes") {
        base.insert(
            "lanes".into(),
            json!(if l == "group" { "client" } else { l }),
        );
    }
    Ok(Value::Object(base))
}
