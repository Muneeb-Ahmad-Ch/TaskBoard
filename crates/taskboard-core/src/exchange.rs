//! Export / import (SPEC §5 Data safety): full JSON (round-trips exactly),
//! tasks CSV (export + best-effort import from Todoist / Trello / Notion /
//! generic CSV), and ICS (meetings as VEVENT, dated tasks as VTODO; VEVENT
//! import). Every import is one undoable action.

use std::collections::HashMap;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params_from_iter};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};
use ts_rs::TS;

use crate::error::{Result, invalid};
use crate::events::{EventKind, NewEvent, insert_event, local_zone};
use crate::id::Id;
use crate::model::{Category, NewProject, NewTask, Priority, text};
use crate::projects::insert_project;
use crate::query::{TaskQuery, TaskSort};
use crate::rrule::Rule;
use crate::storage::{Db, SCHEMA_VERSION};
use crate::tasks::insert_task;
use crate::tx::{Table, Tx};

/// Export order respects foreign keys (parents first).
const TABLES: [&str; 16] = [
    "client",
    "project",
    "status",
    "task",
    "tag",
    "task_tag",
    "calendar_feed",
    "event",
    "event_exception",
    "working_hours",
    "day_override",
    "reminder",
    "time_entry",
    "attachment",
    "saved_view",
    "setting",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ImportReport {
    pub created: i64,
    pub skipped: i64,
    pub projects_created: i64,
    pub warnings: Vec<String>,
}

fn columns(conn: &rusqlite::Connection, table: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(cols)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn to_json(v: Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Integer(i) => json!(i),
        Value::Real(f) => json!(f),
        Value::Text(s) => Json::String(s),
        Value::Blob(b) => match Id::from_slice(&b) {
            Some(id) if b.len() == 16 => Json::String(id.to_string()),
            _ => Json::String(format!("hex:{}", hex(&b))),
        },
    }
}

fn from_json(v: &Json, decl: &str) -> Result<Value> {
    Ok(match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Integer(i64::from(*b)),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or(0.0)),
        },
        Json::String(s) if decl.eq_ignore_ascii_case("BLOB") => {
            if let Some(h) = s.strip_prefix("hex:") {
                let bytes: Option<Vec<u8>> = (0..h.len())
                    .step_by(2)
                    .map(|i| h.get(i..i + 2).and_then(|p| u8::from_str_radix(p, 16).ok()))
                    .collect();
                Value::Blob(bytes.ok_or_else(|| invalid("bad hex value in import"))?)
            } else {
                let id: Id = s
                    .parse()
                    .map_err(|_| invalid(format!("bad id {s:?} in import")))?;
                id.into()
            }
        }
        Json::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    })
}

// ---------- CSV helpers ----------

fn header_index(headers: &[String], names: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)))
}

/// Accepts ISO dates/datetimes and a few common export formats.
fn parse_loose_date(s: &str) -> Option<Date> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(d) = s.get(..10).and_then(|p| p.parse::<Date>().ok()) {
        return Some(d);
    }
    if let Ok(ts) = s.parse::<Timestamp>() {
        let (tz, _) = local_zone();
        return Some(ts.to_zoned(tz).date());
    }
    for fmt in [
        "%B %d, %Y",
        "%b %d, %Y",
        "%d %b %Y",
        "%d %B %Y",
        "%m/%d/%Y",
        "%Y/%m/%d",
    ] {
        // Notion: "October 20, 2026 3:00 PM" → try the date part first.
        let first = s.split(['T']).next().unwrap_or(s);
        for candidate in [s, first] {
            if let Ok(d) = Date::strptime(fmt, candidate) {
                return Some(d);
            }
        }
        let words: Vec<&str> = s.split_whitespace().collect();
        for n in (2..=words.len().min(3)).rev() {
            if let Ok(d) = Date::strptime(fmt, words[..n].join(" ")) {
                return Some(d);
            }
        }
    }
    None
}

