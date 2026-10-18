//! Meetings and time blocks (SPEC §5 "Meetings", "Working hours and time
//! planning"). Events are anchored instants plus an IANA zone; recurring
//! events expand on the fly for the requested range, with per-occurrence
//! exceptions.

use std::collections::HashMap;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{
    boolean, clean_date, clean_name, clean_tz, int, nullable, opt_id, opt_int, opt_text, text,
};
use crate::rrule::Rule;
use crate::storage::Db;
use crate::tx::{Table, Tx};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum EventKind {
    Meeting,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Meeting => "meeting",
            Self::Block => "block",
        }
    }
    fn parse(s: &str) -> Self {
        if s == "block" {
            Self::Block
        } else {
            Self::Meeting
        }
    }
}

impl EventStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Tentative => "tentative",
            Self::Cancelled => "cancelled",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "tentative" => Self::Tentative,
            "cancelled" => Self::Cancelled,
            _ => Self::Confirmed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Event {
    pub id: Id,
    pub kind: EventKind,
    pub title: String,
    pub client_id: Option<Id>,
    pub project_id: Option<Id>,
    pub task_id: Option<Id>,
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
    pub time_zone: String,
    pub all_day_start: Option<String>,
    pub all_day_end: Option<String>,
    pub location: String,
    pub meeting_url: String,
    pub attendees: String,
    pub notes_md: String,
    pub status: EventStatus,
    pub counts_as_time: bool,
    pub flexible: bool,
    pub recurrence_rule: Option<String>,
    /// Human description of the repeat rule, e.g. "Every week on Mon".
    pub recurrence_text: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewEvent {
    pub kind: EventKind,
    pub title: String,
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub task_id: Option<Id>,
    /// Timed events: UTC ms.
    #[serde(default)]
    #[ts(optional)]
    pub start_at: Option<i64>,
    #[serde(default)]
    #[ts(optional)]
    pub end_at: Option<i64>,
    /// All-day events: civil dates, end inclusive.
    #[serde(default)]
    #[ts(optional)]
    pub all_day_start: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub all_day_end: Option<String>,
    /// Defaults to the system time zone.
    #[serde(default)]
    #[ts(optional)]
    pub time_zone: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub meeting_url: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub attendees: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub notes_md: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<EventStatus>,
    /// Defaults to the client's "meetings count as work time" setting.
    #[serde(default)]
    #[ts(optional)]
    pub counts_as_time: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub flexible: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub recurrence_rule: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct EventPatch {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub kind: Option<EventKind>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub client_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub project_id: Option<Option<Id>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub task_id: Option<Option<Id>>,
    #[ts(optional)]
    pub start_at: Option<i64>,
    #[ts(optional)]
    pub end_at: Option<i64>,
    #[ts(optional)]
    pub all_day_start: Option<String>,
    #[ts(optional)]
    pub all_day_end: Option<String>,
    #[ts(optional)]
    pub location: Option<String>,
    #[ts(optional)]
    pub meeting_url: Option<String>,
    #[ts(optional)]
    pub attendees: Option<String>,
    #[ts(optional)]
    pub notes_md: Option<String>,
    #[ts(optional)]
    pub status: Option<EventStatus>,
    #[ts(optional)]
    pub counts_as_time: Option<bool>,
    #[ts(optional)]
    pub flexible: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub recurrence_rule: Option<Option<String>>,
}

/// One concrete appearance of an event in a range (a recurring series yields many).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Occurrence {
    pub event_id: Id,
    /// Series start of this occurrence (key for exceptions); equals `start_at`
    /// unless the occurrence was moved.
    pub original_start: i64,
    pub start_at: i64,
    pub end_at: i64,
    pub all_day: bool,
    pub kind: EventKind,
    pub title: String,
    pub status: EventStatus,
    pub client_id: Option<Id>,
    pub client_name: Option<String>,
    pub project_id: Option<Id>,
    pub project_name: Option<String>,
    /// The task a time block is for, if any.
    pub task_id: Option<Id>,
    /// Project color, else client color, else "gray".
    pub color: String,
    pub location: String,
    pub meeting_url: String,
    pub counts_as_time: bool,
    pub flexible: bool,
    pub recurring: bool,
    /// This occurrence was individually edited.
    pub modified: bool,
}

/// Changes to a single occurrence of a recurring event.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct OccurrencePatch {
    #[ts(optional)]
    pub start_at: Option<i64>,
    #[ts(optional)]
    pub end_at: Option<i64>,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub notes_md: Option<String>,
    #[ts(optional)]
    pub cancelled: Option<bool>,
}

