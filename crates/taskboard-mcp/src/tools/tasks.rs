//! Tasks and subtasks: find, add, change (one or many), complete, archive,
//! reorder on the board, duplicate, delete and restore.

use serde_json::{Value, json};
use taskboard_core::error::invalid;
use taskboard_core::{Category, Db, Id, MoveTask, NewTask, TaskPatch, TaskQuery, TaskSort};

use super::{
    Args, BOOL, DATE, ID, IDS, INT, NULLABLE_ID, PRIORITY, STATUS_NAME, STR, TAGS, TIME, Tool,
    ToolResult, bool_arg, err, from_args, id_arg, id_or_ids, opt_id, parse_point, str_arg,
    tasks_label, today, without,
};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "list_tasks",
        description: "Tasks matching filters, soonest due first. Defaults to open, top-level, unarchived tasks (subtasks appear under their parent in get_task). Use search for free text, due_from/due_to for a date window, status_name for one board column, completed_since for \"what did I finish today\".",
        fields: &[
            ("project_id", ID, false),
            ("group_id", ID, false),
            ("section_id", ID, false),
            ("search", STR, false),
            ("tag", STR, false),
            (
                "status",
                r#"{"type":"string","enum":["open","done","all"],"description":"open = to do or in progress (default; all when completed_since is given)"}"#,
                false,
            ),
            (
                "status_name",
                r#"{"type":"string","description":"Only tasks in the board column with this name, any case"}"#,
                false,
            ),
            (
                "due_from",
                r#"{"type":"string","description":"YYYY-MM-DD inclusive"}"#,
                false,
            ),
            (
                "due_to",
                r#"{"type":"string","description":"YYYY-MM-DD inclusive"}"#,
                false,
            ),
            (
                "undated",
                r#"{"type":"boolean","description":"Only tasks without a due date"}"#,
                false,
            ),
            (
                "completed_since",
                r#"{"type":"string","description":"Only tasks completed at or after this local date (YYYY-MM-DD) or date-time"}"#,
                false,
            ),
            (
                "parent_id",
                r#"{"type":"string","description":"Subtasks of this task"}"#,
                false,
            ),
            (
                "archived",
                r#"{"type":"string","enum":["exclude","include","only"],"description":"Archived tasks: left out (default), included, or only them"}"#,
                false,
            ),
            (
                "include_archived",
                r#"{"type":"boolean","description":"Same as archived: include"}"#,
                false,
            ),
            (
                "sort",
                r#"{"type":"string","enum":["manual","due","priority","created","updated","title"],"description":"manual = board order"}"#,
                false,
            ),
            ("descending", BOOL, false),
            (
                "limit",
                r#"{"type":"integer","description":"1-500, default 50"}"#,
                false,
            ),
            ("offset", r#"{"type":"integer"}"#, false),
        ],
    },
    Tool {
        name: "get_task",
        description: "One task in full (description, dates, tags, status and the project's other columns, archive state) with its subtasks.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "add_task",
        description: "Adds a task. `text` takes quick-add syntax: \"Send invoice fri 3pm #admin !high ~30m @website every month\" (date, time, #tag, !priority, ~estimate, @project, repeat). Explicit fields (status, due_date, priority, tags…) win over what the text says. Without a project it goes to the Inbox.",
        fields: &[
            ("text", STR, true),
            (
                "project_id",
                r#"{"type":"string","description":"Overrides any @project in the text"}"#,
                false,
            ),
            (
                "parent_id",
                r#"{"type":"string","description":"Make it a subtask of this task"}"#,
                false,
            ),
            ("status", STATUS_NAME, false),
            ("status_id", ID, false),
            (
                "description_md",
                r#"{"type":"string","description":"Notes, Markdown"}"#,
                false,
            ),
            ("priority", PRIORITY, false),
            ("start_date", DATE, false),
            ("due_date", DATE, false),
            ("due_time", TIME, false),
            ("estimate_minutes", INT, false),
            ("tags", TAGS, false),
            (
                "progress",
                r#"{"type":["integer","null"],"description":"0-100"}"#,
                false,
            ),
            (
                "recurrence_rule",
                r#"{"type":["string","null"],"description":"RRULE, e.g. FREQ=WEEKLY;BYDAY=MO"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_task",
        description: "Changes fields of one task (id) or many (ids, one undo step). Only the fields given change; pass null to clear a date, time, estimate, progress or parent. Set the board column with status (its name, e.g. \"In Progress\") or status_id; move a task with project_id (status then names a column of the new project).",
        fields: &[
            ("id", ID, false),
            ("ids", IDS, false),
            ("title", STR, false),
            ("description_md", STR, false),
            ("priority", PRIORITY, false),
            ("project_id", ID, false),
            ("status", STATUS_NAME, false),
            ("status_id", ID, false),
            ("parent_id", NULLABLE_ID, false),
            ("start_date", DATE, false),
            ("due_date", DATE, false),
            ("due_time", TIME, false),
            ("estimate_minutes", INT, false),
            ("tags", TAGS, false),
            (
                "progress",
                r#"{"type":["integer","null"],"description":"0-100; null = from subtasks"}"#,
                false,
            ),
            (
                "recurrence_rule",
                r#"{"type":["string","null"],"description":"RRULE, e.g. FREQ=WEEKLY;BYDAY=MO; null stops repeating"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "complete_task",
        description: "Marks one task (id) or many (ids) done, or not done with done=false. A repeating task schedules its next occurrence.",
        fields: &[
            ("id", ID, false),
            ("ids", IDS, false),
            (
                "done",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_task",
        description: "Deletes one task (id) or many (ids) with their subtasks. Undoable; restore_task brings one back later.",
        fields: &[("id", ID, false), ("ids", IDS, false)],
    },
    Tool {
        name: "archive_task",
        description: "Archives one task (id) or many (ids) with their subtasks: hidden from lists but kept, found again with list_tasks archived=only. archived=false restores them (and an archived parent).",
        fields: &[
            ("id", ID, false),
            ("ids", IDS, false),
            (
                "archived",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "archive_completed",
        description: "Archives every completed (or cancelled) top-level task in a project, group or section (or everywhere when none is given), as one undo step.",
        fields: &[
            ("project_id", ID, false),
            ("group_id", ID, false),
            ("section_id", ID, false),
        ],
    },
    Tool {
        name: "move_task",
        description: "Moves a card on the board: into another column (status / status_id) and/or before or after another task in that column. Without before/after it goes to the end of the column.",
        fields: &[
            ("id", ID, true),
            ("status", STATUS_NAME, false),
            ("status_id", ID, false),
            (
                "before",
                r#"{"type":"string","description":"Task id it should come just before"}"#,
                false,
            ),
            (
                "after",
                r#"{"type":"string","description":"Task id it should come just after"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "duplicate_task",
        description: "Copies a task (not its subtasks) right after the original.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "restore_task",
        description: "Brings back a deleted task and the subtasks deleted with it.",
        fields: &[("id", ID, true)],
    },
];

pub(super) fn call(db: &mut Db, name: &str, args: &Args) -> ToolResult {
    match name {
        "list_tasks" => list_tasks(db, args),
        "get_task" => {
            let task = db.task(id_arg(args, "id")?).map_err(err)?;
            task_json(db, &task)
        }
        "add_task" => add_task(db, args),
        "update_task" => update_task(db, args),
        "complete_task" => {
            let (ids, many) = id_or_ids(args)?;
            let done = bool_arg(args, "done", true);
            if !many {
                let task = db.set_task_done(ids[0], done).map_err(err)?;
                return task_json(db, &task);
            }
            let verb = if done { "Complete" } else { "Reopen" };
            let n = db
                .batch(&tasks_label(verb, ids.len()), |db| {
                    for id in &ids {
                        db.set_task_done(*id, done)?;
                    }
                    Ok(ids.len())
                })
                .map_err(err)?;
            Ok(json!({ "updated": n }))
        }
        "delete_task" => {
            let (ids, many) = id_or_ids(args)?;
            if !many {
                db.delete_task(ids[0]).map_err(err)?;
                return Ok(json!({ "deleted": true }));
            }
            let n = db
                .batch(&tasks_label("Delete", ids.len()), |db| {
                    for id in &ids {
                        db.delete_task(*id)?;
                    }
                    Ok(ids.len())
                })
                .map_err(err)?;
            Ok(json!({ "deleted": n }))
        }
        "archive_task" => {
            let (ids, many) = id_or_ids(args)?;
            let archived = bool_arg(args, "archived", true);
            let key = if archived { "archived" } else { "restored" };
            let changes = |db: &mut Db| -> taskboard_core::Result<usize> {
                let mut n = 0;
                for id in &ids {
                    let before = db.task(*id)?.archived_at.is_some();
                    if before != archived {
                        db.set_task_archived(*id, archived)?;
                        n += 1;
                    }
                }
                Ok(n)
            };
            let n = if many {
                let verb = if archived { "Archive" } else { "Restore" };
                db.batch(&tasks_label(verb, ids.len()), changes)
            } else {
                changes(db)
            }
            .map_err(err)?;
            Ok(json!({ key: n }))
        }
        "archive_completed" => {
            let q = TaskQuery {
                project_id: opt_id(args, "project_id")?,
                client_id: opt_id(args, "group_id")?,
                section_id: opt_id(args, "section_id")?,
                ..Default::default()
            };
            Ok(json!({ "archived": db.archive_completed(&q).map_err(err)? }))
        }
        "move_task" => move_task(db, args),
        "duplicate_task" => {
            let task = db.duplicate_task(id_arg(args, "id")?).map_err(err)?;
            task_json(db, &task)
        }
        "restore_task" => {
            let task = db.restore_task(id_arg(args, "id")?).map_err(err)?;
            task_json(db, &task)
        }
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

// ---------- statuses by name ----------

/// The column of `project` called `name` (any case), or an error naming the
/// project's columns.
fn status_by_name(db: &Db, project: Id, name: &str) -> taskboard_core::Result<Id> {
    let wanted = name.trim();
    let columns = db.statuses(project)?;
    if let Some(s) = columns
        .iter()
        .find(|s| s.name.trim().eq_ignore_ascii_case(wanted))
    {
        return Ok(s.id);
    }
    let project_name = db.project(project).map(|p| p.name).unwrap_or_default();
    let names: Vec<&str> = columns.iter().map(|s| s.name.as_str()).collect();
    Err(invalid(format!(
        "no column \"{wanted}\" in {project_name}; its columns are: {}",
        names.join(", ")
    )))
}

/// The status a patch should set: `status_id`, or `status` by name in the
/// task's (new) project.
fn wanted_status(
    db: &Db,
    task: Id,
    project: Option<Id>,
    name: Option<&str>,
    id: Option<Id>,
) -> taskboard_core::Result<Option<Id>> {
    match (name, id) {
        (Some(_), Some(_)) => Err(invalid("give status or status_id, not both")),
        (None, id) => Ok(id),
        (Some(name), None) => {
            let project = match project {
                Some(p) => p,
                None => db.task(task)?.project_id,
            };
            status_by_name(db, project, name).map(Some)
        }
    }
}

// ---------- tools ----------

fn list_tasks(db: &mut Db, args: &Args) -> ToolResult {
    let completed_from = match str_arg(args, "completed_since") {
        Some(s) => Some(parse_point("completed_since", s, false)?),
        None => None,
    };
    let default_status = if completed_from.is_some() {
        "all"
    } else {
        "open"
    };
    let categories = match str_arg(args, "status").unwrap_or(default_status) {
        "open" => vec![Category::Todo, Category::Active],
        "done" => vec![Category::Done],
        "all" => vec![],
        other => return Err(format!("status must be open, done or all, got {other:?}")),
    };
    let sort = match str_arg(args, "sort").unwrap_or("due") {
        "manual" => TaskSort::Manual,
        "due" => TaskSort::Due,
        "priority" => TaskSort::Priority,
        "created" => TaskSort::Created,
        "updated" => TaskSort::Updated,
        "title" => TaskSort::Title,
        other => return Err(format!("unknown sort {other:?}")),
    };
    let (include_archived, archived_only) = match str_arg(args, "archived") {
        None => (bool_arg(args, "include_archived", false), false),
        Some("exclude") => (false, false),
        Some("include") => (true, false),
        Some("only") => (false, true),
        Some(other) => {
            return Err(format!(
                "archived must be exclude, include or only, got {other:?}"
            ));
        }
    };
    let parent_id = opt_id(args, "parent_id")?;
    let q = TaskQuery {
        project_id: opt_id(args, "project_id")?,
        client_id: opt_id(args, "group_id")?,
        section_id: opt_id(args, "section_id")?,
        search: str_arg(args, "search")
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty()),
        tag: str_arg(args, "tag")
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty()),
        status_name: str_arg(args, "status_name")
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty()),
        categories,
        due_from: str_arg(args, "due_from").map(str::to_owned),
        due_to: str_arg(args, "due_to").map(str::to_owned),
        undated: bool_arg(args, "undated", false),
        completed_from,
        parent_id,
        top_level: parent_id.is_none(),
        include_archived,
        archived_only,
        sort,
        descending: bool_arg(args, "descending", false),
        limit: args
            .get("limit")
            .and_then(Value::as_i64)
            .unwrap_or(50)
            .clamp(1, 500),
        offset: args
            .get("offset")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0),
        ..Default::default()
    };
    let page = db.list_tasks(&q).map_err(err)?;
    Ok(json!({
        "total": page.total,
        "tasks": page.rows.iter().map(row_json).collect::<Vec<_>>(),
    }))
}

pub(super) fn row_json(r: &taskboard_core::TaskRow) -> Value {
    json!({
        "id": r.id, "title": r.title, "status": r.status_name, "category": r.category, "priority": r.priority,
        "project_id": r.project_id, "project_name": r.project_name, "group_name": r.client_name,
        "parent_id": r.parent_id, "parent_title": r.parent_title, "start_date": r.start_date, "due_date": r.due_date,
        "due_time": r.due_time, "estimate_minutes": r.estimate_minutes, "tags": r.tags, "progress": r.progress,
        "subtasks": { "done": r.subtasks_done, "total": r.subtasks_total }, "recurring": r.recurring,
        "completed_at": r.completed_at, "archived_at": r.archived_at,
    })
}

/// A task with the names an assistant needs (status, project, the project's
/// columns) and its subtasks.
fn task_json(db: &mut Db, task: &taskboard_core::Task) -> ToolResult {
    let mut v = serde_json::to_value(task).map_err(|e| e.to_string())?;
    let project = db.project(task.project_id).map_err(err)?;
    let columns = db.statuses(task.project_id).map_err(err)?;
    let status = columns.iter().find(|s| s.id == task.status_id);
    let subs = db
        .list_tasks(&TaskQuery {
            parent_id: Some(task.id),
            top_level: false,
            include_archived: true,
            limit: 500,
            ..Default::default()
        })
        .map_err(err)?;
    if let Value::Object(m) = &mut v {
        m.insert("project_name".into(), json!(project.name));
        m.insert("status".into(), json!(status.map(|s| s.name.clone())));
        m.insert("category".into(), json!(status.map(|s| s.category)));
        m.insert(
            "columns".into(),
            json!(columns.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
        );
        m.insert(
            "subtasks".into(),
            json!(subs.rows.iter().map(row_json).collect::<Vec<_>>()),
        );
        m.remove("sort_key");
    }
    Ok(v)
}

fn add_task(db: &mut Db, args: &Args) -> ToolResult {
    let text = str_arg(args, "text").unwrap_or("");
    let parsed = taskboard_core::quickadd::parse(text, today());
    if parsed.title.trim().is_empty() {
        return Err("text needs a task title".to_owned());
    }
    let explicit: TaskPatch = from_args(without(
        args,
        &["text", "project_id", "parent_id", "status", "status_id"],
    ))?;
    let project_id = match opt_id(args, "project_id")? {
        Some(id) => Some(id),
        None => match &parsed.project_hint {
            Some(hint) => Some(db.resolve_project(hint).map_err(err)?.ok_or_else(|| {
                format!("no project matches \"@{hint}\"; see get_overview for the project names")
            })?),
            None => None,
        },
    };
    let parent_id = opt_id(args, "parent_id")?;
    // A subtask lives in its parent's project.
    let project_id = match (project_id, parent_id) {
        (None, Some(parent)) => Some(db.task(parent).map_err(err)?.project_id),
        (p, _) => p,
    };
    let status_id = match (str_arg(args, "status"), opt_id(args, "status_id")?) {
        (Some(_), Some(_)) => return Err("give status or status_id, not both".to_owned()),
        (None, id) => id,
        (Some(name), None) => {
            let project = match project_id {
                Some(p) => p,
                None => db.inbox_id().map_err(err)?,
            };
            Some(status_by_name(db, project, name).map_err(err)?)
        }
    };
    let task = db
        .create_task(NewTask {
            title: parsed.title,
            project_id,
            parent_id,
            status_id,
            description_md: explicit.description_md,
            priority: explicit.priority.or(parsed.priority),
            start_date: explicit.start_date.flatten(),
            due_date: match explicit.due_date {
                Some(d) => d,
                None => parsed.due_date,
            },
            due_time: match explicit.due_time {
                Some(t) => t,
                None => parsed.due_time,
            },
            estimate_minutes: match explicit.estimate_minutes {
                Some(m) => m,
                None => parsed.estimate_minutes,
            },
            tags: explicit.tags.unwrap_or(parsed.tags),
            progress: explicit.progress.flatten(),
            recurrence_rule: match explicit.recurrence_rule {
                Some(r) => r,
                None => parsed.recurrence_rule,
            },
        })
        .map_err(err)?;
    task_json(db, &task)
}

fn update_task(db: &mut Db, args: &Args) -> ToolResult {
    let (ids, many) = id_or_ids(args)?;
    let status_name = str_arg(args, "status").map(str::to_owned);
    let patch: TaskPatch = from_args(without(args, &["id", "ids", "status"]))?;
    let apply = |db: &mut Db, id: Id| -> taskboard_core::Result<taskboard_core::Task> {
        let mut p = patch.clone();
        p.status_id = wanted_status(db, id, p.project_id, status_name.as_deref(), p.status_id)?;
        db.update_task(id, p)
    };
    if !many {
        let task = apply(db, ids[0]).map_err(err)?;
        return task_json(db, &task);
    }
    let n = db
        .batch(&tasks_label("Edit", ids.len()), |db| {
            for id in &ids {
                apply(db, *id)?;
            }
            Ok(ids.len())
        })
        .map_err(err)?;
    Ok(json!({ "updated": n }))
}

fn move_task(db: &mut Db, args: &Args) -> ToolResult {
    let id = id_arg(args, "id")?;
    let task = db.task(id).map_err(err)?;
    let status = wanted_status(
        db,
        id,
        None,
        str_arg(args, "status"),
        opt_id(args, "status_id")?,
    )
    .map_err(err)?
    .unwrap_or(task.status_id);
    let (before, after) = (opt_id(args, "before")?, opt_id(args, "after")?);
    if before.is_some() && after.is_some() {
        return Err("give before or after, not both".to_owned());
    }
    let (mut above, mut below) = (None, None);
    if before.is_some() || after.is_some() {
        // The target column in board order, without the card being moved.
        let rows = db
            .list_tasks(&TaskQuery {
                project_id: Some(task.project_id),
                parent_id: task.parent_id,
                top_level: task.parent_id.is_none(),
                sort: TaskSort::Manual,
                limit: 1000,
                ..Default::default()
            })
            .map_err(err)?
            .rows;
        let column: Vec<Id> = rows
            .iter()
            .filter(|r| r.status_id == status && r.id != id)
            .map(|r| r.id)
            .collect();
        let anchor = before.or(after).unwrap_or(id);
        let at = column
            .iter()
            .position(|t| *t == anchor)
            .ok_or("that task is not in the target column (same project and status)")?;
        if before.is_some() {
            above = at.checked_sub(1).and_then(|i| column.get(i)).copied();
            below = Some(anchor);
        } else {
            above = Some(anchor);
            below = column.get(at + 1).copied();
        }
    }
    let moved = db
        .move_task(
            id,
            MoveTask {
                status_id: (status != task.status_id).then_some(status),
                above,
                below,
            },
        )
        .map_err(err)?;
    task_json(db, &moved)
}
