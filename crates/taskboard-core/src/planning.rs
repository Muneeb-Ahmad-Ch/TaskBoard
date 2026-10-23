//! Working hours, days off, logged time, daily capacity, hours per client and
//! the meeting slot finder (SPEC §5 "Working hours", "Find a meeting slot").

use std::collections::{BTreeMap, HashMap};

use jiff::ToSpan;
use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result, invalid};
use crate::events::{EventKind, EventStatus, local_zone};
use crate::id::Id;
use crate::model::{clean_date, clean_time, int, opt_id, opt_int, text};
use crate::slots::{Span, intersect, normalize, round_up, subtract, suggest};
use crate::storage::Db;
use crate::tx::{Table, Tx};

const MINUTE: i64 = 60_000;
const DAY_MS: i64 = 86_400_000;
/// Working intervals of one date, whether it is a day off, and its note.
type WorkingSpans = (Vec<Span>, bool, String);

const CONFIGURED_KEY: &str = "working_hours_configured";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Interval {
    /// `HH:MM`, local time.
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkingDay {
    /// ISO weekday, 1 = Monday … 7 = Sunday.
    pub weekday: u8,
    pub intervals: Vec<Interval>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum OverrideKind {
    Off,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayOverride {
    pub date: String,
    pub kind: OverrideKind,
    /// Used when `kind` is `custom`.
    #[serde(default)]
    pub intervals: Vec<Interval>,
    #[serde(default)]
    pub note: String,
}

/// A stretch of logged work: a time entry, or a meeting that counts as work time.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LoggedTime {
    /// Time entry id; `None` for meetings counted as work time.
    pub entry_id: Option<Id>,
    pub event_id: Option<Id>,
    pub task_id: Option<Id>,
    pub title: String,
    pub client_id: Option<Id>,
    pub client_name: Option<String>,
    pub project_id: Option<Id>,
    pub project_name: Option<String>,
    pub color: String,
    pub started_at: i64,
    /// `None` while a timer is running.
    pub ended_at: Option<i64>,
    pub note: String,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct NewTimeEntry {
    #[serde(default)]
    #[ts(optional)]
    pub task_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<Id>,
    pub started_at: i64,
    /// Omit to start a running timer.
    #[serde(default)]
    #[ts(optional)]
    pub ended_at: Option<i64>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct TimeEntryPatch {
    #[ts(optional)]
    pub started_at: Option<i64>,
    #[ts(optional)]
    pub ended_at: Option<i64>,
    #[ts(optional)]
    pub note: Option<String>,
}

/// One day's plan for the capacity bar in day headers.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DayPlan {
    pub date: String,
    pub day_off: bool,
    pub note: String,
    /// Working minutes from the schedule (after days off / custom days).
    pub available_min: i64,
    pub meetings_min: i64,
    pub blocks_min: i64,
    /// Estimates of open tasks due this day.
    pub due_estimate_min: i64,
    pub logged_min: i64,
    /// Working-hours intervals, UTC ms, for shading the grid.
    pub working: Vec<(i64, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ClientHours {
    /// The group, or None for a section's projects without a group (and for
    /// time not linked to any project or group).
    pub client_id: Option<Id>,
    pub section_id: Option<Id>,
    /// The section's name (the row's own name for section rows).
    pub section_name: String,
    pub client_name: String,
    pub color: String,
    /// Meetings + time blocks.
    pub planned_min: i64,
    /// Time entries + meetings that count as work time.
    pub logged_min: i64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct FindSlots {
    pub from_date: String,
    pub to_date: String,
    pub duration_min: i64,
    /// Gap kept before and after other meetings (default 10).
    #[serde(default)]
    #[ts(optional)]
    pub buffer_min: Option<i64>,
    /// Only slots starting at or after this local time.
    #[serde(default)]
    #[ts(optional)]
    pub earliest: Option<String>,
    /// Only slots ending at or before this local time.
    #[serde(default)]
    #[ts(optional)]
    pub latest: Option<String>,
    /// Time blocks marked flexible don't count as busy (default true).
    #[serde(default)]
    #[ts(optional)]
    pub blocks_flexible: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub max_meetings_per_day: Option<u32>,
    /// No slots sooner than this from now (default 120).
    #[serde(default)]
    #[ts(optional)]
    pub min_notice_min: Option<i64>,
    /// Also fit inside this client's working hours, in their time zone.
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Slot {
    pub start_at: i64,
    pub end_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SlotDay {
    pub date: String,
    /// Free windows long enough for the meeting.
    pub windows: Vec<Slot>,
    /// Suggested slots, best first.
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SlotResult {
    pub days: Vec<SlotDay>,
    /// IANA zone of the client, when one was given and has a zone.
    pub client_time_zone: Option<String>,
    pub local_time_zone: String,
}

fn parse_hm(s: &str) -> Result<Time> {
    let clean = clean_time(s)?;
    clean
        .parse::<Time>()
        .map_err(|_| invalid(format!("bad time {s:?}")))
}

fn clean_intervals(list: &[Interval]) -> Result<Vec<Interval>> {
    let mut parsed: Vec<(Time, Time)> = Vec::new();
    for i in list {
        let (a, b) = (parse_hm(&i.start)?, parse_hm(&i.end)?);
        if b <= a {
            return Err(invalid(format!(
                "{}–{} ends before it starts",
                i.start, i.end
            )));
        }
        parsed.push((a, b));
    }
    parsed.sort();
    if parsed.windows(2).any(|w| w[1].0 < w[0].1) {
        return Err(invalid("working hours on one day must not overlap"));
    }
    Ok(parsed
        .into_iter()
        .map(|(a, b)| Interval {
            start: fmt_hm(a),
            end: fmt_hm(b),
        })
        .collect())
}

fn fmt_hm(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

fn default_week() -> Vec<WorkingDay> {
    (1..=7)
        .map(|weekday| WorkingDay {
            weekday,
            intervals: if weekday <= 5 {
                vec![Interval {
                    start: "09:00".into(),
                    end: "17:00".into(),
                }]
            } else {
                Vec::new()
            },
        })
        .collect()
}

fn day_spans(date: Date, intervals: &[Interval], tz: &TimeZone) -> Vec<Span> {
    intervals
        .iter()
        .filter_map(|i| {
            let (a, b) = (parse_hm(&i.start).ok()?, parse_hm(&i.end).ok()?);
            let s = tz
                .to_zoned(date.to_datetime(a))
                .ok()?
                .timestamp()
                .as_millisecond();
            let e = tz
                .to_zoned(date.to_datetime(b))
                .ok()?
                .timestamp()
                .as_millisecond();
            Some((s, e))
        })
        .collect()
}

fn date_range(from: &str, to: &str, max_days: i64) -> Result<Vec<Date>> {
    let (from, to): (Date, Date) = (
        clean_date(from)?.parse().map_err(|_| invalid("bad date"))?,
        clean_date(to)?.parse().map_err(|_| invalid("bad date"))?,
    );
    if to < from {
        return Err(invalid("the end date is before the start date"));
    }
    let days = i64::from((to - from).get_days()) + 1;
    if days > max_days {
        return Err(invalid(format!("at most {max_days} days at a time")));
    }
    Ok((0..days)
        .filter_map(|i| from.checked_add(i.days()).ok())
        .collect())
}

fn overlap_min(spans: &[Span], window: Span) -> i64 {
    spans
        .iter()
        .map(|&(a, b)| (b.min(window.1) - a.max(window.0)).max(0))
        .sum::<i64>()
        / MINUTE
}

fn resolve_entry_owner(tx: &Tx<'_>, input: &NewTimeEntry) -> Result<()> {
    if input.task_id.is_none() && input.project_id.is_none() && input.client_id.is_none() {
        return Err(invalid("log time against a task, project or client"));
    }
    if let Some(t) = input.task_id {
        let ok: bool = tx.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM task WHERE id = ?1 AND deleted_at IS NULL)",
            [t],
            |r| r.get(0),
        )?;
        if !ok {
            return Err(Error::NotFound("task"));
        }
    }
    Ok(())
}

impl Db {
    // ---------- working hours ----------

    pub fn working_hours(&self) -> Result<Vec<WorkingDay>> {
        let configured: bool = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM setting WHERE key = ?1)",
            [CONFIGURED_KEY],
            |r| r.get(0),
        )?;
        if !configured {
            return Ok(default_week());
        }
        let mut week: Vec<WorkingDay> = (1..=7)
            .map(|weekday| WorkingDay {
                weekday,
                intervals: Vec::new(),
            })
            .collect();
        let mut stmt = self.conn().prepare(
            "SELECT weekday, start_time, end_time FROM working_hours ORDER BY weekday, start_time",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, u8>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (wd, start, end) = row?;
            if let Some(day) = week.get_mut(usize::from(wd.saturating_sub(1))) {
                day.intervals.push(Interval { start, end });
            }
        }
        Ok(week)
    }

    /// Replaces the weekly schedule (one undoable action).
    pub fn set_working_hours(&mut self, week: Vec<WorkingDay>) -> Result<Vec<WorkingDay>> {
        let mut clean: Vec<(u8, Vec<Interval>)> = Vec::new();
        for day in &week {
            if !(1..=7).contains(&day.weekday) {
                return Err(invalid("weekday must be 1 (Mon) to 7 (Sun)"));
            }
            clean.push((day.weekday, clean_intervals(&day.intervals)?));
        }
        self.transact("Change working hours", |tx| {
            let existing: Vec<(i64, String)> = {
                let mut stmt = tx
                    .conn()
                    .prepare("SELECT weekday, start_time FROM working_hours")?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            };
            for (wd, start) in existing {
                tx.delete(
                    Table::WorkingHours,
                    &[Value::Integer(wd), Value::Text(start)],
                )?;
            }
            for (wd, intervals) in &clean {
                for i in intervals {
                    tx.insert(
                        Table::WorkingHours,
                        &[
                            ("weekday", int(i64::from(*wd))),
                            ("start_time", text(i.start.clone())),
                            ("end_time", text(i.end.clone())),
                        ],
                    )?;
                }
            }
            let configured: bool = tx.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM setting WHERE key = ?1)",
                [CONFIGURED_KEY],
                |r| r.get(0),
            )?;
            if !configured {
                tx.insert(
                    Table::Setting,
                    &[("key", text(CONFIGURED_KEY)), ("value", text("true"))],
                )?;
            }
            Ok(())
        })?;
        self.working_hours()
    }

    pub fn day_overrides(&self, from: &str, to: &str) -> Result<Vec<DayOverride>> {
        let mut stmt = self.conn().prepare(
            "SELECT date, kind, intervals, note FROM day_override WHERE date >= ?1 AND date <= ?2 ORDER BY date",
        )?;
        let rows = stmt
            .query_map(params![clean_date(from)?, clean_date(to)?], |r| {
                let intervals: Option<String> = r.get(2)?;
                Ok(DayOverride {
                    date: r.get(0)?,
                    kind: if r.get::<_, String>(1)? == "off" {
                        OverrideKind::Off
                    } else {
                        OverrideKind::Custom
                    },
                    intervals: intervals
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    note: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Marks a day off, or gives it custom hours (replacing any previous override).
    pub fn set_day_override(&mut self, o: DayOverride) -> Result<()> {
        let date = clean_date(&o.date)?;
        let intervals = match o.kind {
            OverrideKind::Off => None,
            OverrideKind::Custom => Some(serde_json::to_string(&clean_intervals(&o.intervals)?)?),
        };
        let label = match o.kind {
            OverrideKind::Off => format!("Mark {date} off"),
            OverrideKind::Custom => format!("Set hours for {date}"),
        };
        self.transact(&label, |tx| {
            let exists: bool = tx.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM day_override WHERE date = ?1)",
                [&date],
                |r| r.get(0),
            )?;
            let cols = [
                (
                    "kind",
                    text(if o.kind == OverrideKind::Off {
                        "off"
                    } else {
                        "custom"
                    }),
                ),
                (
                    "intervals",
                    intervals.clone().map_or(Value::Null, Value::Text),
                ),
                ("note", text(o.note.trim())),
            ];
            if exists {
                tx.update(Table::DayOverride, &[text(date.clone())], &cols)?;
            } else {
                let mut row = vec![("date", text(date.clone()))];
                row.extend(cols);
                tx.insert(Table::DayOverride, &row)?;
            }
            Ok(())
        })
    }

    pub fn clear_day_override(&mut self, date: &str) -> Result<()> {
        let date = clean_date(date)?;
        self.transact(&format!("Restore normal hours on {date}"), |tx| {
            tx.delete(Table::DayOverride, &[text(date.clone())])
        })
    }

    /// Working intervals per date (local zone), after days off / custom days.
    fn working_spans(&self, dates: &[Date], tz: &TimeZone) -> Result<BTreeMap<Date, WorkingSpans>> {
        let week = self.working_hours()?;
        let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
            return Ok(BTreeMap::new());
        };
        let overrides: HashMap<String, DayOverride> = self
            .day_overrides(&first.to_string(), &last.to_string())?
            .into_iter()
            .map(|o| (o.date.clone(), o))
            .collect();
        let mut out = BTreeMap::new();
        for &d in dates {
            let (intervals, off, note) = match overrides.get(&d.to_string()) {
                Some(o) if o.kind == OverrideKind::Off => (Vec::new(), true, o.note.clone()),
                Some(o) => (o.intervals.clone(), false, o.note.clone()),
                None => {
                    let wd = d.weekday().to_monday_one_offset();
                    let day = week
                        .iter()
                        .find(|w| i8::try_from(w.weekday).ok() == Some(wd));
                    (
                        day.map(|w| w.intervals.clone()).unwrap_or_default(),
                        false,
                        String::new(),
                    )
                }
            };
            out.insert(d, (day_spans(d, &intervals, tz), off, note));
        }
        Ok(out)
    }

    // ---------- time tracking ----------

    pub fn running_timer(&self) -> Result<Option<LoggedTime>> {
        let id: Option<Id> = self
            .conn()
            .query_row(
                "SELECT id FROM time_entry WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => Ok(self
                .time_entries_where("te.id = ?1", &[id.into()])?
                .into_iter()
                .next()),
            None => Ok(None),
        }
    }

    /// Logs time, or starts a timer when `ended_at` is omitted (stopping any
    /// running one first, in the same action).
    pub fn log_time(&mut self, input: NewTimeEntry) -> Result<Id> {
        if let Some(end) = input.ended_at
            && end < input.started_at
        {
            return Err(invalid("time entry ends before it starts"));
        }
        let id = Id::new();
        let label = if input.ended_at.is_some() {
            "Log time"
        } else {
            "Start timer"
        };
        self.transact(label, |tx| {
            resolve_entry_owner(tx, &input)?;
            if input.ended_at.is_none() {
                stop_running(tx)?;
            }
            tx.insert(
                Table::TimeEntry,
                &[
                    ("id", id.into()),
                    ("task_id", opt_id(input.task_id)),
                    ("project_id", opt_id(input.project_id)),
                    ("client_id", opt_id(input.client_id)),
                    ("started_at", int(input.started_at)),
                    ("ended_at", opt_int(input.ended_at)),
                    ("note", text(input.note.clone().unwrap_or_default())),
                ],
            )
        })?;
        Ok(id)
    }

    /// Stops the running timer, if any. Returns whether one was running.
    pub fn stop_timer(&mut self) -> Result<bool> {
        let running: bool = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM time_entry WHERE ended_at IS NULL)",
            [],
            |r| r.get(0),
        )?;
        if !running {
            return Ok(false);
        }
        self.transact("Stop timer", stop_running)?;
        Ok(true)
    }

    pub fn update_time_entry(&mut self, id: Id, patch: TimeEntryPatch) -> Result<()> {
        self.transact("Edit time entry", |tx| {
            let (start, end): (i64, Option<i64>) = tx
                .conn()
                .query_row(
                    "SELECT started_at, ended_at FROM time_entry WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or(Error::NotFound("time entry"))?;
            let (start, end) = (patch.started_at.unwrap_or(start), patch.ended_at.or(end));
            if end.is_some_and(|e| e < start) {
                return Err(invalid("time entry ends before it starts"));
            }
            let mut cols = vec![("started_at", int(start)), ("ended_at", opt_int(end))];
            if let Some(n) = &patch.note {
                cols.push(("note", text(n.clone())));
            }
            tx.update(Table::TimeEntry, &[id.into()], &cols)?;
            Ok(())
        })
    }

    pub fn delete_time_entry(&mut self, id: Id) -> Result<()> {
        self.transact("Delete time entry", |tx| {
            tx.delete(Table::TimeEntry, &[id.into()])
        })
    }

    fn time_entries_where(&self, filter: &str, args: &[Value]) -> Result<Vec<LoggedTime>> {
        // A task's project/client win over the entry's own columns.
        let sql = format!(
            "SELECT te.id, te.task_id, coalesce(t.title, p.name, c.name, 'Time'),
                    coalesce(pc.id, p.client_id, te.client_id),
                    coalesce(pc.name, (SELECT name FROM client WHERE id = p.client_id), c.name),
                    coalesce(tp.id, p.id), coalesce(tp.name, p.name),
                    coalesce(tp.color, p.color, c.color, 'gray'),
                    te.started_at, te.ended_at, te.note
             FROM time_entry te
             LEFT JOIN task t ON t.id = te.task_id
             LEFT JOIN project tp ON tp.id = t.project_id
             LEFT JOIN client pc ON pc.id = tp.client_id
             LEFT JOIN project p ON p.id = te.project_id
             LEFT JOIN client c ON c.id = te.client_id
             WHERE {filter} ORDER BY te.started_at"
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(LoggedTime {
                    entry_id: Some(r.get(0)?),
                    event_id: None,
                    task_id: r.get(1)?,
                    title: r.get(2)?,
                    client_id: r.get(3)?,
                    client_name: r.get(4)?,
                    project_id: r.get(5)?,
                    project_name: r.get(6)?,
                    color: r.get(7)?,
                    started_at: r.get(8)?,
                    ended_at: r.get(9)?,
                    note: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Logged work overlapping `[from, to)`: time entries plus meetings that
    /// count as work time (derived, so cancelling a meeting removes its time).
    pub fn logged_time(&self, from: i64, to: i64) -> Result<Vec<LoggedTime>> {
        let mut out = self.time_entries_where(
            "te.started_at < ?1 AND coalesce(te.ended_at, ?3) > ?2",
            &[
                Value::Integer(to),
                Value::Integer(from),
                Value::Integer(crate::clock::now_ms()),
            ],
        )?;
        for o in self.occurrences(from, to)? {
            if o.kind == EventKind::Meeting
                && o.counts_as_time
                && o.status != EventStatus::Cancelled
                && !o.all_day
            {
                out.push(LoggedTime {
                    entry_id: None,
                    event_id: Some(o.event_id),
                    task_id: None,
                    title: o.title,
                    client_id: o.client_id,
                    client_name: o.client_name,
                    project_id: o.project_id,
                    project_name: o.project_name,
                    color: o.color,
                    started_at: o.start_at,
                    ended_at: Some(o.end_at),
                    note: String::new(),
                });
            }
        }
        out.sort_by_key(|l| l.started_at);
        Ok(out)
    }

    // ---------- capacity & reports ----------

    pub fn day_plans(&self, from: &str, to: &str) -> Result<Vec<DayPlan>> {
        let (tz, _) = local_zone();
        let dates = date_range(from, to, 62)?;
        let spans = self.working_spans(&dates, &tz)?;
        let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
            return Ok(Vec::new());
        };
        let start_ms = tz
            .to_zoned(first.to_datetime(Time::midnight()))?
            .timestamp()
            .as_millisecond();
        let end_ms = tz
            .to_zoned(last.tomorrow()?.to_datetime(Time::midnight()))?
            .timestamp()
            .as_millisecond();
        let occ = self.occurrences(start_ms, end_ms)?;
        let logged = self.logged_time(start_ms, end_ms)?;
        let now = crate::clock::now_ms();
        let mut due: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt = self.conn().prepare(
                // Live work only; a task whose subtasks carry estimates counts
                // through them, not twice.
                "SELECT t.due_date, coalesce(sum(t.estimate_minutes), 0) FROM task t
                 WHERE t.deleted_at IS NULL AND t.completed_at IS NULL AND t.archived_at IS NULL
                   AND t.due_date BETWEEN ?1 AND ?2
                   AND t.project_id NOT IN (SELECT id FROM project WHERE archived_at IS NOT NULL)
                   AND NOT EXISTS (SELECT 1 FROM task c WHERE c.parent_id = t.id
                                   AND c.deleted_at IS NULL AND c.archived_at IS NULL
                                   AND c.completed_at IS NULL AND c.estimate_minutes IS NOT NULL)
                 GROUP BY t.due_date",
            )?;
            let rows = stmt.query_map(params![first.to_string(), last.to_string()], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            for row in rows {
                let (d, m): (String, i64) = row?;
                due.insert(d, m);
            }
        }
        let mut out = Vec::new();
        for d in dates {
            let day0 = tz
                .to_zoned(d.to_datetime(Time::midnight()))?
                .timestamp()
                .as_millisecond();
            let day1 = tz
                .to_zoned(d.tomorrow()?.to_datetime(Time::midnight()))?
                .timestamp()
                .as_millisecond();
            let (working, off, note) = spans.get(&d).cloned().unwrap_or_default();
            let timed = |kind: EventKind| -> Vec<Span> {
                occ.iter()
                    .filter(|o| o.kind == kind && !o.all_day && o.status != EventStatus::Cancelled)
                    .map(|o| (o.start_at, o.end_at))
                    .collect()
            };
            let logged_spans: Vec<Span> = logged
                .iter()
                .map(|l| (l.started_at, l.ended_at.unwrap_or(now)))
                .collect();
            out.push(DayPlan {
                date: d.to_string(),
                day_off: off,
                note,
                available_min: working.iter().map(|(a, b)| b - a).sum::<i64>() / MINUTE,
                meetings_min: overlap_min(&normalize(timed(EventKind::Meeting)), (day0, day1)),
                blocks_min: overlap_min(&normalize(timed(EventKind::Block)), (day0, day1)),
                due_estimate_min: due.get(&d.to_string()).copied().unwrap_or(0),
                logged_min: overlap_min(&logged_spans, (day0, day1)),
                working,
            });
        }
        Ok(out)
    }

    /// Planned (meetings + blocks) vs logged minutes per client in `[from, to)`.
    pub fn hours_by_client(&self, from: i64, to: i64) -> Result<Vec<ClientHours>> {
        // Time on a project without a group counts toward its section.
        let sections: HashMap<Id, (String, String)> = self
            .sections()?
            .into_iter()
            .map(|s| (s.id, (s.name, s.color)))
            .collect();
        let section_of = |sql: &str| -> Result<HashMap<Id, Id>> {
            let mut stmt = self.conn().prepare(sql)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, Id>(0)?, r.get::<_, Id>(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        };
        let client_section =
            section_of("SELECT id, section_id FROM client WHERE section_id IS NOT NULL")?;
        let project_section = section_of(
            "SELECT id, section_id FROM project WHERE client_id IS NULL AND section_id IS NOT NULL",
        )?;
        let mut map: BTreeMap<String, ClientHours> = BTreeMap::new();
        let mut entry = |client: Option<Id>,
                         project: Option<Id>,
                         name: Option<String>,
                         color: &str|
         -> String {
            let section = match client {
                Some(c) => client_section.get(&c).copied(),
                None => project.and_then(|p| project_section.get(&p).copied()),
            };
            let (section_name, section_color) = section
                .and_then(|s| sections.get(&s).cloned())
                .unwrap_or_default();
            let key = match (client, section) {
                (Some(c), _) => c.to_string(),
                (None, Some(s)) => format!("s:{s}"),
                (None, None) => String::new(),
            };
            map.entry(key.clone()).or_insert_with(|| ClientHours {
                client_id: client,
                section_id: section,
                client_name: match client {
                    Some(_) => name.unwrap_or_default(),
                    None if section.is_some() => section_name.clone(),
                    None => "No group".into(),
                },
                section_name,
                color: if client.is_none() && section.is_some() {
                    section_color
                } else {
                    color.to_owned()
                },
                planned_min: 0,
                logged_min: 0,
            });
            key
        };
        let mut planned: Vec<(String, i64)> = Vec::new();
        for o in self.occurrences(from, to)? {
            if o.all_day || o.status == EventStatus::Cancelled {
                continue;
            }
            let key = entry(o.client_id, o.project_id, o.client_name.clone(), &o.color);
            planned.push((
                key,
                (o.end_at.min(to) - o.start_at.max(from)).max(0) / MINUTE,
            ));
        }
        let now = crate::clock::now_ms();
        let mut logged: Vec<(String, i64)> = Vec::new();
        for l in self.logged_time(from, to)? {
            let key = entry(l.client_id, l.project_id, l.client_name.clone(), &l.color);
            logged.push((
                key,
                (l.ended_at.unwrap_or(now).min(to) - l.started_at.max(from)).max(0) / MINUTE,
            ));
        }
        for (k, m) in planned {
            if let Some(c) = map.get_mut(&k) {
                c.planned_min += m;
            }
        }
        for (k, m) in logged {
            if let Some(c) = map.get_mut(&k) {
                c.logged_min += m;
            }
        }
        let mut out: Vec<ClientHours> = map.into_values().collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.planned_min + c.logged_min));
        Ok(out)
    }

    // ---------- slot finder ----------

    pub fn find_slots(&self, q: &FindSlots) -> Result<SlotResult> {
        if !(5..=8 * 60).contains(&q.duration_min) {
            return Err(invalid(
                "meeting length must be between 5 minutes and 8 hours",
            ));
        }
        let (tz, tz_name) = local_zone();
        let dates = date_range(&q.from_date, &q.to_date, 31)?;
        let working = self.working_spans(&dates, &tz)?;
        let buffer = q.buffer_min.unwrap_or(10).clamp(0, 120) * MINUTE;
        let duration = q.duration_min * MINUTE;
        let not_before = crate::clock::now_ms() + q.min_notice_min.unwrap_or(120).max(0) * MINUTE;
        let earliest = q.earliest.as_deref().map(parse_hm).transpose()?;
        let latest = q.latest.as_deref().map(parse_hm).transpose()?;
        let blocks_flexible = q.blocks_flexible.unwrap_or(true);

        // Client working hours, converted from the client's zone.
        let client: Option<(TimeZone, String, Time, Time)> = match q.client_id {
            Some(id) => {
                let c = self.client(id)?;
                match c
                    .time_zone
                    .as_deref()
                    .and_then(|z| TimeZone::get(z).ok().map(|t| (t, z.to_owned())))
                {
                    Some((ctz, name)) => {
                        let ws = parse_hm(c.work_start.as_deref().unwrap_or("09:00"))?;
                        let we = parse_hm(c.work_end.as_deref().unwrap_or("17:00"))?;
                        Some((ctz, name, ws, we))
                    }
                    None => None,
                }
            }
            None => None,
        };

        let (Some(first), Some(last)) = (dates.first(), dates.last()) else {
            return Ok(SlotResult {
                days: Vec::new(),
                client_time_zone: None,
                local_time_zone: tz_name,
            });
        };
        let range0 = tz
            .to_zoned(first.to_datetime(Time::midnight()))?
            .timestamp()
            .as_millisecond()
            - DAY_MS;
        let range1 = tz
            .to_zoned(last.tomorrow()?.to_datetime(Time::midnight()))?
            .timestamp()
            .as_millisecond()
            + DAY_MS;
        let occ = self.occurrences(range0, range1)?;
        let busy: Vec<Span> = occ
            .iter()
            .filter(|o| o.status != EventStatus::Cancelled && !o.all_day)
            .filter(|o| o.kind == EventKind::Meeting || !(blocks_flexible && o.flexible))
            .map(|o| (o.start_at - buffer, o.end_at + buffer))
            .collect();

        let mut days = Vec::new();
        for d in dates {
            let (spans, _off, _) = working.get(&d).cloned().unwrap_or_default();
            let mut available = spans;
            let day_ms = |t: Time| {
                tz.to_zoned(d.to_datetime(t))
                    .ok()
                    .map(|z| z.timestamp().as_millisecond())
            };
            if let Some(e) = earliest.and_then(day_ms) {
                available = intersect(&available, &[(e, i64::MAX)]);
            }
            if let Some(l) = latest.and_then(day_ms) {
                available = intersect(&available, &[(i64::MIN, l)]);
            }
            if let Some((ctz, _, ws, we)) = &client {
                let mut theirs = Vec::new();
                for offset in [-1i64, 0, 1] {
                    if let Ok(cd) = d.checked_add(offset.days())
                        && let (Ok(s), Ok(e)) = (
                            ctz.to_zoned(cd.to_datetime(*ws)),
                            ctz.to_zoned(cd.to_datetime(*we)),
                        )
                    {
                        theirs.push((
                            s.timestamp().as_millisecond(),
                            e.timestamp().as_millisecond(),
                        ));
                    }
                }
                available = intersect(&available, &theirs);
            }
            if let Some(cap) = q.max_meetings_per_day {
                let day0 = day_ms(Time::midnight()).unwrap_or(0);
                let count = occ
                    .iter()
                    .filter(|o| {
                        o.kind == EventKind::Meeting
                            && o.status != EventStatus::Cancelled
                            && !o.all_day
                    })
                    .filter(|o| o.start_at >= day0 && o.start_at < day0 + DAY_MS)
                    .count();
                if count >= cap as usize {
                    days.push(SlotDay {
                        date: d.to_string(),
                        windows: Vec::new(),
                        slots: Vec::new(),
                    });
                    continue;
                }
            }
            let free: Vec<Span> = subtract(&available, &busy)
                .into_iter()
                .map(|(a, b)| (a.max(not_before), b))
                .filter(|(a, b)| b - a >= duration)
                .collect();
            let origin = day_ms(Time::midnight()).unwrap_or(0);
            let slots = suggest(&free, duration, 15 * MINUTE, origin, not_before);
            days.push(SlotDay {
                date: d.to_string(),
                windows: free
                    .iter()
                    .map(|&(a, b)| Slot {
                        start_at: round_up(a, origin, 5 * MINUTE),
                        end_at: b,
                    })
                    .filter(|s| s.end_at - s.start_at >= duration)
                    .collect(),
                slots: slots
                    .into_iter()
                    .map(|(a, b)| Slot {
                        start_at: a,
                        end_at: b,
                    })
                    .collect(),
            });
        }
        Ok(SlotResult {
            days,
            client_time_zone: client.map(|(_, name, ..)| name),
            local_time_zone: tz_name,
        })
    }
}

fn stop_running(tx: &mut Tx<'_>) -> Result<()> {
    let running: Option<Id> = tx
        .conn()
        .query_row(
            "SELECT id FROM time_entry WHERE ended_at IS NULL",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = running {
        let now = tx.now();
        tx.update(Table::TimeEntry, &[id.into()], &[("ended_at", int(now))])?;
    }
    Ok(())
}

/// Convenience: the timestamp of a local date + time (used by tests and the CLI).
pub fn local_ms(date: Date, time: Time) -> Option<i64> {
    let (tz, _) = local_zone();
    tz.to_zoned(date.to_datetime(time))
        .ok()
        .map(|z| z.timestamp().as_millisecond())
}

impl From<jiff::Error> for Error {
    fn from(err: jiff::Error) -> Self {
        Error::InvalidInput(err.to_string())
    }
}