/// (cancelled, start override, end override, title override) for one occurrence.
type ExceptionRow = (bool, Option<i64>, Option<i64>, Option<String>);

const COLS: &str = "id, kind, title, client_id, project_id, task_id, start_at, end_at, time_zone,
                    all_day_start, all_day_end, location, meeting_url, attendees, notes_md, status,
                    counts_as_time, flexible, recurrence_rule, created_at, updated_at";

fn map(r: &Row<'_>) -> rusqlite::Result<Event> {
    let rule: Option<String> = r.get(18)?;
    Ok(Event {
        id: r.get(0)?,
        kind: EventKind::parse(&r.get::<_, String>(1)?),
        title: r.get(2)?,
        client_id: r.get(3)?,
        project_id: r.get(4)?,
        task_id: r.get(5)?,
        start_at: r.get(6)?,
        end_at: r.get(7)?,
        time_zone: r.get(8)?,
        all_day_start: r.get(9)?,
        all_day_end: r.get(10)?,
        location: r.get(11)?,
        meeting_url: r.get(12)?,
        attendees: r.get(13)?,
        notes_md: r.get(14)?,
        status: EventStatus::parse(&r.get::<_, String>(15)?),
        counts_as_time: r.get(16)?,
        flexible: r.get(17)?,
        recurrence_text: rule
            .as_deref()
            .and_then(|s| Rule::parse(s).ok())
            .map(|r| r.describe()),
        recurrence_rule: rule,
        created_at: r.get(19)?,
        updated_at: r.get(20)?,
    })
}

/// The system time zone and its IANA name ("UTC" if unknown).
pub fn local_zone() -> (TimeZone, String) {
    let tz = TimeZone::system();
    let name = tz.iana_name().unwrap_or("UTC").to_owned();
    (tz, name)
}

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap_or(TimeZone::UTC)
}

const MAX_TIMED_MS: i64 = 14 * 86_400_000;

fn check_times(
    start: Option<i64>,
    end: Option<i64>,
    day_start: Option<&str>,
    day_end: Option<&str>,
) -> Result<()> {
    match (start, end, day_start, day_end) {
        (Some(s), Some(e), None, None) => {
            if e <= s {
                return Err(invalid("an event must end after it starts"));
            }
            if e - s > MAX_TIMED_MS {
                return Err(invalid("a timed event can last at most 14 days"));
            }
            Ok(())
        }
        (None, None, Some(a), Some(b)) => {
            if b < a {
                return Err(invalid(
                    "an all-day event must end on or after its first day",
                ));
            }
            Ok(())
        }
        _ => Err(invalid(
            "give either a start and end time, or all-day dates",
        )),
    }
}

fn load(tx: &Tx<'_>, id: Id) -> Result<Event> {
    tx.conn()
        .query_row(
            &format!("SELECT {COLS} FROM event WHERE id = ?1 AND deleted_at IS NULL"),
            [id],
            map,
        )
        .optional()?
        .ok_or(Error::NotFound("event"))
}

