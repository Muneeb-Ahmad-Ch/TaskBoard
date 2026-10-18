//! The tools an assistant can call, their JSON schemas, and how each maps
//! onto the core. One file per area; this one holds the shared pieces.
//!
//! Arguments are checked against each tool's property list before anything
//! runs, so a misspelled field is an error the assistant can act on rather
//! than something silently ignored. Writes go through the core's normal
//! paths, so they are undoable and the app shows them at once.

mod calendar;
mod misc;
mod structure;
mod tasks;
mod time;

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use taskboard_core::{Db, Id};

pub(crate) type ToolResult = Result<Value, String>;
pub(crate) type Args = Map<String, Value>;

pub(crate) struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// (field, JSON schema, required)
    pub fields: &'static [(&'static str, &'static str, bool)],
}

// Schemas as JSON text: short to write; `definitions()` parses them.
pub(crate) const ID: &str = r#"{"type":"string"}"#;
pub(crate) const IDS: &str = r#"{"type":"array","items":{"type":"string"},"description":"Several ids at once (instead of id); one undo step for all"}"#;
pub(crate) const STR: &str = r#"{"type":"string"}"#;
pub(crate) const DATE: &str =
    r#"{"type":["string","null"],"description":"YYYY-MM-DD; null clears it"}"#;
pub(crate) const DAY: &str = r#"{"type":"string","description":"YYYY-MM-DD"}"#;
pub(crate) const TIME: &str =
    r#"{"type":["string","null"],"description":"HH:MM, 24-hour; null clears it"}"#;
pub(crate) const WHEN: &str =
    r#"{"type":"string","description":"Local date-time, YYYY-MM-DDTHH:MM"}"#;
pub(crate) const RANGE_FROM: &str =
    r#"{"type":"string","description":"YYYY-MM-DD or YYYY-MM-DDTHH:MM, inclusive"}"#;
pub(crate) const RANGE_TO: &str = r#"{"type":"string","description":"YYYY-MM-DD (whole day included) or YYYY-MM-DDTHH:MM, exclusive"}"#;
pub(crate) const PRIORITY: &str =
    r#"{"type":"string","enum":["none","low","med","high","urgent"]}"#;
pub(crate) const CATEGORY: &str = r#"{"type":"string","enum":["todo","active","done","cancelled"],"description":"What the column means: to do, in progress, done or cancelled"}"#;
pub(crate) const COLOR: &str =
    r#"{"type":"string","description":"\"gray\" or a hue 0-360, e.g. \"262\""}"#;
pub(crate) const BOOL: &str = r#"{"type":"boolean"}"#;
pub(crate) const INT: &str = r#"{"type":["integer","null"]}"#;
pub(crate) const TAGS: &str = r#"{"type":"array","items":{"type":"string"}}"#;
pub(crate) const NULLABLE_ID: &str = r#"{"type":["string","null"]}"#;
pub(crate) const NULLABLE_STR: &str = r#"{"type":["string","null"]}"#;
pub(crate) const STATUS_NAME: &str = r#"{"type":"string","description":"Column name in the task's project, any case (e.g. \"In Progress\"); see get_overview or list_statuses"}"#;

/// Every tool, by area, in the order `tools/list` shows them.
fn areas() -> [&'static [Tool]; 5] {
    [
        structure::TOOLS,
        tasks::TOOLS,
        calendar::TOOLS,
        time::TOOLS,
        misc::TOOLS,
    ]
}

fn all_tools() -> impl Iterator<Item = &'static Tool> {
    areas().into_iter().flat_map(|a| a.iter())
}

/// Tool definitions as MCP wants them.
pub fn definitions() -> Vec<Value> {
    all_tools()
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
    let tool = all_tools().find(|t| t.name == name).ok_or_else(|| {
        format!("unknown tool \"{name}\"; call tools/list for the available ones")
    })?;
    let args = check_args(tool, args)?;
    let in_area = |area: &[Tool]| area.iter().any(|t| t.name == name);
    if in_area(structure::TOOLS) {
        structure::call(db, name, &args)
    } else if in_area(tasks::TOOLS) {
        tasks::call(db, name, &args)
    } else if in_area(calendar::TOOLS) {
        calendar::call(db, name, &args)
    } else if in_area(time::TOOLS) {
        time::call(db, name, &args)
    } else {
        misc::call(db, name, &args)
    }
}

// ---------- argument helpers ----------

pub(crate) fn err(e: taskboard_core::Error) -> String {
    e.to_string()
}

