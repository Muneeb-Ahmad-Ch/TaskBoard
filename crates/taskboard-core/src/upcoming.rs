//! The Reminders page: everything that will remind you in a date window
//! (timed tasks, meetings, optionally time blocks), the future repeats of a
//! repeating task (which only exist once the current one is done), and the
//! list of every repeating task and event series.

use std::collections::HashMap;

use jiff::ToSpan;
use jiff::civil::{Date, Time};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Result, invalid};
use crate::events::{EventKind, EventStatus, local_zone};
use crate::id::Id;
use crate::model::clean_date;
use crate::notify::local_ms;
use crate::recur::TaskRule;
use crate::rrule::{Freq, Rule};
use crate::storage::Db;

const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;
const DEFAULT_DAYS: i64 = 30;
const MAX_DAYS: i64 = 366;
/// Safety cap on items, so a daily repeat over a year can't flood the page.
const MAX_ITEMS: usize = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ReminderKind {
    Task,
    Meeting,
    Block,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ReminderQuery {
    /// First day (local `YYYY-MM-DD`); default today. From today, overdue
    /// reminders are included too.
    #[ts(optional)]
    pub from: Option<String>,
    /// Days to look ahead, 1–366 (default 30).
    #[ts(optional)]
    pub days: Option<i64>,
    /// Empty = tasks and meetings.
    #[ts(as = "Option<Vec<ReminderKind>>", optional)]
    pub kinds: Vec<ReminderKind>,
}

/// One thing on a day: a timed or repeating task, or a meeting / time block.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ReminderItem {
    pub kind: ReminderKind,
    /// Unique within a list.
    pub key: String,
    pub title: String,
    /// Local `YYYY-MM-DD`.
    pub date: String,
    /// Local `HH:MM`; none for an untimed task or an all-day event.
    pub time: Option<String>,
    /// When it happens (Unix ms); the start of the day when untimed.
    pub at: i64,
    /// When the notification fires; none when nothing will (untimed, all
    /// day, time blocks, or reminders off).
    pub remind_at: Option<i64>,
    /// How it repeats, in words ("Every month on the 1st").
    pub repeat: Option<String>,
    /// A future repeat of a task: made when the one before it is done.
    pub projected: bool,
    /// A task due before today and not done.
    pub overdue: bool,
    pub task_id: Option<Id>,
    pub event_id: Option<Id>,
    /// Series start of an event occurrence (to open that occurrence).
    pub original_start: Option<i64>,
    pub end_at: Option<i64>,
    pub project_id: Option<Id>,
    pub project_name: Option<String>,
    pub client_name: Option<String>,
    /// Project color, else group color, else "gray".
    pub color: String,
    pub meeting_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct UpcomingReminders {
    /// Notifications are on (Settings → Reminders).
    pub reminders_on: bool,
    pub meeting_lead_min: i64,
    pub task_lead_min: i64,
    /// First and last day covered (`to` is earlier than asked when `truncated`).
    pub from: String,
    pub to: String,
    /// The window held more than the page can show; it ends at `to`.
    pub truncated: bool,
    pub items: Vec<ReminderItem>,
}

/// A repeating task or event series.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RepeatingItem {
    pub kind: ReminderKind,
    pub title: String,
    /// The stored rule (RRULE, or `AFTER:<n><D|W|M>` for tasks).
    pub rule: String,
    /// The rule in words.
    pub describe: String,
    /// Repeats counted from when the current one is done.
    pub after_completion: bool,
    /// Next date (local `YYYY-MM-DD`) and time, if known.
    pub next_date: Option<String>,
    pub next_time: Option<String>,
    pub next_at: Option<i64>,
    /// End of the next occurrence (events).
    pub next_end_at: Option<i64>,
    /// An all-day event series.
    pub all_day: bool,
    pub task_id: Option<Id>,
    pub event_id: Option<Id>,
    pub original_start: Option<i64>,
    pub project_id: Option<Id>,
    pub project_name: Option<String>,
    pub client_name: Option<String>,
    pub color: String,
}

