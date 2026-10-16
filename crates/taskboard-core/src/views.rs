//! Read models for the app shell: sidebar counts and Quick Add previews.

use jiff::ToSpan;
use jiff::civil::Date;
use serde::Serialize;
use ts_rs::TS;

use crate::error::Result;
use crate::id::Id;
use crate::model::Category;
use crate::query::TaskQuery;
use crate::quickadd::{ParsedQuickAdd, parse};
use crate::storage::Db;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ProjectRef {
    pub id: Id,
    pub name: String,
    pub color: String,
    pub client_name: Option<String>,
}

/// What Quick Add would create, shown live while typing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct QuickAddPreview {
    pub parsed: ParsedQuickAdd,
    pub project: Option<ProjectRef>,
    /// An `@hint` was typed but matches no project.
    pub project_missing: bool,
    /// Words for a typed repeat rule, e.g. "Every week on Fri".
    pub recurrence_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SidebarCounts {
    pub inbox: i64,
    /// Open tasks due today or earlier.
    pub today: i64,
    pub overdue: i64,
    /// Open tasks due in the next 7 days (after today).
    pub upcoming: i64,
}

/// Footer numbers for the Table view, over the whole query (not just the page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TaskSummary {
    pub total: i64,
    pub done: i64,
    pub estimate_minutes: i64,
}

fn open() -> Vec<Category> {
    vec![Category::Todo, Category::Active]
}

impl Db {
    pub fn preview_quick_add(&self, line: &str, today: Date) -> Result<QuickAddPreview> {
        let parsed = parse(line, today);
        let (project, project_missing) = match &parsed.project_hint {
            Some(hint) => match self.find_project(hint)? {
                Some(p) => (
                    Some(ProjectRef {
                        id: p.project.id,
                        name: p.project.name,
                        color: p.project.color,
                        client_name: p.client_name,
                    }),
                    false,
                ),
                None => (None, true),
            },
            None => (None, false),
        };
        let recurrence_text = parsed
            .recurrence_rule
            .as_deref()
            .and_then(|r| crate::recur::TaskRule::parse(r).ok())
            .map(|r| r.describe());
        Ok(QuickAddPreview {
            parsed,
            project,
            project_missing,
            recurrence_text,
        })
    }

    /// Total matching tasks without fetching rows.
    pub fn count_tasks(&self, q: &TaskQuery) -> Result<i64> {
        Ok(self
            .list_tasks(&TaskQuery {
                limit: 1,
                ..q.clone()
            })?
            .total)
    }

    pub fn summarize_tasks(&self, q: &TaskQuery) -> Result<TaskSummary> {
        let (where_sql, args) = self.task_filter(q)?;
        let (total, done, estimate) = self.conn().query_row(
            &format!(
                "SELECT count(*), count(t.completed_at), coalesce(sum(t.estimate_minutes), 0)
                 FROM task t WHERE {where_sql}"
            ),
            rusqlite::params_from_iter(args.iter()),
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(TaskSummary {
            total,
            done,
            estimate_minutes: estimate,
        })
    }

    pub fn sidebar_counts(&self, today: Date) -> Result<SidebarCounts> {
        let day = |d: Date| Some(d.to_string());
        let yesterday = today.yesterday().ok();
        let week_end = today.checked_add(7.days()).ok();
        Ok(SidebarCounts {
            inbox: self.count_tasks(&TaskQuery {
                project_id: Some(self.inbox_id()?),
                categories: open(),
                ..Default::default()
            })?,
            today: self.count_tasks(&TaskQuery {
                due_to: day(today),
                categories: open(),
                ..Default::default()
            })?,
            overdue: match yesterday {
                Some(y) => self.count_tasks(&TaskQuery {
                    due_to: day(y),
                    categories: open(),
                    ..Default::default()
                })?,
                None => 0,
            },
            upcoming: match (today.tomorrow().ok(), week_end) {
                (Some(from), Some(to)) => self.count_tasks(&TaskQuery {
                    due_from: day(from),
                    due_to: day(to),
                    categories: open(),
                    ..Default::default()
                })?,
                _ => 0,
            },
        })
    }
}
