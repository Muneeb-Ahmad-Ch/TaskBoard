//! Dashboard numbers (SPEC §5 Tracking): this week vs last, overdue,
//! streaks, a two-week completion trend, per-client summary and per-project
//! progress with a burndown reconstructed from created/completed timestamps.

use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::ToSpan;
use jiff::civil::{Date, Time};
use serde::Serialize;
use ts_rs::TS;

use crate::error::Result;
use crate::events::{EventKind, EventStatus, local_zone};
use crate::id::Id;
use crate::storage::Db;

const TREND_DAYS: i64 = 14;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DayCount {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ClientSummary {
    /// The group, or None for a section's projects that have no group.
    pub client_id: Option<Id>,
    pub section_id: Option<Id>,
    /// The section's name (the row's own name for section rows).
    pub section_name: String,
    pub name: String,
    pub color: String,
    pub open: i64,
    pub overdue: i64,
    pub done_this_week: i64,
    pub logged_min_this_week: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProjectProgress {
    pub project_id: Id,
    pub name: String,
    pub client_name: Option<String>,
    pub color: String,
    pub done: i64,
    pub total: i64,
    pub open: i64,
    pub deadline: Option<String>,
    /// Open tasks at the end of each of the last 14 days (oldest first).
    pub burndown: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Dashboard {
    pub completed_this_week: i64,
    pub completed_last_week: i64,
    pub due_today: i64,
    pub overdue: i64,
    pub due_next_7_days: i64,
    pub meetings_today: i64,
    pub logged_min_this_week: i64,
    /// Consecutive days (ending today, or yesterday if nothing yet today)
    /// with at least one completed task.
    pub streak_days: i64,
    pub best_streak_days: i64,
    pub completions: Vec<DayCount>,
    pub clients: Vec<ClientSummary>,
    pub projects: Vec<ProjectProgress>,
}

impl Db {
    pub fn dashboard(&self, today: Date) -> Result<Dashboard> {
        let (tz, _) = local_zone();
        let start_of = |d: Date| -> i64 {
            tz.to_zoned(d.to_datetime(Time::midnight()))
                .map(|z| z.timestamp().as_millisecond())
                .unwrap_or(0)
        };
        let date_of = |ms: i64| -> Option<Date> {
            jiff::Timestamp::from_millisecond(ms)
                .ok()
                .map(|t| t.to_zoned(tz.clone()).date())
        };
        let week_start = today
            .checked_sub(i64::from(today.weekday().to_monday_zero_offset()).days())
            .unwrap_or(today);
        let last_week_start = week_start.checked_sub(7.days()).unwrap_or(week_start);
        let tomorrow = today.tomorrow().unwrap_or(today);
        let (ws, lws, now_end) = (
            start_of(week_start),
            start_of(last_week_start),
            start_of(tomorrow),
        );

        // Completions over the past year, bucketed by local date.
        let since = start_of(today.checked_sub(400.days()).unwrap_or(today));
        let mut by_day: BTreeMap<Date, i64> = BTreeMap::new();
        {
            let mut stmt = self.conn().prepare(
                // Finished work: cancelled tasks are closed but not done.
                "SELECT t.completed_at FROM task t JOIN status st ON st.id = t.status_id
                 WHERE t.completed_at >= ?1 AND t.deleted_at IS NULL AND st.category = 'done'",
            )?;
            for ms in stmt.query_map([since], |r| r.get::<_, i64>(0))? {
                if let Some(d) = date_of(ms?) {
                    *by_day.entry(d).or_default() += 1;
                }
            }
        }
        let count_between = |a: i64, b: i64| -> i64 {
            by_day
                .iter()
                .filter(|(d, _)| {
                    let s = start_of(**d);
                    s >= a && s < b
                })
                .map(|(_, n)| n)
                .sum()
        };

        // Streaks.
        let mut streak = 0;
        let mut cursor = if by_day.contains_key(&today) {
            today
        } else {
            today.yesterday().unwrap_or(today)
        };
        while by_day.contains_key(&cursor) {
            streak += 1;
            match cursor.yesterday() {
                Ok(d) => cursor = d,
                Err(_) => break,
            }
        }
        let mut best = 0;
        let mut run = 0;
        let mut prev: Option<Date> = None;
        for d in by_day.keys() {
            run = match prev {
                Some(p) if p.tomorrow().ok() == Some(*d) => run + 1,
                _ => 1,
            };
            best = best.max(run);
            prev = Some(*d);
        }

        let completions = (0..TREND_DAYS)
            .rev()
            .filter_map(|i| today.checked_sub(i.days()).ok())
            .map(|d| DayCount {
                date: d.to_string(),
                count: by_day.get(&d).copied().unwrap_or(0),
            })
            .collect();

        let counts = self.sidebar_counts(today)?;
        let meetings_today = self
            .occurrences(start_of(today), now_end)?
            .iter()
            .filter(|o| {
                o.kind == EventKind::Meeting && o.status != EventStatus::Cancelled && !o.all_day
            })
            .count() as i64;
        let hours = self.hours_by_client(
            ws,
            start_of(week_start.checked_add(7.days()).unwrap_or(week_start)),
        )?;
        // Rows are keyed by group, or by section for projects without a group.
        let logged_by_client: HashMap<Option<Id>, i64> = hours
            .iter()
            .map(|h| (h.client_id.or(h.section_id), h.logged_min))
            .collect();

        // Per group (and per section for projects without a group): open /
        // overdue / done this week, top-level tasks.
        let today_s = today.to_string();
        let mut clients: Vec<ClientSummary> = Vec::new();
        {
            let mut stmt = self.conn().prepare(
                "SELECT p.client_id, p.section_id, coalesce(s.name, ''),
                        coalesce(c.name, s.name, 'Other'), coalesce(c.color, s.color, 'gray'),
                        count(CASE WHEN t.completed_at IS NULL THEN 1 END),
                        count(CASE WHEN t.completed_at IS NULL AND t.due_date < ?1 THEN 1 END),
                        count(CASE WHEN t.completed_at >= ?2 AND st.category = 'done' THEN 1 END)
                 FROM task t JOIN project p ON p.id = t.project_id
                 JOIN status st ON st.id = t.status_id
                 LEFT JOIN client c ON c.id = p.client_id
                 LEFT JOIN section s ON s.id = p.section_id
                 WHERE t.deleted_at IS NULL AND t.archived_at IS NULL AND t.parent_id IS NULL
                   AND p.deleted_at IS NULL AND p.archived_at IS NULL AND p.is_inbox = 0
                   AND (c.id IS NULL OR c.archived_at IS NULL)
                 GROUP BY p.client_id, p.section_id
                 ORDER BY s.sort_key, c.sort_key IS NULL, c.sort_key",
            )?;
            let rows = stmt.query_map(rusqlite::params![today_s, ws], |r| {
                Ok((
                    (r.get::<_, Option<Id>>(0)?, r.get::<_, Option<Id>>(1)?),
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            })?;
            for row in rows {
                let ((client_id, section_id), section_name, name, color, open, overdue, done) =
                    row?;
                clients.push(ClientSummary {
                    client_id,
                    section_id,
                    section_name,
                    name,
                    color,
                    open,
                    overdue,
                    done_this_week: done,
                    logged_min_this_week: logged_by_client
                        .get(&client_id.or(section_id))
                        .copied()
                        .unwrap_or(0),
                });
            }
        }

        // Projects with open work: progress + 14-day burndown.
        let mut projects: Vec<ProjectProgress> = self
            .projects(false)?
            .into_iter()
            .filter(|p| !p.project.is_inbox && p.total_tasks > 0)
            .map(|p| ProjectProgress {
                project_id: p.project.id,
                name: p.project.name.clone(),
                client_name: p.client_name.clone(),
                color: p.project.color.clone(),
                done: p.done_tasks,
                total: p.total_tasks,
                open: p.open_tasks,
                deadline: p.project.deadline.clone(),
                burndown: Vec::new(),
            })
            .collect();
        projects.sort_by_key(|p| std::cmp::Reverse(p.open));
        projects.truncate(8);
        let ids: HashSet<Id> = projects.iter().map(|p| p.project_id).collect();
        let day_ends: Vec<i64> = (0..TREND_DAYS)
            .rev()
            .filter_map(|i| today.checked_sub(i.days()).ok())
            .map(|d| start_of(d.tomorrow().unwrap_or(d)))
            .collect();
        let mut lifetimes: HashMap<Id, Vec<(i64, Option<i64>)>> = HashMap::new();
        {
            let mut stmt = self.conn().prepare(
                "SELECT project_id, created_at, completed_at FROM task
                 WHERE deleted_at IS NULL AND parent_id IS NULL",
            )?;
            for row in stmt.query_map([], |r| {
                Ok((
                    r.get::<_, Id>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })? {
                let (p, created, completed) = row?;
                if ids.contains(&p) {
                    lifetimes.entry(p).or_default().push((created, completed));
                }
            }
        }
        for p in &mut projects {
            let life = lifetimes
                .get(&p.project_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            p.burndown = day_ends
                .iter()
                .map(|&end| {
                    life.iter()
                        .filter(|(c, done)| *c < end && done.is_none_or(|d| d >= end))
                        .count() as i64
                })
                .collect();
        }

        Ok(Dashboard {
            completed_this_week: count_between(ws, now_end),
            completed_last_week: count_between(lws, ws),
            due_today: counts.today - counts.overdue,
            overdue: counts.overdue,
            due_next_7_days: counts.upcoming,
            meetings_today,
            logged_min_this_week: hours.iter().map(|h| h.logged_min).sum(),
            streak_days: streak,
            best_streak_days: best,
            completions,
            clients,
            projects,
        })
    }
}
