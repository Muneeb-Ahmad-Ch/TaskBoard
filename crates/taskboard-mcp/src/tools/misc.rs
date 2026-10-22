//! Everything else: dashboard, undo / redo, saved views, reminders, settings,
//! backups.
//!
//! Deliberately absent: the AI access switch (an assistant must never turn
//! its own access on or off), restoring backups and importing, which replace
//! data wholesale.

use serde_json::{Map, Value, json};
use taskboard_core::{Db, NotifySettings, SavedViewPatch};

use super::{Args, BOOL, ID, STR, Tool, ToolResult, err, id_arg, str_arg, to_value, today};

const SCOPE: &str =
    r#"{"type":"string","enum":["today","upcoming","inbox","all","section","group","project"]}"#;
const VIEW: &str = r#"{"type":"string","enum":["table","board","calendar"]}"#;
const SORT: &str = r#"{"type":"string","enum":["manual","due","priority","created","updated","title","status","project","progress","estimate","tags"],"description":"created = when added; status = board column order; tags = first tag"}"#;
const BOARD_SORT: &str = r#"{"type":"string","enum":["manual","due","priority","created","updated","title","progress","estimate","project"],"description":"Card order in board columns; manual = the order cards were dragged into"}"#;
const NAMES: &str = r#"{"type":"array","items":{"type":"string"}}"#;
const PRIORITIES: &str = r#"{"type":"array","items":{"type":"string","enum":["none","low","med","high","urgent"]},"description":"Filter: any of these priorities"}"#;
const DUE_FILTER: &str = r#"{"type":["string","null"],"enum":["overdue","today","week","none","dated",null],"description":"Filter: overdue, due today, due in the next 7 days, no due date, or has one"}"#;
const ADDED_FILTER: &str = r#"{"type":["string","null"],"enum":["today","week","month",null],"description":"Filter: added today, in the last 7 days or the last 30 days"}"#;
const GROUP_BY: &str = r#"{"type":"string","enum":["none","status","priority","project","group","due","added"],"description":"Table grouping"}"#;
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
        description: "Saves a view for the sidebar: which list (scope, plus scope_id for a section, group or project), how it is shown, sorted and grouped, an optional search, and filters (priorities, statuses, tags, project_ids, due, added) like the app's Filter menu.",
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
            ("priorities", PRIORITIES, false),
            ("statuses", NAMES, false),
            ("tags", NAMES, false),
            ("project_ids", NAMES, false),
            ("due", DUE_FILTER, false),
            ("added", ADDED_FILTER, false),
            ("group_by", GROUP_BY, false),
            ("board_sort", BOARD_SORT, false),
            ("board_descending", BOOL, false),
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
            ("priorities", PRIORITIES, false),
            ("statuses", NAMES, false),
            ("tags", NAMES, false),
            ("project_ids", NAMES, false),
            ("due", DUE_FILTER, false),
            ("added", ADDED_FILTER, false),
            ("group_by", GROUP_BY, false),
            ("board_sort", BOARD_SORT, false),
            ("board_descending", BOOL, false),
        ],
    },
    Tool {
        name: "delete_saved_view",
        description: "Deletes a saved view (the tasks it showed are untouched).",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "get_settings",
        description: "Settings an assistant may change: hide_repeated_context (in a group's or project's list, leave out the name every row shares), start_with_windows, keep_running_in_tray. The AI access switch exists only in the app.",
        fields: &[],
    },
    Tool {
        name: "update_settings",
        description: "Changes any of hide_repeated_context, start_with_windows, keep_running_in_tray (only the fields given). The AI access switch, restoring backups and importing are only in the app.",
        fields: &[
            ("hide_repeated_context", BOOL, false),
            ("start_with_windows", BOOL, false),
            ("keep_running_in_tray", BOOL, false),
        ],
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
                "priorities",
                "statuses",
                "tags",
                "project_ids",
                "due",
                "added",
                "group_by",
                "board_sort",
                "board_descending",
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
        "get_settings" => settings_json(db),
        "update_settings" => {
            let flag = |k: &str| args.get(k).and_then(Value::as_bool);
            let mut display = db.display_settings().map_err(err)?;
            if let Some(v) = flag("hide_repeated_context") {
                display.hide_repeated_context = v;
                db.set_display_settings(display).map_err(err)?;
            }
            // The AI access switch is read and written back unchanged.
            let mut app = db.app_settings().map_err(err)?;
            let start = flag("start_with_windows");
            let tray = flag("keep_running_in_tray");
            if start.is_some() || tray.is_some() {
                app.autostart = start.unwrap_or(app.autostart);
                app.close_to_tray = tray.unwrap_or(app.close_to_tray);
                db.set_app_settings(app).map_err(err)?;
            }
            settings_json(db)
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
    if let Some(g) = str_arg(args, "group_by") {
        let g = match g {
            "group" => "client",
            "none" | "status" | "priority" | "project" | "due" | "added" => g,
            other => return Err(format!("unknown group_by {other:?}")),
        };
        base.insert("groupBy".into(), json!(g));
    }
    base.entry("groupBy").or_insert(json!("none"));
    if let Some(v) = str_arg(args, "board_sort") {
        base.insert("boardSort".into(), json!(v));
    }
    base.entry("boardSort").or_insert(json!("manual"));
    if let Some(v) = args.get("board_descending").and_then(Value::as_bool) {
        base.insert("boardDescending".into(), json!(v));
    }
    base.entry("boardDescending").or_insert(json!(false));
    let mut filter = base
        .get("filter")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    view_filter(&mut filter, args)?;
    base.insert("filter".into(), Value::Object(filter));
    Ok(Value::Object(base))
}

/// The Filter menu part of a view spec (keys as the app stores them).
fn view_filter(filter: &mut Map<String, Value>, args: &Args) -> Result<(), String> {
    let names = |key: &str, strip_hash: bool| -> Result<Option<Vec<String>>, String> {
        match args.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(Vec::new())),
            Some(Value::Array(list)) => list
                .iter()
                .map(|v| {
                    let s = v
                        .as_str()
                        .ok_or_else(|| format!("{key} must be a list of names"))?;
                    let s = if strip_hash {
                        s.trim().trim_start_matches('#')
                    } else {
                        s
                    }
                    .trim();
                    Ok(s.to_owned())
                })
                .filter(|r| !matches!(r, Ok(s) if s.is_empty()))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(format!("{key} must be a list")),
        }
    };
    if let Some(list) = names("priorities", false)? {
        if let Some(bad) = list
            .iter()
            .find(|p| !["none", "low", "med", "high", "urgent"].contains(&p.as_str()))
        {
            return Err(format!(
                "priorities must be none, low, med, high or urgent, got {bad:?}"
            ));
        }
        filter.insert("priorities".into(), json!(list));
    }
    if let Some(list) = names("statuses", false)? {
        filter.insert("statuses".into(), json!(list));
    }
    if let Some(list) = names("tags", true)? {
        filter.insert("tags".into(), json!(list));
    }
    if let Some(list) = names("project_ids", false)? {
        let mut ids = Vec::with_capacity(list.len());
        for id in list {
            ids.push(super::parse_id("project_ids", &json!(id))?);
        }
        filter.insert("projects".into(), json!(ids));
    }
    for (key, allowed) in [
        ("due", &["overdue", "today", "week", "none", "dated"][..]),
        ("added", &["today", "week", "month"][..]),
    ] {
        match args.get(key) {
            None => {}
            Some(Value::Null) => {
                filter.insert(key.into(), Value::Null);
            }
            Some(Value::String(v)) if allowed.contains(&v.as_str()) => {
                filter.insert(key.into(), json!(v));
            }
            Some(other) => {
                return Err(format!(
                    "{key} must be one of {} or null, got {other}",
                    allowed.join(", ")
                ));
            }
        }
    }
    for key in ["priorities", "statuses", "tags", "projects"] {
        filter.entry(key).or_insert(json!([]));
    }
    for key in ["due", "added"] {
        filter.entry(key).or_insert(Value::Null);
    }
    Ok(())
}

fn settings_json(db: &Db) -> ToolResult {
    let display = db.display_settings().map_err(err)?;
    let app = db.app_settings().map_err(err)?;
    Ok(json!({
        "hide_repeated_context": display.hide_repeated_context,
        "start_with_windows": app.autostart,
        "keep_running_in_tray": app.close_to_tray,
    }))
}