fn parse_priority(s: &str) -> Option<Priority> {
    Some(match s.trim().to_lowercase().as_str() {
        "urgent" | "p1" | "4" | "highest" | "critical" => Priority::Urgent,
        "high" | "p2" | "3" => Priority::High,
        "medium" | "med" | "normal" | "p3" | "2" => Priority::Med,
        "low" | "p4" | "1" | "lowest" => Priority::Low,
        _ => return None,
    })
}

fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_lowercase().as_str(),
        "true" | "yes" | "1" | "x" | "done" | "complete" | "completed" | "closed" | "archived"
    )
}

// ---------- ICS helpers ----------

fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn ics_unescape(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Folds a content line at 75 octets (RFC 5545 §3.1).
fn fold(line: &str, out: &mut String) {
    let mut count = 0;
    for ch in line.chars() {
        let len = ch.len_utf8();
        if count + len > 75 {
            out.push_str("\r\n ");
            count = 1;
        }
        out.push(ch);
        count += len;
    }
    out.push_str("\r\n");
}

fn ics_utc(ms: i64) -> String {
    Timestamp::from_millisecond(ms)
        .map(|t| t.strftime("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_default()
}

/// Parses `DTSTART`-style values. Returns (UTC ms, None) for timed values or
/// (_, Some(date)) for all-day dates.
fn parse_ics_time(value: &str, params: &HashMap<String, String>) -> Option<(i64, Option<Date>)> {
    let v = value.trim();
    if params
        .get("VALUE")
        .is_some_and(|p| p.eq_ignore_ascii_case("DATE"))
        || v.len() == 8
    {
        let d = Date::strptime("%Y%m%d", v).ok()?;
        return Some((0, Some(d)));
    }
    let (body, utc) = match v.strip_suffix('Z') {
        Some(b) => (b, true),
        None => (v, false),
    };
    let dt = DateTime::strptime("%Y%m%dT%H%M%S", body).ok()?;
    let tz = if utc {
        TimeZone::UTC
    } else {
        params
            .get("TZID")
            .and_then(|z| TimeZone::get(z.trim_matches('"')).ok())
            .unwrap_or_else(|| local_zone().0)
    };
    Some((tz.to_zoned(dt).ok()?.timestamp().as_millisecond(), None))
}

/// `PT1H30M`, `P1D` → minutes.
fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim().trim_start_matches('+');
    let body = s.strip_prefix('P')?;
    let (date_part, time_part) = body.split_once('T').unwrap_or((body, ""));
    let mut minutes = 0i64;
    let mut num = String::new();
    for (part, is_time) in [(date_part, false), (time_part, true)] {
        for c in part.chars() {
            if c.is_ascii_digit() {
                num.push(c);
                continue;
            }
            let n: i64 = num.parse().ok()?;
            num.clear();
            minutes += match (c, is_time) {
                ('W', false) => n * 7 * 24 * 60,
                ('D', false) => n * 24 * 60,
                ('H', true) => n * 60,
                ('M', true) => n,
                ('S', true) => n / 60,
                _ => return None,
            };
        }
    }
    Some(minutes)
}

impl Db {
    // ---------- JSON ----------

    /// Everything, as pretty JSON. Ids are UUID strings.
    pub fn export_json(&self) -> Result<String> {
        let mut tables = Map::new();
        for table in TABLES {
            let cols = columns(self.conn(), table)?;
            let mut stmt = self.conn().prepare(&format!("SELECT * FROM {table}"))?;
            let rows: Vec<Json> = stmt
                .query_map([], |r| {
                    let mut obj = Map::new();
                    for (i, (name, _)) in cols.iter().enumerate() {
                        obj.insert(name.clone(), to_json(r.get::<_, Value>(i)?));
                    }
                    Ok(Json::Object(obj))
                })?
                .collect::<rusqlite::Result<_>>()?;
            tables.insert(table.into(), Json::Array(rows));
        }
        let doc = json!({
            "app": "taskboard",
            "format": 1,
            "schema_version": SCHEMA_VERSION,
            "exported_at": crate::clock::now_ms(),
            "tables": tables,
        });
        Ok(serde_json::to_string_pretty(&doc)?)
    }

    /// Merges a JSON export: rows whose key already exists are skipped. The
    /// export's Inbox maps onto this database's Inbox.
    pub fn import_json(&mut self, input: &str) -> Result<ImportReport> {
        let doc: Json = serde_json::from_str(input)
            .map_err(|e| invalid(format!("not a Taskboard export: {e}")))?;
        if doc.get("app").and_then(Json::as_str) != Some("taskboard") {
            return Err(invalid("this file is not a Taskboard JSON export"));
        }
        if doc.get("format").and_then(Json::as_i64) != Some(1) {
            return Err(invalid("unsupported export format version"));
        }
        let tables = doc
            .get("tables")
            .and_then(Json::as_object)
            .ok_or_else(|| invalid("export has no tables"))?
            .clone();
        let local_inbox = self.inbox_id()?;
        let mut report = ImportReport::default();
        self.transact("Import from JSON", |tx| {
            tx.conn().execute_batch("PRAGMA defer_foreign_keys = ON")?;
            // The export's Inbox (and its statuses) map onto ours.
            let empty = Vec::new();
            let rows_of = |t: &str| {
                tables
                    .get(t)
                    .and_then(Json::as_array)
                    .unwrap_or(&empty)
                    .clone()
            };
            let their_inbox: Option<String> = rows_of("project")
                .iter()
                .find(|p| p.get("is_inbox").and_then(Json::as_i64) == Some(1))
                .and_then(|p| p.get("id").and_then(Json::as_str).map(str::to_owned));
            let mut status_map: HashMap<String, Id> = HashMap::new();
            if let Some(inbox) = &their_inbox {
                let ours: Vec<(Id, String)> = {
                    let mut stmt = tx.conn().prepare(
                        "SELECT id, name FROM status WHERE project_id = ?1 AND deleted_at IS NULL",
                    )?;
                    stmt.query_map([local_inbox], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for s in rows_of("status") {
                    if s.get("project_id").and_then(Json::as_str) == Some(inbox.as_str())
                        && let (Some(id), Some(name)) = (
                            s.get("id").and_then(Json::as_str),
                            s.get("name").and_then(Json::as_str),
                        )
                    {
                        let target = ours
                            .iter()
                            .find(|(_, n)| n.eq_ignore_ascii_case(name))
                            .or(ours.first());
                        if let Some((ours_id, _)) = target {
                            status_map.insert(id.to_owned(), *ours_id);
                        }
                    }
                }
            }
            let running_timer: bool = tx.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM time_entry WHERE ended_at IS NULL)",
                [],
                |r| r.get(0),
            )?;

            for table_name in TABLES {
                let Some(table) = Table::from_name(table_name) else {
                    continue;
                };
                let cols = columns(tx.conn(), table_name)?;
                let decl: HashMap<&str, &str> =
                    cols.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
                for row in rows_of(table_name) {
                    let Some(obj) = row.as_object() else { continue };
                    let mut obj = obj.clone();
                    // Inbox remapping.
                    if let Some(inbox) = &their_inbox {
                        if table_name == "project"
                            && obj.get("id").and_then(Json::as_str) == Some(inbox)
                        {
                            continue;
                        }
                        if table_name == "status"
                            && obj.get("project_id").and_then(Json::as_str) == Some(inbox)
                        {
                            continue;
                        }
                        if table_name == "task"
                            && obj.get("project_id").and_then(Json::as_str) == Some(inbox)
                        {
                            obj.insert("project_id".into(), Json::String(local_inbox.to_string()));
                            if let Some(s) = obj
                                .get("status_id")
                                .and_then(Json::as_str)
                                .and_then(|s| status_map.get(s))
                            {
                                obj.insert("status_id".into(), Json::String(s.to_string()));
                            }
                        }
                    }
                    if table_name == "time_entry"
                        && running_timer
                        && obj.get("ended_at").is_some_and(Json::is_null)
                    {
                        report.skipped += 1;
                        report
                            .warnings
                            .push("skipped a running timer (one is already running)".into());
                        continue;
                    }
                    let key: Vec<Value> = table
                        .key_cols()
                        .iter()
                        .map(|k| {
                            from_json(
                                obj.get(*k).unwrap_or(&Json::Null),
                                decl.get(k).copied().unwrap_or(""),
                            )
                        })
                        .collect::<Result<_>>()?;
                    let where_sql = table
                        .key_cols()
                        .iter()
                        .map(|k| format!("{k} = ?"))
                        .collect::<Vec<_>>()
                        .join(" AND ");
                    let exists: bool = tx.conn().query_row(
                        &format!("SELECT EXISTS(SELECT 1 FROM {table_name} WHERE {where_sql})"),
                        params_from_iter(key.iter()),
                        |r| r.get(0),
                    )?;
                    if exists {
                        report.skipped += 1;
                        continue;
                    }
                    let values: Vec<(String, Value)> = obj
                        .iter()
                        .filter(|(k, _)| decl.contains_key(k.as_str()))
                        .map(|(k, v)| {
                            Ok((
                                k.clone(),
                                from_json(v, decl.get(k.as_str()).copied().unwrap_or(""))?,
                            ))
                        })
                        .collect::<Result<_>>()?;
                    tx.insert(table, &values)?;
                    report.created += 1;
                }
            }
            Ok(())
        })?;
        Ok(report)
    }

    // ---------- CSV ----------

    /// Tasks matching `query` as CSV (all pages).
    pub fn export_tasks_csv(&self, query: &TaskQuery) -> Result<String> {
        let mut w = csv::Writer::from_writer(Vec::new());
        let csv_err = |e: csv::Error| invalid(e.to_string());
        w.write_record([
            "Title",
            "Client",
            "Project",
            "Status",
            "Priority",
            "Due date",
            "Due time",
            "Start date",
            "Estimate (min)",
            "Progress (%)",
            "Tags",
            "Completed",
            "Repeats",
        ])
        .map_err(csv_err)?;
        let fmt_ms = |ms: Option<i64>| -> String {
            let (tz, _) = local_zone();
            ms.and_then(|m| Timestamp::from_millisecond(m).ok())
                .map(|t| t.to_zoned(tz).strftime("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default()
        };
        let mut offset = 0;
        loop {
            let page = self.list_tasks(&TaskQuery {
                offset,
                limit: 1000,
                sort: TaskSort::Due,
                ..query.clone()
            })?;
            for r in &page.rows {
                let priority = match r.priority {
                    Priority::Urgent => "urgent",
                    Priority::High => "high",
                    Priority::Med => "medium",
                    Priority::Low => "low",
                    Priority::None => "",
                };
                w.write_record([
                    r.title.as_str(),
                    r.client_name.as_deref().unwrap_or(""),
                    r.project_name.as_str(),
                    r.status_name.as_str(),
                    priority,
                    r.due_date.as_deref().unwrap_or(""),
                    r.due_time.as_deref().unwrap_or(""),
                    r.start_date.as_deref().unwrap_or(""),
                    &r.estimate_minutes
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    &r.progress.to_string(),
                    &r.tags.join(", "),
                    &fmt_ms(r.completed_at),
                    if r.recurring { "yes" } else { "" },
                ])
                .map_err(csv_err)?;
            }
            offset += 1000;
            if offset >= page.total {
                break;
            }
        }
        let bytes = w.into_inner().map_err(|e| invalid(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| invalid(e.to_string()))
    }

    /// Best-effort task import from CSV exports (Todoist, Trello, Notion, or
    /// any CSV with a title column). Unknown projects are created (personal);
    /// rows without a project go to `default_project` or the Inbox.
    pub fn import_tasks_csv(
        &mut self,
        input: &str,
        default_project: Option<Id>,
    ) -> Result<ImportReport> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(input.trim_start_matches('\u{feff}').as_bytes());
        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| invalid(format!("could not read the CSV header: {e}")))?
            .iter()
            .map(str::to_owned)
            .collect();
        let col = |names: &[&str]| header_index(&headers, names);
        let title_col = col(&[
            "title",
            "task",
            "task name",
            "name",
            "content",
            "card name",
            "summary",
        ])
        .ok_or_else(|| {
            invalid("no title column (looked for Title, Task, Name, Content, Card Name)")
        })?;
        let desc_col = col(&["description", "notes", "card description", "desc", "note"]);
        let project_col = col(&[
            "project",
            "project name",
            "list",
            "list name",
            "board",
            "section",
        ]);
        let due_col = col(&["due", "due date", "date", "deadline", "due_date", "due on"]);
        let prio_col = col(&["priority", "prio"]);
        let tags_col = col(&["tags", "labels", "label", "tag"]);
        let done_col = col(&[
            "completed",
            "done",
            "is completed",
            "closed",
            "status",
            "archived",
        ]);
        let type_col = col(&["type"]);
        let records: Vec<csv::StringRecord> = reader
            .records()
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| invalid(format!("CSV error: {e}")))?;
        let inbox = self.inbox_id()?;
        let fallback = default_project.unwrap_or(inbox);
        let mut report = ImportReport::default();
        self.transact("Import tasks from CSV", |tx| {
            let mut projects: HashMap<String, Id> = HashMap::new();
            for (n, rec) in records.iter().enumerate() {
                let get = |i: Option<usize>| i.and_then(|i| rec.get(i)).unwrap_or("").trim();
                let title = get(Some(title_col));
                if title.is_empty() {
                    report.skipped += 1;
                    continue;
                }
                // Todoist exports sections and notes as rows too.
                if type_col.is_some() && !matches!(get(type_col).to_lowercase().as_str(), "" | "task") {
                    report.skipped += 1;
                    continue;
                }
                let project = match get(project_col) {
                    "" => fallback,
                    name => match projects.get(&name.to_lowercase()) {
                        Some(id) => *id,
                        None => {
                            let found: Option<Id> = tx
                                .conn()
                                .query_row(
                                    "SELECT id FROM project WHERE name = ?1 COLLATE NOCASE AND deleted_at IS NULL LIMIT 1",
                                    [name],
                                    |r| r.get(0),
                                )
                                .optional()?;
                            let id = match found {
                                Some(id) => id,
                                None => {
                                    report.projects_created += 1;
                                    insert_project(tx, &NewProject { name: name.into(), ..Default::default() })?
                                }
                            };
                            projects.insert(name.to_lowercase(), id);
                            id
                        }
                    },
                };
                let due_raw = get(due_col);
                let due = parse_loose_date(due_raw);
                if !due_raw.is_empty() && due.is_none() {
                    report.warnings.push(format!("row {}: could not read date {due_raw:?}", n + 2));
                }
                let tags: Vec<String> = get(tags_col)
                    .split([',', ';'])
                    .map(|t| t.trim().trim_start_matches('@').trim_start_matches('#').to_owned())
                    .filter(|t| !t.is_empty())
                    .collect();
                let done_raw = get(done_col);
                let done = truthy(done_raw);
                let id = insert_task(
                    tx,
                    &NewTask {
                        title: title.chars().take(300).collect(),
                        project_id: Some(project),
                        description_md: Some(get(desc_col).to_owned()).filter(|d| !d.is_empty()),
                        priority: parse_priority(get(prio_col)),
                        due_date: due.map(|d| d.to_string()),
                        tags,
                        ..Default::default()
                    },
                    inbox,
                )?;
                if done {
                    complete_in_tx(tx, id, project)?;
                }
                report.created += 1;
            }
            Ok(())
        })?;
        Ok(report)
    }

    // ---------- ICS ----------

    /// Meetings/blocks as VEVENT (with RRULE and cancelled dates), open dated
    /// tasks as VTODO.
    pub fn export_ics(&self) -> Result<String> {
        let mut out = String::new();
        for line in [
            "BEGIN:VCALENDAR",
            "VERSION:2.0",
            "PRODID:-//Taskboard//Taskboard//EN",
            "CALSCALE:GREGORIAN",
        ] {
            fold(line, &mut out);
        }
        let stamp = ics_utc(crate::clock::now_ms());
        let ids: Vec<Id> = {
            let mut stmt = self
                .conn()
                .prepare("SELECT id FROM event WHERE deleted_at IS NULL ORDER BY start_at")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in ids {
            let e = self.event(id)?;
            fold("BEGIN:VEVENT", &mut out);
            fold(&format!("UID:{id}@taskboard"), &mut out);
            fold(&format!("DTSTAMP:{stamp}"), &mut out);
            fold(&format!("SUMMARY:{}", ics_escape(&e.title)), &mut out);
            match (e.start_at, e.end_at, &e.all_day_start, &e.all_day_end) {
                (Some(s), Some(en), _, _) => {
                    fold(&format!("DTSTART:{}", ics_utc(s)), &mut out);
                    fold(&format!("DTEND:{}", ics_utc(en)), &mut out);
                }
                (_, _, Some(a), Some(b)) => {
                    let end = b.parse::<Date>().ok().and_then(|d| d.tomorrow().ok());
                    fold(
                        &format!("DTSTART;VALUE=DATE:{}", a.replace('-', "")),
                        &mut out,
                    );
                    if let Some(end) = end {
                        fold(
                            &format!("DTEND;VALUE=DATE:{}", end.strftime("%Y%m%d")),
                            &mut out,
                        );
                    }
                }
                _ => {}
            }
            if let Some(rule) = &e.recurrence_rule {
                fold(&format!("RRULE:{rule}"), &mut out);
                let cancelled: Vec<i64> = {
                    let mut stmt = self
                        .conn()
                        .prepare("SELECT original_start FROM event_exception WHERE event_id = ?1 AND cancelled = 1")?;
                    stmt.query_map([id], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for c in cancelled {
                    fold(&format!("EXDATE:{}", ics_utc(c)), &mut out);
                }
            }
            if !e.location.is_empty() {
                fold(&format!("LOCATION:{}", ics_escape(&e.location)), &mut out);
            }
            if !e.meeting_url.is_empty() {
                fold(&format!("URL:{}", e.meeting_url), &mut out);
            }
            if !e.notes_md.is_empty() {
                fold(
                    &format!("DESCRIPTION:{}", ics_escape(&e.notes_md)),
                    &mut out,
                );
            }
            match e.status {
                crate::events::EventStatus::Tentative => fold("STATUS:TENTATIVE", &mut out),
                crate::events::EventStatus::Cancelled => fold("STATUS:CANCELLED", &mut out),
                crate::events::EventStatus::Confirmed => fold("STATUS:CONFIRMED", &mut out),
            }
            if e.kind == EventKind::Block {
                fold("TRANSP:OPAQUE", &mut out);
                fold("CATEGORIES:Time block", &mut out);
            }
            fold("END:VEVENT", &mut out);
        }
        let mut offset = 0;
        loop {
            let page = self.list_tasks(&TaskQuery {
                categories: vec![Category::Todo, Category::Active],
                due_from: Some("0000-01-01".into()),
                offset,
                limit: 1000,
                ..Default::default()
            })?;
            for t in &page.rows {
                let Some(due) = &t.due_date else { continue };
                fold("BEGIN:VTODO", &mut out);
                fold(&format!("UID:{}@taskboard", t.id), &mut out);
                fold(&format!("DTSTAMP:{stamp}"), &mut out);
                fold(&format!("SUMMARY:{}", ics_escape(&t.title)), &mut out);
                match &t.due_time {
                    Some(time) => fold(
                        &format!("DUE:{}T{}00", due.replace('-', ""), time.replace(':', "")),
                        &mut out,
                    ),
                    None => fold(
                        &format!("DUE;VALUE=DATE:{}", due.replace('-', "")),
                        &mut out,
                    ),
                }
                let prio = match t.priority {
                    Priority::Urgent => 1,
                    Priority::High => 3,
                    Priority::Med => 5,
                    Priority::Low => 7,
                    Priority::None => 0,
                };
                if prio > 0 {
                    fold(&format!("PRIORITY:{prio}"), &mut out);
                }
                fold("STATUS:NEEDS-ACTION", &mut out);
                fold("END:VTODO", &mut out);
            }
            offset += 1000;
            if offset >= page.total {
                break;
            }
        }
        fold("END:VCALENDAR", &mut out);
        Ok(out)
    }

    /// Imports VEVENTs as meetings. Events already imported (same UID) are
    /// skipped; unsupported repeat rules import the first occurrence only.
    pub fn import_ics(&mut self, input: &str) -> Result<ImportReport> {
        // Unfold continuation lines.
        let mut lines: Vec<String> = Vec::new();
        for raw in input.split('\n') {
            let raw = raw.trim_end_matches('\r');
            if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
                if let Some(last) = lines.last_mut() {
                    last.push_str(&raw[1..]);
                }
            } else {
                lines.push(raw.to_owned());
            }
        }
        if !lines
            .iter()
            .any(|l| l.trim().eq_ignore_ascii_case("BEGIN:VCALENDAR"))
        {
            return Err(invalid("this file is not an iCalendar (.ics) file"));
        }
        type Props = Vec<(String, HashMap<String, String>, String)>;
        let mut events: Vec<Props> = Vec::new();
        let mut current: Option<Props> = None;
        for line in lines {
            let Some((head, value)) = line.split_once(':') else {
                continue;
            };
            let mut parts = head.split(';');
            let name = parts.next().unwrap_or("").to_ascii_uppercase();
            let params: HashMap<String, String> = parts
                .filter_map(|p| {
                    p.split_once('=')
                        .map(|(k, v)| (k.to_ascii_uppercase(), v.to_owned()))
                })
                .collect();
            match (name.as_str(), value.trim()) {
                ("BEGIN", "VEVENT") => current = Some(Vec::new()),
                ("END", "VEVENT") => events.extend(current.take()),
                _ => {
                    if let Some(c) = current.as_mut() {
                        c.push((name, params, value.to_owned()));
                    }
                }
            }
        }
        let mut report = ImportReport::default();
        self.transact("Import calendar", |tx| {
            for props in &events {
                let prop = |n: &str| props.iter().find(|(k, ..)| k == n);
                let title = prop("SUMMARY")
                    .map(|(_, _, v)| ics_unescape(v))
                    .unwrap_or_else(|| "Untitled event".into());
                let uid = prop("UID").map(|(_, _, v)| v.clone());
                if let Some(uid) = &uid {
                    // Our own exports use `<event id>@taskboard`: those events
                    // may already be here (same database, or a JSON restore).
                    let own: Option<Id> = uid
                        .strip_suffix("@taskboard")
                        .and_then(|id| id.parse().ok());
                    let seen: bool = tx.conn().query_row(
                        "SELECT EXISTS(SELECT 1 FROM event WHERE deleted_at IS NULL
                           AND (external_uid = ?1 OR id = ?2))",
                        rusqlite::params![uid, own],
                        |r| r.get(0),
                    )?;
                    if seen {
                        report.skipped += 1;
                        continue;
                    }
                }
                let Some((start_ms, start_day)) =
                    prop("DTSTART").and_then(|(_, p, v)| parse_ics_time(v, p))
                else {
                    report.skipped += 1;
                    report.warnings.push(format!("“{title}”: no start time"));
                    continue;
                };
                if prop("STATUS").is_some_and(|(_, _, v)| v.eq_ignore_ascii_case("CANCELLED")) {
                    report.skipped += 1;
                    continue;
                }
                let end = prop("DTEND").and_then(|(_, p, v)| parse_ics_time(v, p));
                let duration = prop("DURATION").and_then(|(_, _, v)| parse_duration(v));
                let rule = prop("RRULE").map(|(_, _, v)| v.clone());
                let rule = match rule {
                    Some(r) => match Rule::parse(&r) {
                        Ok(r) => Some(r.to_string()),
                        Err(_) => {
                            report.warnings.push(format!(
                                "“{title}”: repeat rule not supported, imported once"
                            ));
                            None
                        }
                    },
                    None => None,
                };
                let mut input = NewEvent {
                    kind: EventKind::Meeting,
                    title: title.chars().take(300).collect(),
                    client_id: None,
                    project_id: None,
                    task_id: None,
                    start_at: None,
                    end_at: None,
                    all_day_start: None,
                    all_day_end: None,
                    time_zone: None,
                    location: prop("LOCATION").map(|(_, _, v)| ics_unescape(v)),
                    meeting_url: prop("URL")
                        .map(|(_, _, v)| v.trim().to_owned())
                        .filter(|u| u.starts_with("http")),
                    attendees: None,
                    notes_md: prop("DESCRIPTION").map(|(_, _, v)| ics_unescape(v)),
                    status: None,
                    counts_as_time: None,
                    flexible: None,
                    recurrence_rule: rule,
                };
                match start_day {
                    Some(day) => {
                        let last = match end {
                            // DTEND is exclusive for all-day events.
                            Some((_, Some(e))) if e > day => e.yesterday().unwrap_or(day),
                            _ => day,
                        };
                        input.all_day_start = Some(day.to_string());
                        input.all_day_end = Some(last.to_string());
                    }
                    None => {
                        let end_ms = match (end, duration) {
                            (Some((ms, None)), _) => ms,
                            (_, Some(min)) => start_ms + min * 60_000,
                            _ => start_ms + 3_600_000,
                        };
                        input.start_at = Some(start_ms);
                        input.end_at = Some(end_ms.max(start_ms + 60_000));
                    }
                }
                match insert_event(tx, &input) {
                    Ok(id) => {
                        if let Some(uid) = &uid {
                            tx.update(
                                Table::Event,
                                &[id.into()],
                                &[("external_uid", text(uid.clone()))],
                            )?;
                        }
                        // Cancelled occurrences of a repeating series.
                        if input.recurrence_rule.is_some() {
                            for (_, p, v) in props.iter().filter(|(k, ..)| k == "EXDATE") {
                                for one in v.split(',') {
                                    if let Some((ms, None)) = parse_ics_time(one, p) {
                                        tx.insert(
                                            Table::EventException,
                                            &[
                                                ("event_id", id.into()),
                                                ("original_start", Value::Integer(ms)),
                                                ("cancelled", Value::Integer(1)),
                                            ],
                                        )?;
                                    }
                                }
                            }
                        }
                        report.created += 1;
                    }
                    Err(err) => {
                        report.skipped += 1;
                        report.warnings.push(format!("“{title}”: {err}"));
                    }
                }
            }
            Ok(())
        })?;
        Ok(report)
    }
}

/// Marks an imported task done (first Done column of its project).
fn complete_in_tx(tx: &mut Tx<'_>, task: Id, project: Id) -> Result<()> {
    let done: Option<Id> = tx
        .conn()
        .query_row(
            "SELECT id FROM status WHERE project_id = ?1 AND category = 'done' AND deleted_at IS NULL
             ORDER BY sort_key LIMIT 1",
            [project],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(done) = done {
        let now = tx.now();
        tx.update(
            Table::Task,
            &[task.into()],
            &[
                ("status_id", done.into()),
                ("completed_at", Value::Integer(now)),
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn loose_dates() {
        assert_eq!(
            parse_loose_date("2026-10-20").unwrap().to_string(),
            "2026-10-20"
        );
        assert_eq!(
            parse_loose_date("2026-10-20T15:00:00Z").map(|d| d.year()),
            Some(2026)
        );
        assert_eq!(
            parse_loose_date("October 20, 2026").unwrap().to_string(),
            "2026-10-20"
        );
        assert_eq!(
            parse_loose_date("October 20, 2026 3:00 PM")
                .unwrap()
                .to_string(),
            "2026-10-20"
        );
        assert_eq!(
            parse_loose_date("20 Oct 2026").unwrap().to_string(),
            "2026-10-20"
        );
        assert_eq!(
            parse_loose_date("10/20/2026").unwrap().to_string(),
            "2026-10-20"
        );
        assert!(parse_loose_date("someday").is_none());
    }

    #[test]
    fn ics_bits() {
        assert_eq!(parse_duration("PT1H30M"), Some(90));
        assert_eq!(parse_duration("P1D"), Some(1440));
        assert_eq!(ics_unescape(&ics_escape("a, b; c\\d\ne")), "a, b; c\\d\ne");
        let mut out = String::new();
        fold(&"x".repeat(160), &mut out);
        assert!(out.split("\r\n").all(|l| l.len() <= 75));
    }
}