/// An open, unarchived task that is timed or repeats.
struct TaskLine {
    id: Id,
    title: String,
    due: Option<Date>,
    time: Option<Time>,
    rule: Option<String>,
    project_id: Id,
    project_name: String,
    client_name: Option<String>,
    color: String,
}

fn hm(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

fn describe_task_rule(rule: &str, anchor: Option<Date>) -> String {
    match TaskRule::parse(rule) {
        Ok(TaskRule::Fixed(r)) => describe_fixed(&r, anchor),
        Ok(other) => other.describe(),
        Err(_) => rule.to_owned(),
    }
}

fn describe_event_rule(rule: &str, anchor: Option<Date>) -> String {
    Rule::parse(rule).map_or_else(|_| rule.to_owned(), |r| describe_fixed(&r, anchor))
}

/// A rule in words, saying which day when it takes it from its date
/// ("Every month" → "Every month on the 3rd", "Every year" → "on Mar 3").
fn describe_fixed(r: &Rule, anchor: Option<Date>) -> String {
    let text = r.describe();
    let Some(d) = anchor else { return text };
    if !r.by_day.is_empty() || !r.by_month_day.is_empty() {
        return text;
    }
    let on = match r.freq {
        Freq::Monthly => format!(" on the {}", ordinal(d.day())),
        Freq::Yearly => format!(" on {}", d.strftime("%b %-d")),
        _ => return text,
    };
    // Before ", 3 times" / ", until …" if the rule has them.
    match text.find(", ") {
        Some(i) => format!("{}{on}{}", &text[..i], &text[i..]),
        None => format!("{text}{on}"),
    }
}

fn ordinal(n: i8) -> String {
    let tail = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{tail}")
}

impl Db {
    fn timed_or_repeating_tasks(
        &self,
        until: Option<Date>,
        rules_only: bool,
    ) -> Result<Vec<TaskLine>> {
        let mut stmt = self.conn().prepare(
            "SELECT t.id, t.title, t.due_date, t.due_time, t.recurrence_rule, t.project_id,
                    p.name, p.color, c.name, c.color
               FROM task t
               JOIN project p ON p.id = t.project_id
               LEFT JOIN client c ON c.id = p.client_id
              WHERE t.deleted_at IS NULL AND t.archived_at IS NULL AND t.completed_at IS NULL
                AND p.deleted_at IS NULL AND p.archived_at IS NULL
                AND (t.recurrence_rule IS NOT NULL
                     OR (t.due_time IS NOT NULL AND t.due_date IS NOT NULL))
                AND (?1 IS NULL OR t.due_date IS NULL OR t.due_date <= ?1)
                AND (?2 = 0 OR t.recurrence_rule IS NOT NULL)
              ORDER BY coalesce(t.due_date, '~'), coalesce(t.due_time, ''), t.sort_key",
        )?;
        let rows = stmt
            .query_map(params![until.map(|d| d.to_string()), rules_only], |r| {
                let due: Option<String> = r.get(2)?;
                let time: Option<String> = r.get(3)?;
                let color: String = r.get(7)?;
                let client_color: Option<String> = r.get(9)?;
                Ok(TaskLine {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    due: due.and_then(|d| d.parse().ok()),
                    time: time.and_then(|t| t.parse().ok()),
                    rule: r.get(4)?,
                    project_id: r.get(5)?,
                    project_name: r.get(6)?,
                    client_name: r.get(8)?,
                    color: if color.is_empty() {
                        client_color.unwrap_or_else(|| "gray".into())
                    } else {
                        color
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Open, unarchived repeating tasks (dated or not).
    fn repeating_tasks(&self) -> Result<Vec<TaskLine>> {
        self.timed_or_repeating_tasks(None, true)
    }

    /// Repeating event series: id → stored rule.
    fn event_rules(&self) -> Result<HashMap<Id, String>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, recurrence_rule FROM event
              WHERE deleted_at IS NULL AND recurrence_rule IS NOT NULL",
        )?;
        let rules = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rules)
    }

    /// What will remind you (and what repeats) between `from` and `from + days`.
    pub fn upcoming_reminders(&self, q: &ReminderQuery, now: i64) -> Result<UpcomingReminders> {
        let days = q.days.unwrap_or(DEFAULT_DAYS);
        if !(1..=MAX_DAYS).contains(&days) {
            return Err(invalid(format!("days must be 1 to {MAX_DAYS}")));
        }
        let (tz, _) = local_zone();
        let today = jiff::Timestamp::from_millisecond(now)
            .map(|t| t.to_zoned(tz.clone()).date())
            .map_err(|e| invalid(e.to_string()))?;
        let from: Date = match &q.from {
            Some(s) => clean_date(s)?
                .parse()
                .map_err(|_| invalid("bad from date"))?,
            None => today,
        };
        let last = from
            .checked_add((days - 1).days())
            .map_err(|e| invalid(e.to_string()))?;
        let kinds = if q.kinds.is_empty() {
            vec![ReminderKind::Task, ReminderKind::Meeting]
        } else {
            q.kinds.clone()
        };
        let s = self.notify_settings()?;
        let (task_leads, event_leads) = self.own_leads()?;
        let mut items = Vec::new();

        if kinds.contains(&ReminderKind::Task) {
            for t in self.timed_or_repeating_tasks(Some(last), false)? {
                let Some(due) = t.due else { continue };
                let overdue = due < today;
                let repeat = t.rule.as_deref().map(|r| describe_task_rule(r, Some(due)));
                let mut push = |date: Date, projected: bool| {
                    let at = local_ms(date, t.time.unwrap_or(Time::midnight())).unwrap_or(now);
                    // An overdue one has passed; its repeats will still remind.
                    let lead = task_leads.get(&t.id).copied().unwrap_or(s.task_lead_min);
                    let remind_at = (s.enabled && !(overdue && !projected) && t.time.is_some())
                        .then(|| at - lead * MINUTE);
                    items.push(ReminderItem {
                        kind: ReminderKind::Task,
                        key: format!("task:{}:{date}", t.id),
                        title: t.title.clone(),
                        date: date.to_string(),
                        time: t.time.map(hm),
                        at,
                        remind_at,
                        repeat: repeat.clone(),
                        projected,
                        overdue: overdue && !projected,
                        task_id: Some(t.id),
                        event_id: None,
                        original_start: None,
                        end_at: None,
                        project_id: Some(t.project_id),
                        project_name: Some(t.project_name.clone()),
                        client_name: t.client_name.clone(),
                        color: t.color.clone(),
                        meeting_url: None,
                    });
                };
                // The current one: in the window, or overdue when the window
                // starts today (or earlier).
                if due >= from || (overdue && from <= today) {
                    push(due, false);
                }
                // Future repeats of a fixed schedule; "after completion" ones
                // can't be known in advance. Like completing it would: the
                // next one comes after its due date and after today, and COUNT
                // counts the ones that happen.
                if let Some(TaskRule::Fixed(rule)) =
                    t.rule.as_deref().and_then(|r| TaskRule::parse(r).ok())
                {
                    let walker = Rule {
                        count: None,
                        ..rule.clone()
                    };
                    let floor = due.max(today);
                    let mut left = rule.count.map(|c| c.saturating_sub(1));
                    for d in walker.dates(due, |d| d <= last) {
                        if d <= floor {
                            continue;
                        }
                        if left == Some(0) || rule.until.is_some_and(|u| d > u) {
                            break;
                        }
                        left = left.map(|c| c - 1);
                        if d >= from {
                            push(d, true);
                        }
                    }
                }
            }
        }

        let want_meetings = kinds.contains(&ReminderKind::Meeting);
        let want_blocks = kinds.contains(&ReminderKind::Block);
        if want_meetings || want_blocks {
            let rules = self.event_rules()?;
            let start = local_ms(from, Time::midnight()).unwrap_or(now);
            let end = local_ms(last.tomorrow().unwrap_or(last), Time::midnight())
                .unwrap_or(start + days * DAY);
            for o in self.occurrences(start, end)? {
                let kind = match o.kind {
                    EventKind::Meeting if want_meetings => ReminderKind::Meeting,
                    EventKind::Block if want_blocks => ReminderKind::Block,
                    _ => continue,
                };
                if o.status == EventStatus::Cancelled {
                    continue;
                }
                let starts = jiff::Timestamp::from_millisecond(o.start_at)
                    .map(|t| t.to_zoned(tz.clone()))
                    .map_err(|e| invalid(e.to_string()))?;
                let lead = event_leads
                    .get(&o.event_id)
                    .copied()
                    .unwrap_or(s.meeting_lead_min);
                let remind_at = (s.enabled && kind == ReminderKind::Meeting && !o.all_day)
                    .then(|| o.start_at - lead * MINUTE);
                items.push(ReminderItem {
                    kind,
                    key: format!("ev:{}:{}", o.event_id, o.original_start),
                    title: o.title,
                    // Still running when the window starts: shown on its first day.
                    date: starts.date().max(from).to_string(),
                    time: (!o.all_day).then(|| hm(starts.time())),
                    at: o.start_at,
                    remind_at,
                    repeat: rules
                        .get(&o.event_id)
                        .map(|r| describe_event_rule(r, Some(starts.date()))),
                    projected: false,
                    overdue: false,
                    task_id: o.task_id,
                    event_id: Some(o.event_id),
                    original_start: Some(o.original_start),
                    end_at: Some(o.end_at),
                    project_id: o.project_id,
                    project_name: o.project_name,
                    client_name: o.client_name,
                    color: o.color,
                    meeting_url: (!o.meeting_url.is_empty()).then_some(o.meeting_url),
                });
            }
        }

        // Overdue first, then by time; untimed tasks before timed ones on a day.
        items.sort_by(|a, b| {
            b.overdue
                .cmp(&a.overdue)
                .then(a.date.cmp(&b.date))
                .then(a.time.is_some().cmp(&b.time.is_some()))
                .then(a.at.cmp(&b.at))
                .then(a.title.cmp(&b.title))
        });
        // Too many: keep whole days up to the cap and say where it ends.
        let mut to = last.to_string();
        let truncated = items.len() > MAX_ITEMS;
        if truncated {
            let cut = items[MAX_ITEMS].date.clone();
            items.retain(|i| i.overdue || i.date < cut);
            to = cut
                .parse::<Date>()
                .ok()
                .and_then(|d| d.yesterday().ok())
                .map_or(cut, |d| d.to_string());
        }
        Ok(UpcomingReminders {
            reminders_on: s.enabled,
            meeting_lead_min: s.meeting_lead_min,
            task_lead_min: s.task_lead_min,
            from: from.to_string(),
            to,
            truncated,
            items,
        })
    }

    /// How many reminders will still fire today (the sidebar badge): timed
    /// open tasks due later today and today's meetings that haven't started.
    pub fn reminders_left_today(&self, now: i64) -> Result<usize> {
        let s = self.notify_settings()?;
        if !s.enabled {
            return Ok(0);
        }
        let (tz, _) = local_zone();
        let today = jiff::Timestamp::from_millisecond(now)
            .map(|t| t.to_zoned(tz.clone()).date())
            .map_err(|e| invalid(e.to_string()))?;
        let (task_leads, event_leads) = self.own_leads()?;
        let mut stmt = self.conn().prepare(
            "SELECT t.id, t.due_time FROM task t
               JOIN project p ON p.id = t.project_id
              WHERE t.deleted_at IS NULL AND t.archived_at IS NULL AND t.completed_at IS NULL
                AND p.archived_at IS NULL AND t.due_date = ?1 AND t.due_time IS NOT NULL",
        )?;
        let mut left = 0;
        for row in stmt.query_map([today.to_string()], |r| {
            Ok((r.get::<_, Id>(0)?, r.get::<_, String>(1)?))
        })? {
            let (id, time) = row?;
            let lead = task_leads.get(&id).copied().unwrap_or(s.task_lead_min);
            if let Ok(time) = time.parse::<Time>()
                && local_ms(today, time).is_some_and(|at| at - lead * MINUTE > now)
            {
                left += 1;
            }
        }
        let end =
            local_ms(today.tomorrow().unwrap_or(today), Time::midnight()).unwrap_or(now + DAY);
        if end > now {
            left += self
                .occurrences(now, end)?
                .iter()
                .filter(|o| {
                    o.kind == EventKind::Meeting
                        && o.status != EventStatus::Cancelled
                        && !o.all_day
                        && o.start_at
                            - event_leads
                                .get(&o.event_id)
                                .copied()
                                .unwrap_or(s.meeting_lead_min)
                                * MINUTE
                            > now
                })
                .count();
        }
        Ok(left)
    }

    /// Every repeating task and event series that hasn't ended, soonest first.
    pub fn repeating(&self, now: i64) -> Result<Vec<RepeatingItem>> {
        let mut out = Vec::new();
        for t in self.repeating_tasks()? {
            let Some(rule) = t.rule else { continue };
            let after_completion = matches!(TaskRule::parse(&rule), Ok(TaskRule::After { .. }));
            out.push(RepeatingItem {
                kind: ReminderKind::Task,
                title: t.title,
                describe: describe_task_rule(&rule, t.due),
                rule,
                after_completion,
                next_date: t.due.map(|d| d.to_string()),
                next_time: t.time.map(hm),
                next_at: t
                    .due
                    .and_then(|d| local_ms(d, t.time.unwrap_or(Time::midnight()))),
                next_end_at: None,
                all_day: false,
                task_id: Some(t.id),
                event_id: None,
                original_start: None,
                project_id: Some(t.project_id),
                project_name: Some(t.project_name),
                client_name: t.client_name,
                color: t.color,
            });
        }
        let rules = self.event_rules()?;
        if !rules.is_empty() {
            // The next occurrence of each series within the longest window allowed.
            let mut seen = HashMap::new();
            for o in self.occurrences(now, now + 399 * DAY)? {
                if !rules.contains_key(&o.event_id)
                    || o.end_at <= now
                    || o.status == EventStatus::Cancelled
                    || seen.contains_key(&o.event_id)
                {
                    continue;
                }
                seen.insert(o.event_id, ());
                let rule = rules.get(&o.event_id).cloned().unwrap_or_default();
                let (tz, _) = local_zone();
                let starts = jiff::Timestamp::from_millisecond(o.start_at)
                    .map(|t| t.to_zoned(tz))
                    .map_err(|e| invalid(e.to_string()))?;
                out.push(RepeatingItem {
                    kind: if o.kind == EventKind::Block {
                        ReminderKind::Block
                    } else {
                        ReminderKind::Meeting
                    },
                    title: o.title,
                    describe: describe_event_rule(&rule, Some(starts.date())),
                    rule,
                    after_completion: false,
                    next_date: Some(starts.date().to_string()),
                    next_time: (!o.all_day).then(|| hm(starts.time())),
                    next_at: Some(o.start_at),
                    next_end_at: Some(o.end_at),
                    all_day: o.all_day,
                    task_id: o.task_id,
                    event_id: Some(o.event_id),
                    original_start: Some(o.original_start),
                    project_id: o.project_id,
                    project_name: o.project_name,
                    client_name: o.client_name,
                    color: o.color,
                });
            }
        }
        // Soonest first; undated repeating tasks last.
        out.sort_by(|a, b| {
            a.next_at
                .is_none()
                .cmp(&b.next_at.is_none())
                .then(a.next_at.cmp(&b.next_at))
                .then(a.title.cmp(&b.title))
        });
        Ok(out)
    }
}
