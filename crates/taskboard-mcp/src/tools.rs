//! The tools an assistant can call, their JSON schemas, and how each maps
//! onto the core. Arguments are checked against the schema's property list
//! first, so a misspelled field is an error the assistant can act on rather
//! than something silently ignored.

use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use taskboard_core::{
    Category, ClientPatch, Db, EventKind, EventPatch, Id, NewClient, NewEvent, NewProject,
    NewSection, NewTask, NewTimeEntry, ProjectPatch, SectionDelete, SectionPatch, TaskPatch,
    TaskQuery, TaskSort,
};

type ToolResult = Result<Value, String>;

struct Tool {
    name: &'static str,
    description: &'static str,
    /// (field, JSON schema, required)
    fields: &'static [(&'static str, &'static str, bool)],
}

// Schemas as JSON text: short to write, and `definitions()` parses them once.
const ID: &str = r#"{"type":"string"}"#;
const STR: &str = r#"{"type":"string"}"#;
const DATE: &str = r#"{"type":["string","null"],"description":"YYYY-MM-DD; null clears it"}"#;
const TIME: &str = r#"{"type":["string","null"],"description":"HH:MM, 24-hour; null clears it"}"#;
const WHEN: &str = r#"{"type":"string","description":"Local date-time, YYYY-MM-DDTHH:MM"}"#;
const PRIORITY: &str = r#"{"type":"string","enum":["none","low","med","high","urgent"]}"#;
const COLOR: &str = r#"{"type":"string","description":"\"gray\" or a hue 0-360, e.g. \"262\""}"#;
const BOOL: &str = r#"{"type":"boolean"}"#;
const INT: &str = r#"{"type":["integer","null"]}"#;
const TAGS: &str = r#"{"type":"array","items":{"type":"string"}}"#;
const NULLABLE_ID: &str = r#"{"type":["string","null"]}"#;
const NULLABLE_STR: &str = r#"{"type":["string","null"]}"#;