/// Project implies its client; returns the client to store.
fn resolve_owner(tx: &Tx<'_>, client: Option<Id>, project: Option<Id>) -> Result<Option<Id>> {
    match project {
        Some(p) => {
            let owner: Option<Option<Id>> = tx
                .conn()
                .query_row(
                    "SELECT client_id FROM project WHERE id = ?1 AND deleted_at IS NULL",
                    [p],
                    |r| r.get(0),
                )
                .optional()?;
            owner.ok_or(Error::NotFound("project"))
        }
        None => {
            if let Some(c) = client {
                let ok: bool = tx.conn().query_row(
                    "SELECT EXISTS(SELECT 1 FROM client WHERE id = ?1 AND deleted_at IS NULL)",
                    [c],
                    |r| r.get(0),
                )?;
                if !ok {
                    return Err(Error::NotFound("client"));
                }
            }
            Ok(client)
        }
    }
}

fn clean_rule(rule: Option<&str>) -> Result<Option<String>> {
    rule.map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|r| Rule::parse(r).map(|r| r.to_string()))
        .transpose()
}

fn ms_to_date(ms: i64, tz: &TimeZone) -> Option<Date> {
    Timestamp::from_millisecond(ms)
        .ok()
        .map(|t| t.to_zoned(tz.clone()).date())
}

fn date_start_ms(d: Date, tz: &TimeZone) -> Option<i64> {
    tz.to_zoned(d.to_datetime(jiff::civil::Time::midnight()))
        .ok()
        .map(|z| z.timestamp().as_millisecond())
}

/// Creates a meeting or time block inside an existing transaction.
pub(crate) fn insert_event(tx: &mut Tx<'_>, input: &NewEvent) -> Result<Id> {
    let title = clean_name(&input.title, "event title")?;
    let day_start = input.all_day_start.as_deref().map(clean_date).transpose()?;
    let day_end = match (&input.all_day_end, &day_start) {
        (Some(e), _) => Some(clean_date(e)?),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    };
    check_times(
        input.start_at,
        input.end_at,
        day_start.as_deref(),
        day_end.as_deref(),
    )?;
    let rule = clean_rule(input.recurrence_rule.as_deref())?;
    let tz_name = match &input.time_zone {
        Some(tz) => clean_tz(tz)?,
        None => local_zone().1,
    };
    let id = Id::new();

    let client = resolve_owner(tx, input.client_id, input.project_id)?;
    let default_counts = match (input.kind, client) {
        (EventKind::Meeting, Some(c)) => tx.conn().query_row(
            "SELECT meetings_count_as_time FROM client WHERE id = ?1",
            [c],
            |r| r.get::<_, bool>(0),
        )?,
        _ => false,
    };
    tx.insert(
        Table::Event,
        &[
            ("id", id.into()),
            ("kind", text(input.kind.as_str())),
            ("title", text(title.clone())),
            ("client_id", opt_id(client)),
            ("project_id", opt_id(input.project_id)),
            ("task_id", opt_id(input.task_id)),
            ("start_at", opt_int(input.start_at)),
            ("end_at", opt_int(input.end_at)),
            ("time_zone", text(tz_name.clone())),
            ("all_day_start", opt_text(day_start.clone())),
            ("all_day_end", opt_text(day_end.clone())),
            ("location", text(input.location.clone().unwrap_or_default())),
            (
                "meeting_url",
                text(input.meeting_url.clone().unwrap_or_default().trim()),
            ),
            (
                "attendees",
                text(input.attendees.clone().unwrap_or_default()),
            ),
            ("notes_md", text(input.notes_md.clone().unwrap_or_default())),
            (
                "status",
                text(input.status.unwrap_or(EventStatus::Confirmed).as_str()),
            ),
            (
                "counts_as_time",
                boolean(input.counts_as_time.unwrap_or(default_counts)),
            ),
            (
                "flexible",
                boolean(input.flexible.unwrap_or(input.kind == EventKind::Block)),
            ),
            ("recurrence_rule", opt_text(rule.clone())),
            ("created_at", int(tx.now())),
        ],
    )?;
    Ok(id)
}

