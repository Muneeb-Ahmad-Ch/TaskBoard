//! Time tracking: log time, run a timer, list and fix entries.

use serde_json::json;
use taskboard_core::{Db, NewTimeEntry, TimeEntryPatch};

use super::{
    Args, ID, RANGE_FROM, RANGE_TO, STR, Tool, ToolResult, err, id_arg, iso, opt_id, parse_when,
    str_arg, window,
};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "log_time",
        description: "Logs worked time on a task, project or group between two local date-times.",
        fields: &[
            ("started", super::WHEN, true),
            ("ended", super::WHEN, true),
            ("task_id", ID, false),
            ("project_id", ID, false),
            ("client_id", ID, false),
            ("note", STR, false),
        ],
    },
    Tool {
        name: "start_timer",
        description: "Starts the timer now on a task, project or group (a running timer is stopped first).",
        fields: &[
            ("task_id", ID, false),
            ("project_id", ID, false),
            ("client_id", ID, false),
            ("note", STR, false),
        ],
    },
    Tool {
        name: "stop_timer",
        description: "Stops the running timer, if any.",
        fields: &[],
    },
    Tool {
        name: "get_timer",
        description: "The running timer: what it is on, when it started and minutes so far.",
        fields: &[],
    },
    Tool {
        name: "list_time",
        description: "Logged time entries (and meetings that count as work time) between two local dates or date-times (default: today and the next 7 days; at most 366).",
        fields: &[("from", RANGE_FROM, false), ("to", RANGE_TO, false)],
    },
    Tool {
        name: "update_time_entry",
        description: "Fixes a time entry's start, end or note.",
        fields: &[
            ("id", ID, true),
            ("started", super::WHEN, false),
            ("ended", super::WHEN, false),
            ("note", STR, false),
        ],
    },
    Tool {
        name: "delete_time_entry",
        description: "Deletes a time entry (undoable).",
        fields: &[("id", ID, true)],
    },
];

pub(super) fn call(db: &mut Db, name: &str, args: &Args) -> ToolResult {
    match name {
        "log_time" => {
            let started = parse_when("started", str_arg(args, "started").unwrap_or(""))?;
            let ended = parse_when("ended", str_arg(args, "ended").unwrap_or(""))?;
            if ended <= started {
                return Err("ended must be after started".to_owned());
            }
            let id = db
                .log_time(entry(args, started, Some(ended))?)
                .map_err(err)?;
            Ok(json!({ "entry_id": id, "minutes": (ended - started) / 60_000 }))
        }
        "start_timer" => {
            let now = taskboard_core::clock::now_ms();
            let id = db.log_time(entry(args, now, None)?).map_err(err)?;
            Ok(json!({ "entry_id": id, "started": iso(now) }))
        }
        "stop_timer" => Ok(json!({ "stopped": db.stop_timer().map_err(err)? })),
        "get_timer" => match db.running_timer().map_err(err)? {
            None => Ok(json!({ "running": false })),
            Some(t) => {
                let now = taskboard_core::clock::now_ms();
                Ok(json!({
                    "running": true, "entry_id": t.entry_id, "title": t.title, "task_id": t.task_id,
                    "project_name": t.project_name, "group_name": t.client_name,
                    "started": iso(t.started_at), "minutes": (now - t.started_at).max(0) / 60_000, "note": t.note,
                }))
            }
        },
        "list_time" => {
            let (from, to) = window(args, 366)?;
            let now = taskboard_core::clock::now_ms();
            let list = db.logged_time(from, to).map_err(err)?;
            Ok(json!({ "entries": list.iter().map(|t| {
                let end = t.ended_at.unwrap_or(now);
                json!({
                    "entry_id": t.entry_id, "event_id": t.event_id, "task_id": t.task_id, "title": t.title,
                    "project_name": t.project_name, "group_name": t.client_name, "started": iso(t.started_at),
                    "ended": t.ended_at.map(iso), "running": t.ended_at.is_none(),
                    "minutes": (end - t.started_at).max(0) / 60_000, "note": t.note,
                })
            }).collect::<Vec<_>>() }))
        }
        "update_time_entry" => {
            let id = id_arg(args, "id")?;
            let when = |k: &str| str_arg(args, k).map(|s| parse_when(k, s)).transpose();
            db.update_time_entry(
                id,
                TimeEntryPatch {
                    started_at: when("started")?,
                    ended_at: when("ended")?,
                    note: str_arg(args, "note").map(str::to_owned),
                },
            )
            .map_err(err)?;
            Ok(json!({ "updated": true }))
        }
        "delete_time_entry" => {
            db.delete_time_entry(id_arg(args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

fn entry(args: &Args, started: i64, ended: Option<i64>) -> Result<NewTimeEntry, String> {
    Ok(NewTimeEntry {
        task_id: opt_id(args, "task_id")?,
        project_id: opt_id(args, "project_id")?,
        client_id: opt_id(args, "client_id")?,
        started_at: started,
        ended_at: ended,
        note: str_arg(args, "note").map(str::to_owned),
    })
}