/// Arguments must be an object with only known fields and every required one.
fn check_args(tool: &Tool, args: Value) -> Result<Args, String> {
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
        let accepts = if known.is_empty() {
            "nothing".to_owned()
        } else {
            known.join(", ")
        };
        return Err(format!(
            "unknown field(s) {}; {} accepts: {accepts}",
            unknown.join(", "),
            tool.name,
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

pub(crate) fn from_args<T: DeserializeOwned>(args: Args) -> Result<T, String> {
    serde_json::from_value(Value::Object(args)).map_err(|e| format!("invalid arguments: {e}"))
}

/// A copy of the arguments without the given keys.
pub(crate) fn without(args: &Args, keys: &[&str]) -> Args {
    let mut rest = args.clone();
    for k in keys {
        rest.remove(*k);
    }
    rest
}

pub(crate) fn parse_id(field: &str, v: &Value) -> Result<Id, String> {
    let s = v
        .as_str()
        .ok_or_else(|| format!("{field} must be an id string"))?;
    s.parse::<Id>().map_err(|_| {
        format!("{field} is not a valid id: {s:?} (ids come from get_overview / list_tasks)")
    })
}

pub(crate) fn id_arg(args: &Args, key: &str) -> Result<Id, String> {
    parse_id(key, args.get(key).unwrap_or(&Value::Null))
}

pub(crate) fn opt_id(args: &Args, key: &str) -> Result<Option<Id>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => parse_id(key, v).map(Some),
    }
}

/// `id` or `ids` (exactly one of them).
pub(crate) fn id_or_ids(args: &Args) -> Result<(Vec<Id>, bool), String> {
    let one = args.get("id").filter(|v| !v.is_null());
    let many = args.get("ids").filter(|v| !v.is_null());
    match (one, many) {
        (Some(_), Some(_)) => Err("give either id or ids, not both".to_owned()),
        (Some(v), None) => Ok((vec![parse_id("id", v)?], false)),
        (None, Some(Value::Array(list))) => {
            if list.is_empty() {
                return Err("ids is empty".to_owned());
            }
            if list.len() > 500 {
                return Err("at most 500 ids at once".to_owned());
            }
            let mut out = Vec::with_capacity(list.len());
            for v in list {
                let id = parse_id("ids", v)?;
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            Ok((out, true))
        }
        (None, Some(_)) => Err("ids must be an array of id strings".to_owned()),
        (None, None) => Err("missing id (or ids for several at once)".to_owned()),
    }
}

pub(crate) fn str_arg<'a>(args: &'a Args, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(crate) fn bool_arg(args: &Args, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn to_value<T: serde::Serialize>(v: T) -> ToolResult {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// "Edit 3 tasks" / "Edit 1 task", the undo label for a bulk change.
pub(crate) fn tasks_label(verb: &str, n: usize) -> String {
    if n == 1 {
        format!("{verb} 1 task")
    } else {
        format!("{verb} {n} tasks")
    }
}

// ---------- time ----------

pub(crate) fn today() -> Date {
    Zoned::now().date()
}

pub(crate) fn zone() -> TimeZone {
    TimeZone::system()
}

pub(crate) fn parse_date(field: &str, s: &str) -> Result<Date, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("{field} must be a date like 2026-10-20, got {s:?}"))
}

/// "YYYY-MM-DDTHH:MM" (or with seconds) in the local zone → epoch milliseconds.
pub(crate) fn parse_when(field: &str, s: &str) -> Result<i64, String> {
    let dt: DateTime = s.trim().parse().map_err(|_| {
        format!("{field} must be a local date-time like 2026-10-20T15:00, got {s:?}")
    })?;
    let zoned = zone().to_zoned(dt).map_err(|e| format!("{field}: {e}"))?;
    Ok(zoned.timestamp().as_millisecond())
}

/// Start of a local day, in epoch milliseconds.
pub(crate) fn day_start(field: &str, d: Date) -> Result<i64, String> {
    zone()
        .to_zoned(d.to_datetime(Time::midnight()))
        .map(|z| z.timestamp().as_millisecond())
        .map_err(|e| format!("{field}: {e}"))
}

/// A date (start of day, or the end of that day for `end_of_day`) or a date-time, local.
pub(crate) fn parse_point(field: &str, s: &str, end_of_day: bool) -> Result<i64, String> {
    let s = s.trim();
    if s.len() == 10 {
        let d = parse_date(field, s)?;
        let d = if end_of_day {
            d.checked_add(1.day()).map_err(|e| e.to_string())?
        } else {
            d
        };
        return day_start(field, d);
    }
    parse_when(field, s)
}

/// `from` / `to` arguments as a millisecond window, today + 7 days when
/// omitted, refused when longer than `max_days`.
pub(crate) fn window(args: &Args, max_days: i64) -> Result<(i64, i64), String> {
    let from = match str_arg(args, "from") {
        Some(s) => parse_point("from", s, false)?,
        None => day_start("from", today())?,
    };
    let to = match str_arg(args, "to") {
        Some(s) => parse_point("to", s, true)?,
        None => day_start(
            "to",
            today().checked_add(8.days()).map_err(|e| e.to_string())?,
        )?,
    };
    if to <= from {
        return Err("to must be after from".to_owned());
    }
    if (to - from) / 86_400_000 > max_days {
        return Err(format!(
            "the range is longer than {max_days} days; ask for a shorter window"
        ));
    }
    Ok((from, to))
}

/// Epoch milliseconds → local "YYYY-MM-DDTHH:MM".
pub(crate) fn iso(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|t| t.to_zoned(zone()).strftime("%Y-%m-%dT%H:%M").to_string())
        .unwrap_or_default()
}

/// Epoch milliseconds → local "HH:MM".
pub(crate) fn hm(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|t| t.to_zoned(zone()).strftime("%H:%M").to_string())
        .unwrap_or_default()
}
