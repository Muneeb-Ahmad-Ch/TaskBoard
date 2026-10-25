//! Calendar: meetings and time blocks (whole series, one occurrence, or this
//! and following), reminders and repeating series, working hours, days off,
//! free slots and day plans.

use jiff::ToSpan;
use serde_json::{Value, json};
use taskboard_core::{
    DayOverride, Db, EventKind, EventPatch, FindSlots, Interval, NewEvent, OccurrencePatch,
    OverrideKind, ReminderKind, ReminderQuery, WorkingDay,
};

use super::{
    Args, BOOL, DAY, EVENT_WHEN, ID, NULLABLE_ID, NULLABLE_STR, RANGE_FROM, RANGE_TO, STR, Tool,
    ToolResult, day_start, err, from_args, hm, id_arg, iso, opt_id, parse_date, parse_when,
    str_arg, window, without,
};
use taskboard_core::Id;

const KIND: &str = r#"{"type":"string","enum":["meeting","block"],"description":"meeting, or block (focus time, usually for a task)"}"#;

const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const HOURS: &str = r#"{"type":"array","items":{"type":"string"},"description":"Local time ranges like \"09:00-12:30\"; empty = not working"}"#;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "list_events",
        description: "Meetings and time blocks between two local dates or date-times (default: today and the next 7 days; at most 366 days), with the group / project they belong to and, for a time block, the task it is for (task_id). Filter by kind, client_id (group), project_id or task_id. Each repeating occurrence has occurrence_start, which update_event needs to change just that one.",
        fields: &[
            ("from", RANGE_FROM, false),
            ("to", RANGE_TO, false),
            ("kind", KIND, false),
            ("client_id", ID, false),
            ("project_id", ID, false),
            ("task_id", ID, false),
        ],
    },
    Tool {
        name: "get_event",
        description: "One meeting or time block in full: notes, attendees, link, repeat rule. With occurrence_start (from list_events), also that occurrence's own notes.",
        fields: &[
            ("id", ID, true),
            (
                "occurrence_start",
                r#"{"type":"string","description":"For a repeating event: the occurrence_start of one occurrence, to read its own notes"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "add_event",
        description: "Adds a meeting or a time block (focus time) at local times, or all day when start / end are plain dates. end defaults to an hour after start (the same day for all-day). recurrence_rule makes it repeat. To book time for a task, schedule_task is simpler.",
        fields: &[
            ("kind", KIND, true),
            ("title", STR, true),
            ("start", EVENT_WHEN, true),
            ("end", EVENT_WHEN, false),
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
                "status",
                r#"{"type":"string","enum":["confirmed","tentative","cancelled"]}"#,
                false,
            ),
            (
                "counts_as_time",
                r#"{"type":"boolean","description":"Count it as logged work time"}"#,
                false,
            ),
            (
                "flexible",
                r#"{"type":"boolean","description":"A block that slot finding may book over"}"#,
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
        name: "schedule_task",
        description: "Books a time block for a task (like dragging it onto the calendar): titled after the task, in its project, linked to it. minutes defaults to the task's estimate, else 60.",
        fields: &[
            ("task_id", ID, true),
            ("start", super::WHEN, true),
            (
                "minutes",
                r#"{"type":"integer","description":"Length, 5 to 720 minutes"}"#,
                false,
            ),
            ("title", STR, false),
            (
                "flexible",
                r#"{"type":"boolean","description":"Slot finding may book over it (default true)"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_event",
        description: "Changes a meeting or time block. Moving start alone keeps its length; plain dates make it all-day. scope: series (default, every occurrence), occurrence (just the one starting at occurrence_start: title, notes_md, start, end, or cancelled=true to skip it) or following (that one and every later one).",
        fields: &[
            ("id", ID, true),
            (
                "scope",
                r#"{"type":"string","enum":["series","occurrence","following"]}"#,
                false,
            ),
            (
                "occurrence_start",
                r#"{"type":"string","description":"For scope occurrence / following: the occurrence's occurrence_start from list_events"}"#,
                false,
            ),
            (
                "cancelled",
                r#"{"type":"boolean","description":"scope occurrence only: skip this occurrence (false brings it back)"}"#,
                false,
            ),
            ("title", STR, false),
            ("kind", KIND, false),
            ("start", EVENT_WHEN, false),
            ("end", EVENT_WHEN, false),
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
            ("flexible", BOOL, false),
            ("recurrence_rule", NULLABLE_STR, false),
        ],
    },
    Tool {
        name: "delete_event",
        description: "Deletes a meeting or time block, the whole series for a repeating one (undoable). To skip one occurrence use update_event scope occurrence cancelled=true.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "get_working_hours",
        description: "Your weekly working hours, Monday first, as local time ranges.",
        fields: &[],
    },
    Tool {
        name: "set_working_hours",
        description: "Changes working hours for the days given (others keep theirs). week: [{\"day\": \"Monday\", \"hours\": [\"09:00-12:00\", \"13:00-17:00\"]}]; empty hours = day off every week.",
        fields: &[(
            "week",
            r#"{"type":"array","items":{"type":"object","properties":{"day":{"type":["string","integer"],"description":"Monday…Sunday (or 1-7)"},"hours":{"type":"array","items":{"type":"string"}}},"required":["day","hours"]}}"#,
            true,
        )],
    },
    Tool {
        name: "list_days_off",
        description: "Days off and days with custom hours between two dates.",
        fields: &[("from", DAY, true), ("to", DAY, true)],
    },
    Tool {
        name: "set_day_off",
        description: "Marks a date as a day off, or (with hours) as a day with different working hours.",
        fields: &[
            ("date", DAY, true),
            ("note", STR, false),
            ("hours", HOURS, false),
        ],
    },
    Tool {
        name: "clear_day_off",
        description: "Removes a day off / custom-hours day, back to the weekly hours.",
        fields: &[("date", DAY, true)],
    },
    Tool {
        name: "find_slots",
        description: "Free time for a meeting of duration_min between two dates, inside your working hours (and the group's, in its time zone, when group_id is given), avoiding meetings and blocks. At most 31 days; meetings of 5 minutes to 8 hours.",
        fields: &[
            ("from", DAY, true),
            ("to", DAY, true),
            (
                "duration_min",
                r#"{"type":"integer","description":"Meeting length in minutes"}"#,
                true,
            ),
            (
                "group_id",
                r#"{"type":"string","description":"Meet with this group: their hours count too"}"#,
                false,
            ),
            (
                "buffer_min",
                r#"{"type":"integer","description":"Gap to keep around other meetings"}"#,
                false,
            ),
            (
                "earliest",
                r#"{"type":"string","description":"HH:MM, not before (local)"}"#,
                false,
            ),
            (
                "latest",
                r#"{"type":"string","description":"HH:MM, not after (local)"}"#,
                false,
            ),
            ("max_meetings_per_day", r#"{"type":"integer"}"#, false),
            (
                "min_notice_min",
                r#"{"type":"integer","description":"Not sooner than this many minutes from now"}"#,
                false,
            ),
            (
                "blocks_flexible",
                r#"{"type":"boolean","description":"Treat flexible time blocks as free"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "get_day_plans",
        description: "Per day between two dates (at most 62): working time available, meetings, blocks, estimates of tasks due and time logged, in minutes.",
        fields: &[("from", DAY, true), ("to", DAY, true)],
    },
    Tool {
        name: "list_reminders",
        description: "What will remind the user, day by day: tasks with a due time, meetings (and time blocks if asked), plus every repeating task with its next repeats, even ones not created yet (projected: true). Overdue ones come first. remind_at is when the notification fires (null when none will). Use it for \"what reminders do I have\" or \"when is rent due next\".",
        fields: &[
            (
                "from",
                r#"{"type":"string","description":"First day, YYYY-MM-DD (default today)"}"#,
                false,
            ),
            (
                "days",
                r#"{"type":"integer","description":"Days to look ahead, 1-366 (default 30)"}"#,
                false,
            ),
            (
                "kinds",
                r#"{"type":"array","items":{"type":"string","enum":["task","meeting","block"]},"description":"Default task and meeting"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "list_repeating",
        description: "Every repeating task, meeting series and time-block series, soonest next date first, with its rule in words. To stop a task repeating: update_task with recurrence_rule null.",
        fields: &[(
            "kinds",
            r#"{"type":"array","items":{"type":"string","enum":["task","meeting","block"]},"description":"Default all"}"#,
            false,
        )],
    },
    Tool {
        name: "get_hours",
        description: "Planned (meetings, blocks) and logged minutes per group between two dates.",
        fields: &[("from", DAY, true), ("to", DAY, true)],
    },
];

pub(super) fn call(db: &mut Db, name: &str, args: &Args) -> ToolResult {
    match name {
        "list_events" => list_events(db, args),
        "get_event" => {
            let id = id_arg(args, "id")?;
            let mut v = event_json(&db.event(id).map_err(err)?)?;
            if let Some(s) = str_arg(args, "occurrence_start") {
                let original = parse_when("occurrence_start", s)?;
                let notes = db.occurrence_notes(id, original).map_err(err)?;
                v["occurrence"] = json!({ "occurrence_start": s, "notes_md": notes });
            }
            Ok(v)
        }
        "add_event" => {
            let input: NewEvent = from_args(with_times(args.clone(), Times::Add)?)?;
            if input.kind == EventKind::Meeting && input.title.trim().is_empty() {
                return Err("title must not be empty".to_owned());
            }
            event_json(&db.create_event(input).map_err(err)?)
        }
        "schedule_task" => schedule_task(db, args),
        "update_event" => update_event(db, args),
        "delete_event" => {
            db.delete_event(id_arg(args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "get_working_hours" => working_hours(db),
        "set_working_hours" => set_working_hours(db, args),
        "list_days_off" => {
            let (from, to) = date_range(args, 366)?;
            let days = db.day_overrides(&from, &to).map_err(err)?;
            Ok(json!({ "days": days.iter().map(|d| json!({
                "date": d.date, "off": d.kind == OverrideKind::Off, "hours": hours_text(&d.intervals), "note": d.note,
            })).collect::<Vec<_>>() }))
        }
        "set_day_off" => {
            let date = parse_date("date", str_arg(args, "date").unwrap_or(""))?.to_string();
            let intervals = match args.get("hours") {
                Some(v) if !v.is_null() => parse_hours(v)?,
                _ => Vec::new(),
            };
            let kind = if intervals.is_empty() {
                OverrideKind::Off
            } else {
                OverrideKind::Custom
            };
            db.set_day_override(DayOverride {
                date: date.clone(),
                kind,
                intervals,
                note: str_arg(args, "note").unwrap_or("").to_owned(),
            })
            .map_err(err)?;
            Ok(json!({ "date": date, "off": kind == OverrideKind::Off }))
        }
        "clear_day_off" => {
            let date = parse_date("date", str_arg(args, "date").unwrap_or(""))?.to_string();
            db.clear_day_override(&date).map_err(err)?;
            Ok(json!({ "cleared": date }))
        }
        "find_slots" => find_slots(db, args),
        "list_reminders" => list_reminders(db, args),
        "list_repeating" => list_repeating(db, args),
        "get_day_plans" => {
            let (from, to) = date_range(args, 62)?;
            let plans = db.day_plans(&from, &to).map_err(err)?;
            Ok(json!({ "days": plans.iter().map(|p| json!({
                "date": p.date, "day_off": p.day_off, "note": p.note, "available_min": p.available_min,
                "meetings_min": p.meetings_min, "blocks_min": p.blocks_min, "due_estimate_min": p.due_estimate_min,
                "logged_min": p.logged_min,
                "working": p.working.iter().map(|(a, b)| format!("{}-{}", hm(*a), hm(*b))).collect::<Vec<_>>(),
            })).collect::<Vec<_>>() }))
        }
        "get_hours" => {
            let (from, to) = date_range(args, 366)?;
            let start = day_start("from", parse_date("from", &from)?)?;
            let end = day_start(
                "to",
                parse_date("to", &to)?
                    .checked_add(1.day())
                    .map_err(|e| e.to_string())?,
            )?;
            Ok(json!({ "groups": db.hours_by_client(start, end).map_err(err)? }))
        }
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

fn reminder_kinds(args: &Args) -> Result<Vec<ReminderKind>, String> {
    match args.get("kinds") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(list)) => list
            .iter()
            .map(|v| match v.as_str() {
                Some("task") => Ok(ReminderKind::Task),
                Some("meeting") => Ok(ReminderKind::Meeting),
                Some("block") => Ok(ReminderKind::Block),
                _ => Err(format!("kinds must be task, meeting or block, got {v}")),
            })
            .collect(),
        Some(_) => Err("kinds must be a list".to_owned()),
    }
}

fn list_reminders(db: &mut Db, args: &Args) -> ToolResult {
    let q = ReminderQuery {
        from: str_arg(args, "from").map(str::to_owned),
        days: args.get("days").and_then(Value::as_i64),
        kinds: reminder_kinds(args)?,
    };
    let r = db
        .upcoming_reminders(&q, taskboard_core::clock::now_ms())
        .map_err(err)?;
    Ok(json!({
        "reminders_on": r.reminders_on,
        "meeting_lead_min": r.meeting_lead_min,
        "task_lead_min": r.task_lead_min,
        "from": r.from,
        "to": r.to,
        "items": r.items.iter().map(|i| json!({
            "kind": i.kind, "title": i.title, "date": i.date, "time": i.time,
            "remind_at": i.remind_at.map(iso), "repeat": i.repeat, "projected": i.projected,
            "overdue": i.overdue, "task_id": i.task_id, "event_id": i.event_id,
            "project_name": i.project_name, "group_name": i.client_name, "meeting_url": i.meeting_url,
        })).collect::<Vec<_>>(),
    }))
}

fn list_repeating(db: &mut Db, args: &Args) -> ToolResult {
    let kinds = reminder_kinds(args)?;
    let list = db.repeating(taskboard_core::clock::now_ms()).map_err(err)?;
    Ok(json!({
        "items": list.iter().filter(|r| kinds.is_empty() || kinds.contains(&r.kind)).map(|r| json!({
            "kind": r.kind, "title": r.title, "repeat": r.describe, "rule": r.rule,
            "after_completion": r.after_completion, "next_date": r.next_date, "next_time": r.next_time,
            "task_id": r.task_id, "event_id": r.event_id, "project_name": r.project_name,
            "group_name": r.client_name,
        })).collect::<Vec<_>>(),
    }))
}

fn event_json(e: &taskboard_core::Event) -> ToolResult {
    let mut v = serde_json::to_value(e).map_err(|x| x.to_string())?;
    if let Value::Object(m) = &mut v {
        m.insert("start".into(), json!(e.start_at.map(iso)));
        m.insert("end".into(), json!(e.end_at.map(iso)));
    }
    Ok(v)
}

fn list_events(db: &mut Db, args: &Args) -> ToolResult {
    let (from, to) = window(args, 366)?;
    let kind = match str_arg(args, "kind") {
        None => None,
        Some("meeting") => Some(EventKind::Meeting),
        Some("block") => Some(EventKind::Block),
        Some(other) => return Err(format!("kind must be meeting or block, got {other:?}")),
    };
    let group = opt_id(args, "client_id")?;
    let project = opt_id(args, "project_id")?;
    let task = opt_id(args, "task_id")?;
    let matches = |want: Option<Id>, have: Option<Id>| want.is_none_or(|w| have == Some(w));
    let list: Vec<_> = db
        .occurrences(from, to)
        .map_err(err)?
        .into_iter()
        .filter(|o| {
            kind.is_none_or(|k| o.kind == k)
                && matches(group, o.client_id)
                && matches(project, o.project_id)
                && matches(task, o.task_id)
        })
        .collect();
    Ok(json!({
        "events": list.iter().map(|o| json!({
            "event_id": o.event_id, "kind": o.kind, "title": o.title, "status": o.status,
            "start": iso(o.start_at), "end": iso(o.end_at), "occurrence_start": iso(o.original_start),
            "all_day": o.all_day, "client_id": o.client_id, "group_name": o.client_name,
            "project_id": o.project_id, "project_name": o.project_name, "location": o.location,
            "meeting_url": o.meeting_url, "recurring": o.recurring, "changed": o.modified,
            "task_id": o.task_id, "counts_as_time": o.counts_as_time, "flexible": o.flexible,
        })).collect::<Vec<_>>(),
    }))
}

/// What `with_times` is preparing: a new event, a change to a whole series
/// (or from one occurrence on), or a change to one occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Times {
    Add,
    Change,
    Occurrence,
}

const HOUR_MS: i64 = 3_600_000;

/// `start` / `end` (local text) become `start_at` / `end_at` (milliseconds),
/// or `all_day_start` / `all_day_end` when they are plain dates. A new
/// event's end defaults to an hour later (the same day when all-day).
fn with_times(mut args: Args, mode: Times) -> Result<Args, String> {
    let mut take = |key: &str| match args.remove(key) {
        Some(Value::String(s)) => Ok(Some(s.trim().to_owned())),
        None | Some(Value::Null) => Ok(None),
        Some(other) => Err(format!("{key} must be a string, got {other}")),
    };
    let start = take("start")?;
    let end = take("end")?;
    let is_date = |s: &str| !s.contains('T') && !s.contains(' ');
    let all_day = match (&start, &end) {
        (None, _) if mode == Times::Add => {
            return Err("missing required field(s): start".to_owned());
        }
        (None, None) => return Ok(args),
        (Some(a), Some(b)) if is_date(a) != is_date(b) => {
            return Err("start and end must both be dates (all-day) or both date-times".to_owned());
        }
        (Some(a), _) => is_date(a),
        (None, Some(b)) => is_date(b),
    };
    if all_day {
        if mode == Times::Occurrence {
            return Err("one occurrence moves by date-time (YYYY-MM-DDTHH:MM); to make the series all-day use scope series".to_owned());
        }
        let a = start
            .as_deref()
            .map(|s| parse_date("start", s))
            .transpose()?;
        let b = end.as_deref().map(|s| parse_date("end", s)).transpose()?;
        if let (Some(a), Some(b)) = (a, b)
            && b < a
        {
            return Err("end date must not be before start date".to_owned());
        }
        let b = if mode == Times::Add { b.or(a) } else { b };
        if let Some(a) = a {
            args.insert("all_day_start".into(), json!(a.to_string()));
        }
        if let Some(b) = b {
            args.insert("all_day_end".into(), json!(b.to_string()));
        }
    } else {
        let a = start
            .as_deref()
            .map(|s| parse_when("start", s))
            .transpose()?;
        let b = end.as_deref().map(|s| parse_when("end", s)).transpose()?;
        let b = match (a, b) {
            (Some(a), None) if mode == Times::Add => Some(a + HOUR_MS),
            (_, b) => b,
        };
        if let (Some(a), Some(b)) = (a, b)
            && b <= a
        {
            return Err("end must be after start".to_owned());
        }
        if let Some(a) = a {
            args.insert("start_at".into(), json!(a));
        }
        if let Some(b) = b {
            args.insert("end_at".into(), json!(b));
        }
    }
    Ok(args)
}

/// Moving only the start keeps the event's length, as dragging does in the app.
fn keep_length(db: &Db, id: Id, args: &mut Args) -> Result<(), String> {
    if args.contains_key("end_at") {
        return Ok(());
    }
    if let Some(start) = args.get("start_at").and_then(Value::as_i64) {
        let e = db.event(id).map_err(err)?;
        let length = match (e.start_at, e.end_at) {
            (Some(a), Some(b)) if b > a => b - a,
            _ => HOUR_MS,
        };
        args.insert("end_at".into(), json!(start + length));
    }
    Ok(())
}

fn schedule_task(db: &mut Db, args: &Args) -> ToolResult {
    let task = db.task(id_arg(args, "task_id")?).map_err(err)?;
    let start = parse_when("start", str_arg(args, "start").unwrap_or(""))?;
    let minutes = match args.get("minutes").and_then(Value::as_i64) {
        Some(m) => m,
        None => task.estimate_minutes.filter(|m| *m > 0).unwrap_or(60),
    };
    if !(5..=720).contains(&minutes) {
        return Err(format!("minutes must be between 5 and 720, got {minutes}"));
    }
    let title = str_arg(args, "title")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(&task.title)
        .to_owned();
    let mut input = Args::new();
    input.insert("kind".into(), json!("block"));
    input.insert("title".into(), json!(title));
    input.insert("project_id".into(), json!(task.project_id));
    input.insert("task_id".into(), json!(task.id));
    input.insert("start_at".into(), json!(start));
    input.insert("end_at".into(), json!(start + minutes * 60_000));
    input.insert(
        "flexible".into(),
        json!(
            args.get("flexible")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        ),
    );
    let event: NewEvent = from_args(input)?;
    event_json(&db.create_event(event).map_err(err)?)
}

fn update_event(db: &mut Db, args: &Args) -> ToolResult {
    let id = id_arg(args, "id")?;
    let scope = str_arg(args, "scope").unwrap_or("series");
    let occurrence = || -> Result<i64, String> {
        let s = str_arg(args, "occurrence_start")
            .ok_or_else(|| format!("scope {scope} needs occurrence_start (from list_events)"))?;
        parse_when("occurrence_start", s)
    };
    match scope {
        "series" => {
            if args.contains_key("cancelled") || args.contains_key("occurrence_start") {
                return Err("cancelled / occurrence_start apply to scope occurrence; for the whole series use status".to_owned());
            }
            let mut changes = with_times(without(args, &["id", "scope"]), Times::Change)?;
            keep_length(db, id, &mut changes)?;
            let patch: EventPatch = from_args(changes)?;
            event_json(&db.update_event(id, patch).map_err(err)?)
        }
        "occurrence" => {
            let original = occurrence()?;
            let allowed = [
                "id",
                "scope",
                "occurrence_start",
                "cancelled",
                "title",
                "notes_md",
                "start",
                "end",
            ];
            let extra: Vec<&str> = args
                .keys()
                .map(String::as_str)
                .filter(|k| !allowed.contains(k))
                .collect();
            if !extra.is_empty() {
                return Err(format!(
                    "{} can't change for one occurrence (only title, notes_md, start, end, cancelled); use scope following or series",
                    extra.join(", ")
                ));
            }
            let mut changes = with_times(
                without(args, &["id", "scope", "occurrence_start"]),
                Times::Occurrence,
            )?;
            keep_length(db, id, &mut changes)?;
            let patch: OccurrencePatch = from_args(changes)?;
            db.edit_occurrence(id, original, patch).map_err(err)?;
            Ok(json!({ "updated": "occurrence", "occurrence_start": iso(original) }))
        }
        "following" => {
            let original = occurrence()?;
            if args.contains_key("cancelled") {
                return Err(
                    "to stop a series from here on, change its recurrence_rule or delete it"
                        .to_owned(),
                );
            }
            let mut changes = with_times(
                without(args, &["id", "scope", "occurrence_start"]),
                Times::Change,
            )?;
            keep_length(db, id, &mut changes)?;
            let patch: EventPatch = from_args(changes)?;
            event_json(&db.split_series(id, original, patch).map_err(err)?)
        }
        other => Err(format!(
            "scope must be series, occurrence or following, got {other:?}"
        )),
    }
}

fn hours_text(intervals: &[Interval]) -> Vec<String> {
    intervals
        .iter()
        .map(|i| format!("{}-{}", i.start, i.end))
        .collect()
}

fn parse_hours(v: &Value) -> Result<Vec<Interval>, String> {
    let list = v
        .as_array()
        .ok_or("hours must be a list like [\"09:00-17:00\"]")?;
    list.iter()
        .map(|item| {
            let s = item
                .as_str()
                .ok_or("each hours entry must be text like \"09:00-17:00\"")?;
            let (start, end) = s
                .split_once('-')
                .ok_or_else(|| format!("hours entry {s:?} must look like 09:00-17:00"))?;
            Ok(Interval {
                start: start.trim().to_owned(),
                end: end.trim().to_owned(),
            })
        })
        .collect()
}

fn parse_day(v: &Value) -> Result<u8, String> {
    if let Some(n) = v.as_u64() {
        return u8::try_from(n)
            .ok()
            .filter(|d| (1..=7).contains(d))
            .ok_or_else(|| format!("day {n} must be 1 (Monday) to 7 (Sunday)"));
    }
    let s = v
        .as_str()
        .ok_or("day must be a weekday name or 1-7")?
        .trim()
        .to_lowercase();
    DAYS.iter()
        .position(|d| d.to_lowercase() == s || (s.len() >= 3 && d.to_lowercase().starts_with(&s)))
        .and_then(|i| u8::try_from(i + 1).ok())
        .ok_or_else(|| format!("unknown day {s:?}; use Monday…Sunday"))
}

fn working_hours(db: &mut Db) -> ToolResult {
    let mut week = db.working_hours().map_err(err)?;
    week.sort_by_key(|d| d.weekday);
    Ok(json!({ "week": week.iter().map(|d| json!({
        "day": DAYS.get(usize::from(d.weekday).saturating_sub(1)).copied().unwrap_or("?"),
        "weekday": d.weekday,
        "hours": hours_text(&d.intervals),
    })).collect::<Vec<_>>() }))
}

fn set_working_hours(db: &mut Db, args: &Args) -> ToolResult {
    let changes = args
        .get("week")
        .and_then(Value::as_array)
        .ok_or("week must be a list of {day, hours}")?;
    let mut week: Vec<WorkingDay> = db.working_hours().map_err(err)?;
    for change in changes {
        let obj = change
            .as_object()
            .ok_or("each week entry must be {\"day\": …, \"hours\": […]}")?;
        if let Some(k) = obj.keys().find(|k| *k != "day" && *k != "hours") {
            return Err(format!(
                "unknown field {k} in a week entry; use day and hours"
            ));
        }
        let day = parse_day(obj.get("day").unwrap_or(&Value::Null))?;
        let intervals = parse_hours(obj.get("hours").unwrap_or(&Value::Null))?;
        match week.iter_mut().find(|d| d.weekday == day) {
            Some(d) => d.intervals = intervals,
            None => week.push(WorkingDay {
                weekday: day,
                intervals,
            }),
        }
    }
    db.set_working_hours(week).map_err(err)?;
    working_hours(db)
}

/// `from` / `to` as dates (to inclusive), refused when longer than `max_days`.
fn date_range(args: &Args, max_days: i64) -> Result<(String, String), String> {
    let from = parse_date("from", str_arg(args, "from").unwrap_or(""))?;
    let to = parse_date("to", str_arg(args, "to").unwrap_or(""))?;
    if to < from {
        return Err("to must not be before from".to_owned());
    }
    let days = from.until(to).map_err(|e| e.to_string())?.get_days();
    if i64::from(days) >= max_days {
        return Err(format!(
            "the range is longer than {max_days} days; ask for a shorter window"
        ));
    }
    Ok((from.to_string(), to.to_string()))
}

fn find_slots(db: &mut Db, args: &Args) -> ToolResult {
    let (from, to) = date_range(args, 31)?;
    let int = |k: &str| args.get(k).and_then(Value::as_i64);
    let duration = int("duration_min").ok_or("duration_min must be a whole number of minutes")?;
    if !(5..=8 * 60).contains(&duration) {
        return Err("duration_min must be between 5 and 480 (8 hours)".to_owned());
    }
    let q = FindSlots {
        from_date: from,
        to_date: to,
        duration_min: duration,
        buffer_min: int("buffer_min"),
        earliest: str_arg(args, "earliest").map(str::to_owned),
        latest: str_arg(args, "latest").map(str::to_owned),
        blocks_flexible: args.get("blocks_flexible").and_then(Value::as_bool),
        max_meetings_per_day: int("max_meetings_per_day").and_then(|n| u32::try_from(n).ok()),
        min_notice_min: int("min_notice_min"),
        client_id: opt_id(args, "group_id")?,
    };
    let r = db.find_slots(&q).map_err(err)?;
    Ok(json!({
        "local_time_zone": r.local_time_zone,
        "group_time_zone": r.client_time_zone,
        "days": r.days.iter().map(|d| json!({
            "date": d.date,
            "slots": d.slots.iter().map(|s| json!({ "start": iso(s.start_at), "end": iso(s.end_at) })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_parse_by_name_prefix_or_number() {
        assert_eq!(parse_day(&json!("monday")), Ok(1));
        assert_eq!(parse_day(&json!("Sun")), Ok(7));
        assert_eq!(parse_day(&json!(3)), Ok(3));
        assert!(parse_day(&json!(0)).is_err());
        assert!(parse_day(&json!("someday")).is_err());
    }

    #[test]
    fn hours_parse_ranges() {
        let h = parse_hours(&json!(["09:00-12:00", " 13:00 - 17:30 "])).unwrap_or_default();
        assert_eq!(hours_text(&h), ["09:00-12:00", "13:00-17:30"]);
        assert!(parse_hours(&json!(["nine to five"])).is_err());
    }
}