impl Db {
    pub fn event(&self, id: Id) -> Result<Event> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM event WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("event"))
    }

    pub fn create_event(&mut self, input: NewEvent) -> Result<Event> {
        let title = clean_name(&input.title, "event title")?;
        let label = match input.kind {
            EventKind::Meeting => "Add meeting",
            EventKind::Block => "Add time block",
        };
        let id = self.transact(&format!("{label} “{title}”"), |tx| {
            insert_event(tx, &input)
        })?;
        self.event(id)
    }

    pub fn update_event(&mut self, id: Id, patch: EventPatch) -> Result<Event> {
        let current = self.event(id)?;
        self.transact(&format!("Edit “{}”", current.title), |tx| {
            let mut changes: Vec<(&str, Value)> = Vec::new();
            if let Some(t) = &patch.title {
                changes.push(("title", text(clean_name(t, "event title")?)));
            }
            if let Some(k) = patch.kind {
                changes.push(("kind", text(k.as_str())));
            }
            let project = patch.project_id.unwrap_or(current.project_id);
            let client_in = patch.client_id.unwrap_or(current.client_id);
            if patch.project_id.is_some() || patch.client_id.is_some() {
                let client = resolve_owner(tx, client_in, project)?;
                changes.push(("project_id", opt_id(project)));
                changes.push(("client_id", opt_id(client)));
            }
            if let Some(t) = patch.task_id {
                changes.push(("task_id", opt_id(t)));
            }
            let start = patch.start_at.or(current.start_at);
            let end = patch.end_at.or(current.end_at);
            let day_start = patch
                .all_day_start
                .as_deref()
                .map(clean_date)
                .transpose()?
                .or(current.all_day_start.clone());
            let day_end = patch
                .all_day_end
                .as_deref()
                .map(clean_date)
                .transpose()?
                .or(current.all_day_end.clone());
            // Switching between timed and all-day clears the other form.
            let (start, end, day_start, day_end) =
                if patch.start_at.is_some() || patch.end_at.is_some() {
                    (start, end, None, None)
                } else if patch.all_day_start.is_some() || patch.all_day_end.is_some() {
                    (None, None, day_start.clone(), day_end.or(day_start))
                } else {
                    (start, end, day_start, day_end)
                };
            check_times(start, end, day_start.as_deref(), day_end.as_deref())?;
            changes.push(("start_at", opt_int(start)));
            changes.push(("end_at", opt_int(end)));
            changes.push(("all_day_start", opt_text(day_start)));
            changes.push(("all_day_end", opt_text(day_end)));
            for (col, value) in [
                ("location", &patch.location),
                ("meeting_url", &patch.meeting_url),
                ("attendees", &patch.attendees),
                ("notes_md", &patch.notes_md),
            ] {
                if let Some(v) = value {
                    changes.push((col, text(v.trim_end())));
                }
            }
            if let Some(s) = patch.status {
                changes.push(("status", text(s.as_str())));
            }
            if let Some(b) = patch.counts_as_time {
                changes.push(("counts_as_time", boolean(b)));
            }
            if let Some(b) = patch.flexible {
                changes.push(("flexible", boolean(b)));
            }
            if let Some(rule) = &patch.recurrence_rule {
                changes.push(("recurrence_rule", opt_text(clean_rule(rule.as_deref())?)));
            }
            tx.update(Table::Event, &[id.into()], &changes)?;
            Ok(())
        })?;
        self.event(id)
    }

    /// Deletes an event (the whole series for a recurring one).
    pub fn delete_event(&mut self, id: Id) -> Result<()> {
        let current = self.event(id)?;
        self.transact(&format!("Delete “{}”", current.title), |tx| {
            let now = tx.now();
            tx.update(Table::Event, &[id.into()], &[("deleted_at", int(now))])?;
            Ok(())
        })
    }

    /// Moves, retitles or cancels one occurrence of a recurring event.
    pub fn edit_occurrence(
        &mut self,
        id: Id,
        original_start: i64,
        patch: OccurrencePatch,
    ) -> Result<()> {
        let current = self.event(id)?;
        if current.recurrence_rule.is_none() {
            return Err(invalid("this event does not repeat; edit it directly"));
        }
        if let (Some(s), Some(e)) = (patch.start_at, patch.end_at) {
            check_times(Some(s), Some(e), None, None)?;
        }
        let label = if patch.cancelled == Some(true) {
            "Cancel"
        } else {
            "Edit"
        };
        self.transact(&format!("{label} one “{}”", current.title), |tx| {
            let key = [Value::from(id), Value::Integer(original_start)];
            let exists: bool = tx.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM event_exception WHERE event_id = ?1 AND original_start = ?2)",
                params![id, original_start],
                |r| r.get(0),
            )?;
            let mut cols: Vec<(&str, Value)> = Vec::new();
            if let Some(c) = patch.cancelled {
                cols.push(("cancelled", boolean(c)));
            }
            if let Some(s) = patch.start_at {
                cols.push(("start_at", int(s)));
            }
            if let Some(e) = patch.end_at {
                cols.push(("end_at", int(e)));
            }
            if let Some(t) = &patch.title {
                cols.push(("title", text(clean_name(t, "event title")?)));
            }
            if let Some(n) = &patch.notes_md {
                cols.push(("notes_md", text(n.clone())));
            }
            if exists {
                tx.update(Table::EventException, &key, &cols)?;
            } else {
                cols.push(("event_id", id.into()));
                cols.push(("original_start", int(original_start)));
                tx.insert(Table::EventException, &cols)?;
            }
            Ok(())
        })
    }

    /// "This and following": ends the series before `original_start` and
    /// starts a new series there with `patch` applied. Returns the new series.
    pub fn split_series(
        &mut self,
        id: Id,
        original_start: i64,
        patch: EventPatch,
    ) -> Result<Event> {
        let current = self.event(id)?;
        let rule_str = current
            .recurrence_rule
            .clone()
            .ok_or_else(|| invalid("this event does not repeat"))?;
        let (Some(start), Some(end)) = (current.start_at, current.end_at) else {
            return Err(invalid("only timed series can be split"));
        };
        if original_start <= start {
            // Splitting at the first occurrence is just editing the series.
            return self.update_event(id, patch);
        }
        let tz = zone(&current.time_zone);
        let mut old_rule = Rule::parse(&rule_str)?;
        let mut new_rule = old_rule.clone();
        let split_date =
            ms_to_date(original_start, &tz).ok_or_else(|| invalid("bad occurrence time"))?;
        let dtstart = Timestamp::from_millisecond(start)
            .map_err(|_| invalid("bad start"))?
            .to_zoned(tz.clone())
            .datetime();
        if let Some(count) = old_rule.count {
            let before = old_rule.dates(dtstart.date(), |d| d < split_date).len() as u32;
            old_rule.count = Some(before.max(1));
            new_rule.count = Some(count.saturating_sub(before).max(1));
        } else {
            old_rule.until = split_date.checked_sub(1.day()).ok();
        }
        let new_id = Id::new();
        let duration = end - start;
        self.transact(&format!("Edit following “{}”", current.title), |tx| {
            tx.update(Table::Event, &[id.into()], &[("recurrence_rule", text(old_rule.to_string()))])?;
            let base = load(tx, id)?;
            let start_at = patch.start_at.unwrap_or(original_start);
            let end_at = patch.end_at.unwrap_or(start_at + duration);
            check_times(Some(start_at), Some(end_at), None, None)?;
            let project = patch.project_id.unwrap_or(base.project_id);
            let client = resolve_owner(tx, patch.client_id.unwrap_or(base.client_id), project)?;
            let rule = match &patch.recurrence_rule {
                Some(r) => clean_rule(r.as_deref())?,
                None => Some(new_rule.to_string()),
            };
            tx.insert(
                Table::Event,
                &[
                    ("id", new_id.into()),
                    ("kind", text(patch.kind.unwrap_or(base.kind).as_str())),
                    ("title", text(match &patch.title {
                        Some(t) => clean_name(t, "event title")?,
                        None => base.title.clone(),
                    })),
                    ("client_id", opt_id(client)),
                    ("project_id", opt_id(project)),
                    ("task_id", opt_id(patch.task_id.unwrap_or(base.task_id))),
                    ("start_at", int(start_at)),
                    ("end_at", int(end_at)),
                    ("time_zone", text(base.time_zone.clone())),
                    ("location", text(patch.location.clone().unwrap_or(base.location.clone()))),
                    ("meeting_url", text(patch.meeting_url.clone().unwrap_or(base.meeting_url.clone()))),
                    ("attendees", text(patch.attendees.clone().unwrap_or(base.attendees.clone()))),
                    ("notes_md", text(patch.notes_md.clone().unwrap_or(base.notes_md.clone()))),
                    ("status", text(patch.status.unwrap_or(base.status).as_str())),
                    ("counts_as_time", boolean(patch.counts_as_time.unwrap_or(base.counts_as_time))),
                    ("flexible", boolean(patch.flexible.unwrap_or(base.flexible))),
                    ("recurrence_rule", opt_text(rule)),
                    ("created_at", int(tx.now())),
                ],
            )?;
            // Exceptions at or after the split now belong to the new series.
            let moved: Vec<i64> = {
                let mut stmt = tx.conn().prepare(
                    "SELECT original_start FROM event_exception WHERE event_id = ?1 AND original_start >= ?2",
                )?;
                stmt.query_map(params![id, original_start], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for at in moved {
                tx.delete(Table::EventException, &[id.into(), Value::Integer(at)])?;
            }
            Ok(())
        })?;
        self.event(new_id)
    }

    /// Every occurrence overlapping `[from, to)` (UTC ms), recurring series
    /// expanded, exceptions applied, cancelled occurrences dropped.
    pub fn occurrences(&self, from: i64, to: i64) -> Result<Vec<Occurrence>> {
        if to <= from || to - from > 400 * 86_400_000 {
            return Err(invalid("range must be positive and at most 400 days"));
        }
        let (local, _) = local_zone();
        let from_date = ms_to_date(from, &local)
            .map(|d| d.to_string())
            .unwrap_or_default();
        let to_date = ms_to_date(to, &local)
            .map(|d| d.to_string())
            .unwrap_or_default();
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS},
                    (SELECT name FROM client c WHERE c.id = e.client_id),
                    (SELECT name FROM project p WHERE p.id = e.project_id),
                    coalesce((SELECT color FROM project p WHERE p.id = e.project_id),
                             (SELECT color FROM client c WHERE c.id = e.client_id), 'gray')
             FROM event e
             WHERE e.deleted_at IS NULL AND (
                   (e.recurrence_rule IS NULL AND e.start_at < ?1 AND e.end_at > ?2)
                OR (e.recurrence_rule IS NULL AND e.all_day_start <= ?3 AND e.all_day_end >= ?4)
                OR (e.recurrence_rule IS NOT NULL AND (e.start_at < ?1 OR e.all_day_start <= ?3)))"
        ))?;
        let rows: Vec<(Event, Option<String>, Option<String>, String)> = stmt
            .query_map(params![to, from, to_date, from_date], |r| {
                Ok((map(r)?, r.get(21)?, r.get(22)?, r.get(23)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        let recurring: Vec<Id> = rows
            .iter()
            .filter(|(e, ..)| e.recurrence_rule.is_some())
            .map(|(e, ..)| e.id)
            .collect();
        let mut exceptions: HashMap<(Id, i64), ExceptionRow> = HashMap::new();
        if !recurring.is_empty() {
            let mut stmt = self.conn().prepare(
                "SELECT event_id, original_start, cancelled, start_at, end_at, title FROM event_exception
                 WHERE event_id IN (SELECT id FROM event WHERE recurrence_rule IS NOT NULL AND deleted_at IS NULL)",
            )?;
            let found = stmt.query_map([], |r| {
                Ok((
                    (r.get(0)?, r.get(1)?),
                    (r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
                ))
            })?;
            for item in found {
                let (k, v) = item?;
                exceptions.insert(k, v);
            }
        }

        let mut out = Vec::new();
        for (e, client_name, project_name, color) in rows {
            let make = |original: i64,
                        start: i64,
                        end: i64,
                        all_day: bool,
                        title: String,
                        modified: bool| Occurrence {
                event_id: e.id,
                original_start: original,
                start_at: start,
                end_at: end,
                all_day,
                kind: e.kind,
                title,
                status: e.status,
                client_id: e.client_id,
                client_name: client_name.clone(),
                project_id: e.project_id,
                project_name: project_name.clone(),
                task_id: e.task_id,
                color: color.clone(),
                location: e.location.clone(),
                meeting_url: e.meeting_url.clone(),
                counts_as_time: e.counts_as_time,
                flexible: e.flexible,
                recurring: e.recurrence_rule.is_some(),
                modified,
            };
            let tz = zone(&e.time_zone);
            match (
                &e.recurrence_rule,
                e.start_at,
                e.end_at,
                &e.all_day_start,
                &e.all_day_end,
            ) {
                (None, Some(s), Some(en), _, _) => {
                    out.push(make(s, s, en, false, e.title.clone(), false))
                }
                (None, None, None, Some(a), Some(b)) => {
                    if let (Ok(a), Ok(b)) = (a.parse::<Date>(), b.parse::<Date>())
                        && let (Some(s), Some(en)) = (
                            date_start_ms(a, &local),
                            b.tomorrow().ok().and_then(|d| date_start_ms(d, &local)),
                        )
                    {
                        out.push(make(s, s, en, true, e.title.clone(), false));
                    }
                }
                (Some(rule), Some(s), Some(en), _, _) => {
                    let Ok(rule) = Rule::parse(rule) else {
                        continue;
                    };
                    let Ok(dt) = Timestamp::from_millisecond(s) else {
                        continue;
                    };
                    let duration = en - s;
                    // Widen the window so moved occurrences are still found.
                    let starts = rule.occurrences(
                        dt.to_zoned(tz.clone()).datetime(),
                        &tz,
                        duration,
                        from - 86_400_000,
                        to + 86_400_000,
                    );
                    for start in starts {
                        let (start_at, end_at, title, modified) =
                            match exceptions.get(&(e.id, start)) {
                                Some((true, ..)) => continue,
                                Some((false, ns, ne, nt)) => {
                                    let ns = ns.unwrap_or(start);
                                    (
                                        ns,
                                        ne.unwrap_or(ns + duration),
                                        nt.clone().unwrap_or_else(|| e.title.clone()),
                                        true,
                                    )
                                }
                                None => (start, start + duration, e.title.clone(), false),
                            };
                        if start_at < to && end_at > from {
                            out.push(make(start, start_at, end_at, false, title, modified));
                        }
                    }
                }
                (Some(rule), None, None, Some(a), Some(b)) => {
                    let (Ok(rule), Ok(a), Ok(b)) =
                        (Rule::parse(rule), a.parse::<Date>(), b.parse::<Date>())
                    else {
                        continue;
                    };
                    let span_days = (b - a).get_days();
                    let Some(to_d) = ms_to_date(to, &local) else {
                        continue;
                    };
                    for d in rule.dates(a, |d| d <= to_d) {
                        let (Some(s), Some(en)) = (
                            date_start_ms(d, &local),
                            d.checked_add((i64::from(span_days) + 1).days())
                                .ok()
                                .and_then(|x| date_start_ms(x, &local)),
                        ) else {
                            continue;
                        };
                        if matches!(exceptions.get(&(e.id, s)), Some((true, ..))) {
                            continue;
                        }
                        if en > from && s < to {
                            out.push(make(s, s, en, true, e.title.clone(), false));
                        }
                    }
                }
                _ => {}
            }
        }
        out.sort_by_key(|o| (o.start_at, o.end_at));
        Ok(out)
    }
}