const TOOLS: &[Tool] = &[
    Tool {
        name: "get_overview",
        description: "The map of the workspace: sections, groups, projects (with task counts), today's date and the Today / Inbox / overdue counts. Call this first to learn the ids other tools need.",
        fields: &[],
    },
    Tool {
        name: "list_tasks",
        description: "Tasks matching filters, newest due first. Defaults to open, top-level tasks (subtasks appear under their parent in get_task). Use search for free text, due_from/due_to for a date window.",
        fields: &[
            ("project_id", ID, false),
            ("group_id", ID, false),
            ("section_id", ID, false),
            ("search", STR, false),
            ("tag", STR, false),
            (
                "status",
                r#"{"type":"string","enum":["open","done","all"],"description":"open = to do or in progress (default)"}"#,
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
                "parent_id",
                r#"{"type":"string","description":"Subtasks of this task"}"#,
                false,
            ),
            ("include_archived", BOOL, false),
            (
                "sort",
                r#"{"type":"string","enum":["manual","due","priority","created","updated","title"]}"#,
                false,
            ),
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
        description: "One task in full (description, dates, tags, status, project) with its subtasks.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "add_task",
        description: "Adds a task. `text` takes quick-add syntax: \"Send invoice fri 3pm #admin !high ~30m @website every month\" (date, time, #tag, !priority, ~estimate, @project, repeat). Without a project it goes to the Inbox.",
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
            (
                "description_md",
                r#"{"type":"string","description":"Notes, Markdown"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_task",
        description: "Changes fields of a task. Only the fields given change; pass null to clear a date, time, estimate, progress or parent. Move a task with project_id.",
        fields: &[
            ("id", ID, true),
            ("title", STR, false),
            ("description_md", STR, false),
            ("priority", PRIORITY, false),
            ("project_id", ID, false),
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
        description: "Marks a task done (or not done with done=false). A repeating task schedules its next occurrence.",
        fields: &[
            ("id", ID, true),
            (
                "done",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_task",
        description: "Deletes a task and its subtasks (undoable).",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "add_project",
        description: "Adds a project inside a group (client_id) or directly in a section (section_id). With neither it goes to the first section.",
        fields: &[
            ("name", STR, true),
            (
                "client_id",
                r#"{"type":"string","description":"Group to put it in"}"#,
                false,
            ),
            ("section_id", ID, false),
            ("color", COLOR, false),
            (
                "code",
                r#"{"type":"string","description":"Job / PO code"}"#,
                false,
            ),
            (
                "deadline",
                r#"{"type":"string","description":"YYYY-MM-DD"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_project",
        description: "Changes a project: name, color, code, state (active / on_hold / completed), dates, or where it lives (client_id, null = no group; section_id for groupless projects).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("color", COLOR, false),
            ("client_id", NULLABLE_ID, false),
            ("section_id", ID, false),
            ("code", NULLABLE_STR, false),
            (
                "state",
                r#"{"type":"string","enum":["active","on_hold","completed"]}"#,
                false,
            ),
            ("start_date", DATE, false),
            ("deadline", DATE, false),
        ],
    },
    Tool {
        name: "archive_project",
        description: "Hides a project and its tasks from everyday views (archived=false brings it back).",
        fields: &[
            ("id", ID, true),
            (
                "archived",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_project",
        description: "Deletes a project and all its tasks (undoable). The Inbox cannot be deleted.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "add_group",
        description: "Adds a group (a client, team, person… whatever the section calls its items) to a section.",
        fields: &[
            ("name", STR, true),
            (
                "section_id",
                r#"{"type":"string","description":"Default: the first section"}"#,
                false,
            ),
            ("color", COLOR, false),
            ("contact", STR, false),
            ("notes_md", STR, false),
            (
                "time_zone",
                r#"{"type":"string","description":"IANA zone, e.g. Europe/London (used to find meeting slots)"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_group",
        description: "Changes a group: name, color, contact, notes, time zone and working hours, or moves it to another section (section_id, its projects go along).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("section_id", ID, false),
            ("color", COLOR, false),
            ("contact", STR, false),
            ("notes_md", STR, false),
            ("time_zone", NULLABLE_STR, false),
            ("work_start", TIME, false),
            ("work_end", TIME, false),
            ("meetings_count_as_time", BOOL, false),
        ],
    },
    Tool {
        name: "archive_group",
        description: "Archives a group with its projects (archived=false restores them).",
        fields: &[
            ("id", ID, true),
            (
                "archived",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_group",
        description: "Deletes an empty group. Move or delete its projects first.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "add_section",
        description: "Adds a sidebar section (e.g. \"Teams\"). item_name is what one group in it is called (\"team\").",
        fields: &[
            ("name", STR, true),
            ("item_name", STR, false),
            ("color", COLOR, false),
        ],
    },
    Tool {
        name: "update_section",
        description: "Renames or recolors a section, or changes what its groups are called.",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("item_name", STR, false),
            ("color", COLOR, false),
        ],
    },
    Tool {
        name: "delete_section",
        description: "Deletes a section. Its groups and projects move to move_to (another section id); without move_to everything in it is deleted. The last section cannot be deleted.",
        fields: &[("id", ID, true), ("move_to", ID, false)],
    },
    Tool {
        name: "list_events",
        description: "Meetings and time blocks between two local dates or date-times (default: today and the next 7 days), with the group / project they belong to.",
        fields: &[
            (
                "from",
                r#"{"type":"string","description":"YYYY-MM-DD or YYYY-MM-DDTHH:MM, inclusive"}"#,
                false,
            ),
            (
                "to",
                r#"{"type":"string","description":"YYYY-MM-DD (whole day included) or YYYY-MM-DDTHH:MM, exclusive"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "add_event",
        description: "Adds a meeting or a time block (focus time) at local times.",
        fields: &[
            (
                "kind",
                r#"{"type":"string","enum":["meeting","block"]}"#,
                true,
            ),
            ("title", STR, true),
            ("start", WHEN, true),
            ("end", WHEN, true),
            ("client_id", ID, false),
            ("project_id", ID, false),
            ("task_id", ID, false),
            ("location", STR, false),
            ("meeting_url", STR, false),
            (
                "attendees",
                r#"{"type":"string","description":"Free text, e.g. names or emails"}"#,
                false,
            ),
            ("notes_md", STR, false),
            (
                "counts_as_time",
                r#"{"type":"boolean","description":"Count it as logged work time"}"#,
                false,
            ),
            (
                "recurrence_rule",
                r#"{"type":"string","description":"RRULE, e.g. FREQ=WEEKLY;BYDAY=TU"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_event",
        description: "Changes a meeting or time block (the whole series for a repeating one).",
        fields: &[
            ("id", ID, true),
            ("title", STR, false),
            (
                "kind",
                r#"{"type":"string","enum":["meeting","block"]}"#,
                false,
            ),
            ("start", WHEN, false),
            ("end", WHEN, false),
            ("client_id", NULLABLE_ID, false),
            ("project_id", NULLABLE_ID, false),
            ("task_id", NULLABLE_ID, false),
            ("location", STR, false),
            ("meeting_url", STR, false),
            ("attendees", STR, false),
            ("notes_md", STR, false),
            (
                "status",
                r#"{"type":"string","enum":["confirmed","tentative","cancelled"]}"#,
                false,
            ),
            ("counts_as_time", BOOL, false),
            ("recurrence_rule", NULLABLE_STR, false),
        ],
    },
    Tool {
        name: "delete_event",
        description: "Deletes a meeting or time block (undoable).",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "log_time",
        description: "Logs worked time on a task, project or group between two local date-times.",
        fields: &[
            ("started", WHEN, true),
            ("ended", WHEN, true),
            ("task_id", ID, false),
            ("project_id", ID, false),
            ("client_id", ID, false),
            ("note", STR, false),
        ],
    },
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
];

/// Tool definitions as MCP wants them.
pub fn definitions() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|t| {
            let mut props = Map::new();
            let mut required = Vec::new();
            for (name, schema, req) in t.fields {
                let parsed: Value = serde_json::from_str(schema).unwrap_or_else(|_| json!({}));
                props.insert((*name).to_owned(), parsed);
                if *req {
                    required.push(Value::String((*name).to_owned()));
                }
            }
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": { "type": "object", "properties": props, "required": required, "additionalProperties": false },
            })
        })
        .collect()
}

/// Runs a tool; errors are messages for the assistant.
pub fn call(db: &mut Db, name: &str, args: Value) -> ToolResult {
    let tool = TOOLS.iter().find(|t| t.name == name).ok_or_else(|| {
        format!("unknown tool \"{name}\"; call tools/list for the available ones")
    })?;
    let args = check_args(tool, args)?;
    match name {
        "get_overview" => overview(db),
        "list_tasks" => list_tasks(db, &args),
        "get_task" => get_task(db, &args),
        "add_task" => add_task(db, &args),
        "update_task" => {
            let id = id_arg(&args, "id")?;
            let patch: TaskPatch = from_args(without(&args, "id"))?;
            let task = db.update_task(id, patch).map_err(err)?;
            task_json(db, &task)
        }
        "complete_task" => {
            let done = args.get("done").and_then(Value::as_bool).unwrap_or(true);
            let task = db.set_task_done(id_arg(&args, "id")?, done).map_err(err)?;
            task_json(db, &task)
        }
        "delete_task" => {
            db.delete_task(id_arg(&args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "add_project" => {
            let input: NewProject = from_args(args)?;
            to_value(db.create_project(input).map_err(err)?)
        }
        "update_project" => {
            let id = id_arg(&args, "id")?;
            let patch: ProjectPatch = from_args(without(&args, "id"))?;
            to_value(db.update_project(id, patch).map_err(err)?)
        }
        "archive_project" => {
            let archived = args
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            db.set_project_archived(id_arg(&args, "id")?, archived)
                .map_err(err)?;
            Ok(json!({ "archived": archived }))
        }
        "delete_project" => {
            db.delete_project(id_arg(&args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "add_group" => {
            let input: NewClient = from_args(args)?;
            to_value(db.create_client(input).map_err(err)?)
        }
        "update_group" => {
            let id = id_arg(&args, "id")?;
            let patch: ClientPatch = from_args(without(&args, "id"))?;
            to_value(db.update_client(id, patch).map_err(err)?)
        }
        "archive_group" => {
            let archived = args
                .get("archived")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            db.set_client_archived(id_arg(&args, "id")?, archived)
                .map_err(err)?;
            Ok(json!({ "archived": archived }))
        }
        "delete_group" => {
            db.delete_client(id_arg(&args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "add_section" => {
            let input: NewSection = from_args(args)?;
            to_value(db.create_section(input).map_err(err)?)
        }
        "update_section" => {
            let id = id_arg(&args, "id")?;
            let patch: SectionPatch = from_args(without(&args, "id"))?;
            to_value(db.update_section(id, patch).map_err(err)?)
        }
        "delete_section" => {
            let how = match args.get("move_to") {
                Some(v) if !v.is_null() => SectionDelete::MoveTo {
                    section_id: id_arg(&args, "move_to")?,
                },
                _ => SectionDelete::Everything,
            };
            db.delete_section(id_arg(&args, "id")?, how).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "list_events" => list_events(db, &args),
        "add_event" => add_event(db, args),
        "update_event" => update_event(db, args),
        "delete_event" => {
            db.delete_event(id_arg(&args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "log_time" => log_time(db, &args),
        "get_dashboard" => to_value(db.dashboard(today()).map_err(err)?),
        "undo" => Ok(json!({ "undid": db.undo().map_err(err)? })),
        "redo" => Ok(json!({ "redid": db.redo().map_err(err)? })),
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

// ---------- argument helpers ----------

fn err(e: taskboard_core::Error) -> String {
    e.to_string()
}

/// Arguments must be an object with only known fields and every required one.
fn check_args(tool: &Tool, args: Value) -> Result<Map<String, Value>, String> {
    let map = match args {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        _ => return Err("arguments must be a JSON object".to_owned()),
    };
    let known: Vec<&str> = tool.fields.iter().map(|f| f.0).collect();
    let unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !known.contains(k))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown field(s) {}; {} accepts: {}",
            unknown.join(", "),
            tool.name,
            known.join(", ")
        ));
    }
    let missing: Vec<&str> = tool
        .fields
        .iter()
        .filter(|f| f.2 && map.get(f.0).is_none_or(Value::is_null))
        .map(|f| f.0)
        .collect();
    if !missing.is_empty() {
        return Err(format!("missing required field(s): {}", missing.join(", ")));
    }
    Ok(map)
}

fn from_args<T: DeserializeOwned>(args: Map<String, Value>) -> Result<T, String> {
    serde_json::from_value(Value::Object(args)).map_err(|e| format!("invalid arguments: {e}"))
}

fn without(args: &Map<String, Value>, key: &str) -> Map<String, Value> {
    let mut rest = args.clone();
    rest.remove(key);
    rest
}

fn id_arg(args: &Map<String, Value>, key: &str) -> Result<Id, String> {
    let s = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be an id string"))?;
    s.parse::<Id>().map_err(|_| {
        format!("{key} is not a valid id: {s:?} (ids come from get_overview / list_tasks)")
    })
}

fn opt_id(args: &Map<String, Value>, key: &str) -> Result<Option<Id>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => id_arg(args, key).map(Some),
    }
}

fn str_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn to_value<T: serde::Serialize>(v: T) -> ToolResult {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

// ---------- time ----------

fn today() -> Date {
    Zoned::now().date()
}

fn zone() -> TimeZone {
    TimeZone::system()
}

/// "YYYY-MM-DDTHH:MM" (or with seconds) in the local zone → epoch milliseconds.
fn parse_when(field: &str, s: &str) -> Result<i64, String> {
    let dt: DateTime = s.trim().parse().map_err(|_| {
        format!("{field} must be a local date-time like 2026-10-20T15:00, got {s:?}")
    })?;
    let zoned = zone().to_zoned(dt).map_err(|e| format!("{field}: {e}"))?;
    Ok(zoned.timestamp().as_millisecond())
}

/// A date (start of day) or a date-time, local.
fn parse_point(field: &str, s: &str, end_of_day: bool) -> Result<i64, String> {
    let s = s.trim();
    if s.len() == 10 {
        let d: Date = s
            .parse()
            .map_err(|_| format!("{field} must be YYYY-MM-DD or YYYY-MM-DDTHH:MM, got {s:?}"))?;
        let d = if end_of_day {
            d.checked_add(1.day()).map_err(|e| e.to_string())?
        } else {
            d
        };
        return zone()
            .to_zoned(d.to_datetime(jiff::civil::Time::midnight()))
            .map(|z| z.timestamp().as_millisecond())
            .map_err(|e| format!("{field}: {e}"));
    }
    parse_when(field, s)
}

fn iso(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|t| t.to_zoned(zone()).strftime("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}

// ---------- tools ----------

fn overview(db: &mut Db) -> ToolResult {
    let sections = db.sections().map_err(err)?;
    let groups = db.clients(false).map_err(err)?;
    let projects = db.projects(false).map_err(err)?;
    let counts = db.sidebar_counts(today()).map_err(err)?;
    let item = |section: Option<Id>| {
        sections.iter().find(|s| Some(s.id) == section).map(|s| {
            if s.item_name.trim().is_empty() {
                "group".to_owned()
            } else {
                s.item_name.clone()
            }
        })
    };
    Ok(json!({
        "today": today().to_string(),
        "sections": sections.iter().map(|s| json!({ "id": s.id, "name": s.name, "item_name": s.item_name, "color": s.color })).collect::<Vec<_>>(),
        "groups": groups.iter().map(|c| json!({
            "id": c.id, "name": c.name, "section_id": c.section_id, "kind": item(c.section_id),
            "color": c.color, "time_zone": c.time_zone, "contact": c.contact,
        })).collect::<Vec<_>>(),
        "projects": projects.iter().map(|p| json!({
            "id": p.project.id, "name": p.project.name, "client_id": p.project.client_id, "group_name": p.client_name,
            "section_id": p.project.section_id, "is_inbox": p.project.is_inbox, "state": p.project.state,
            "deadline": p.project.deadline, "open_tasks": p.open_tasks, "total_tasks": p.total_tasks, "done_tasks": p.done_tasks,
        })).collect::<Vec<_>>(),
        "counts": counts,
    }))
}

fn list_tasks(db: &mut Db, args: &Map<String, Value>) -> ToolResult {
    let categories = match str_arg(args, "status").unwrap_or("open") {
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
    let parent_id = opt_id(args, "parent_id")?;
    let q = TaskQuery {
        project_id: opt_id(args, "project_id")?,
        client_id: opt_id(args, "group_id")?,
        section_id: opt_id(args, "section_id")?,
        search: str_arg(args, "search")
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty()),
        tag: str_arg(args, "tag").map(str::to_owned),
        categories,
        due_from: str_arg(args, "due_from").map(str::to_owned),
        due_to: str_arg(args, "due_to").map(str::to_owned),
        undated: args
            .get("undated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        parent_id,
        top_level: parent_id.is_none(),
        include_archived: args
            .get("include_archived")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sort,
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

fn row_json(r: &taskboard_core::TaskRow) -> Value {
    json!({
        "id": r.id, "title": r.title, "status": r.status_name, "category": r.category, "priority": r.priority,
        "project_id": r.project_id, "project_name": r.project_name, "group_name": r.client_name,
        "parent_id": r.parent_id, "start_date": r.start_date, "due_date": r.due_date, "due_time": r.due_time,
        "estimate_minutes": r.estimate_minutes, "tags": r.tags, "progress": r.progress,
        "subtasks": { "done": r.subtasks_done, "total": r.subtasks_total }, "recurring": r.recurring,
        "completed_at": r.completed_at,
    })
}

/// A task with the names an assistant needs (status, project) and its subtasks.
fn task_json(db: &mut Db, task: &taskboard_core::Task) -> ToolResult {
    let mut v = serde_json::to_value(task).map_err(|e| e.to_string())?;
    let project = db.project(task.project_id).map_err(err)?;
    let status = db
        .statuses(task.project_id)
        .map_err(err)?
        .into_iter()
        .find(|s| s.id == task.status_id);
    let subs = db
        .list_tasks(&TaskQuery {
            parent_id: Some(task.id),
            top_level: false,
            limit: 500,
            ..Default::default()
        })
        .map_err(err)?;
    if let Value::Object(m) = &mut v {
        m.insert("project_name".into(), json!(project.name));
        m.insert(
            "status".into(),
            json!(status.as_ref().map(|s| s.name.clone())),
        );
        m.insert("category".into(), json!(status.map(|s| s.category)));
        m.insert(
            "subtasks".into(),
            json!(subs.rows.iter().map(row_json).collect::<Vec<_>>()),
        );
        m.remove("sort_key");
    }
    Ok(v)
}

fn get_task(db: &mut Db, args: &Map<String, Value>) -> ToolResult {
    let task = db.task(id_arg(args, "id")?).map_err(err)?;
    task_json(db, &task)
}

fn add_task(db: &mut Db, args: &Map<String, Value>) -> ToolResult {
    let text = str_arg(args, "text").unwrap_or("");
    let parsed = taskboard_core::quickadd::parse(text, today());
    if parsed.title.trim().is_empty() {
        return Err("text needs a task title".to_owned());
    }
    let project_id = match opt_id(args, "project_id")? {
        Some(id) => Some(id),
        None => match &parsed.project_hint {
            Some(hint) => Some(db.resolve_project(hint).map_err(err)?.ok_or_else(|| {
                format!("no project matches \"@{hint}\"; see get_overview for the project names")
            })?),
            None => None,
        },
    };
    let task = db
        .create_task(NewTask {
            title: parsed.title,
            project_id,
            parent_id: opt_id(args, "parent_id")?,
            description_md: str_arg(args, "description_md").map(str::to_owned),
            priority: parsed.priority,
            due_date: parsed.due_date,
            due_time: parsed.due_time,
            estimate_minutes: parsed.estimate_minutes,
            tags: parsed.tags,
            recurrence_rule: parsed.recurrence_rule,
            ..Default::default()
        })
        .map_err(err)?;
    task_json(db, &task)
}

fn event_json(e: &taskboard_core::Event) -> ToolResult {
    let mut v = serde_json::to_value(e).map_err(|x| x.to_string())?;
    if let Value::Object(m) = &mut v {
        m.insert("start".into(), json!(e.start_at.map(iso)));
        m.insert("end".into(), json!(e.end_at.map(iso)));
    }
    Ok(v)
}

fn list_events(db: &mut Db, args: &Map<String, Value>) -> ToolResult {
    let from = match str_arg(args, "from") {
        Some(s) => parse_point("from", s, false)?,
        None => parse_point("from", &today().to_string(), false)?,
    };
    let to = match str_arg(args, "to") {
        Some(s) => parse_point("to", s, true)?,
        None => parse_point(
            "to",
            &today()
                .checked_add(7.days())
                .map_err(|e| e.to_string())?
                .to_string(),
            false,
        )?,
    };
    let list = db.occurrences(from, to).map_err(err)?;
    Ok(json!({
        "events": list.iter().map(|o| json!({
            "event_id": o.event_id, "kind": o.kind, "title": o.title, "status": o.status,
            "start": iso(o.start_at), "end": iso(o.end_at), "all_day": o.all_day,
            "client_id": o.client_id, "group_name": o.client_name, "project_id": o.project_id, "project_name": o.project_name,
            "location": o.location, "meeting_url": o.meeting_url, "recurring": o.recurring,
        })).collect::<Vec<_>>(),
    }))
}

/// `start` / `end` (local text) become `start_at` / `end_at` (milliseconds).
fn with_times(mut args: Map<String, Value>, required: bool) -> Result<Map<String, Value>, String> {
    for (text, ms) in [("start", "start_at"), ("end", "end_at")] {
        match args.remove(text) {
            Some(Value::String(s)) => {
                args.insert(ms.to_owned(), json!(parse_when(text, &s)?));
            }
            Some(other) => return Err(format!("{text} must be a string, got {other}")),
            None if required => return Err(format!("missing required field(s): {text}")),
            None => {}
        }
    }
    Ok(args)
}

fn add_event(db: &mut Db, args: Map<String, Value>) -> ToolResult {
    let args = with_times(args, true)?;
    let input: NewEvent = from_args(args)?;
    if input.kind == EventKind::Meeting && input.title.trim().is_empty() {
        return Err("title must not be empty".to_owned());
    }
    event_json(&db.create_event(input).map_err(err)?)
}

fn update_event(db: &mut Db, args: Map<String, Value>) -> ToolResult {
    let id = id_arg(&args, "id")?;
    let args = with_times(without(&args, "id"), false)?;
    let patch: EventPatch = from_args(args)?;
    event_json(&db.update_event(id, patch).map_err(err)?)
}

fn log_time(db: &mut Db, args: &Map<String, Value>) -> ToolResult {
    let started = parse_when("started", str_arg(args, "started").unwrap_or(""))?;
    let ended = parse_when("ended", str_arg(args, "ended").unwrap_or(""))?;
    if ended <= started {
        return Err("ended must be after started".to_owned());
    }
    let id = db
        .log_time(NewTimeEntry {
            task_id: opt_id(args, "task_id")?,
            project_id: opt_id(args, "project_id")?,
            client_id: opt_id(args, "client_id")?,
            started_at: started,
            ended_at: Some(ended),
            note: str_arg(args, "note").map(str::to_owned),
        })
        .map_err(err)?;
    Ok(json!({ "entry_id": id, "minutes": (ended - started) / 60_000 }))
}
