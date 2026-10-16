//! Windowed task queries for every view (SPEC §3: no view loads all tasks).

use rusqlite::params_from_iter;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::Result;
use crate::id::Id;
use crate::model::{Category, Priority, clean_date, text};
use crate::storage::Db;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TaskSort {
    #[default]
    Manual,
    Due,
    Priority,
    Created,
    Updated,
    Title,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct TaskQuery {
    #[ts(optional)]
    pub client_id: Option<Id>,
    #[ts(optional)]
    pub project_id: Option<Id>,
    /// Only projects without a client (Inbox excluded).
    #[ts(as = "Option<bool>", optional)]
    pub personal: bool,
    /// Empty = every category.
    #[ts(as = "Option<Vec<Category>>", optional)]
    pub categories: Vec<Category>,
    #[ts(optional)]
    pub search: Option<String>,
    #[ts(optional)]
    pub tag: Option<String>,
    #[ts(optional)]
    pub due_from: Option<String>,
    #[ts(optional)]
    pub due_to: Option<String>,
    /// Only direct subtasks of this task.
    #[ts(optional)]
    pub parent_id: Option<Id>,
    /// Only tasks without a parent.
    #[ts(as = "Option<bool>", optional)]
    pub top_level: bool,
    #[ts(as = "Option<bool>", optional)]
    pub include_archived: bool,
    #[ts(as = "Option<TaskSort>", optional)]
    pub sort: TaskSort,
    #[ts(as = "Option<bool>", optional)]
    pub descending: bool,
    #[ts(as = "Option<i64>", optional)]
    pub offset: i64,
    /// Default 200, max 1000.
    #[ts(as = "Option<i64>", optional)]
    pub limit: i64,
}

/// One row for Table / Board / Calendar, with everything needed to render it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TaskRow {
    pub id: Id,
    pub title: String,
    pub priority: Priority,
    pub status_id: Id,
    pub status_name: String,
    pub status_color: String,
    pub category: Category,
    pub project_id: Id,
    pub project_name: String,
    pub project_color: String,
    pub client_id: Option<Id>,
    pub client_name: Option<String>,
    pub parent_id: Option<Id>,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub estimate_minutes: Option<i64>,
    pub completed_at: Option<i64>,
    pub sort_key: String,
    pub subtasks_total: i64,
    pub subtasks_done: i64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TaskPage {
    pub rows: Vec<TaskRow>,
    pub total: i64,
}

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;
const TAG_SEP: char = '\u{1f}';

/// Turns free text into a safe FTS5 query: every word must match as a prefix.
pub(crate) fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn order_by(sort: TaskSort, desc: bool) -> String {
    let dir = if desc { "DESC" } else { "ASC" };
    match sort {
        TaskSort::Manual => format!("t.sort_key {dir}, t.rid"),
        TaskSort::Due => format!(
            "t.due_date IS NULL, t.due_date {dir}, t.due_time IS NULL, t.due_time {dir}, t.sort_key"
        ),
        TaskSort::Priority => format!(
            "t.priority {}, t.due_date IS NULL, t.due_date, t.sort_key",
            if desc { "ASC" } else { "DESC" }
        ),
        TaskSort::Created => format!("t.created_at {dir}, t.rid"),
        TaskSort::Updated => format!("t.updated_at {dir}, t.rid"),
        TaskSort::Title => format!("t.title COLLATE NOCASE {dir}, t.rid"),
    }
}

impl Db {
    pub fn list_tasks(&self, q: &TaskQuery) -> Result<TaskPage> {
        let mut filters = vec![
            "t.deleted_at IS NULL".to_owned(),
            "p.deleted_at IS NULL".to_owned(),
        ];
        let mut args: Vec<Value> = Vec::new();

        if !q.include_archived {
            filters.push("t.archived_at IS NULL AND p.archived_at IS NULL".into());
        }
        if let Some(client) = q.client_id {
            filters.push("p.client_id = ?".into());
            args.push(client.into());
        }
        if let Some(project) = q.project_id {
            filters.push("t.project_id = ?".into());
            args.push(project.into());
        }
        if q.personal {
            filters.push("p.client_id IS NULL AND p.is_inbox = 0".into());
        }
        if !q.categories.is_empty() {
            let marks = vec!["?"; q.categories.len()].join(", ");
            filters.push(format!("s.category IN ({marks})"));
            args.extend(q.categories.iter().map(|c| text(c.as_str())));
        }
        if let Some(search) = q.search.as_deref().and_then(fts_query) {
            filters.push("t.rid IN (SELECT rowid FROM task_fts WHERE task_fts MATCH ?)".into());
            args.push(text(search));
        }
        if let Some(tag) = q.tag.as_deref().map(|t| t.trim().trim_start_matches('#')) {
            filters.push(
                "EXISTS (SELECT 1 FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                         WHERE tt.task_id = t.id AND g.name = ? COLLATE NOCASE)"
                    .into(),
            );
            args.push(text(tag));
        }
        if let Some(from) = &q.due_from {
            filters.push("t.due_date >= ?".into());
            args.push(text(clean_date(from)?));
        }
        if let Some(to) = &q.due_to {
            filters.push("t.due_date <= ?".into());
            args.push(text(clean_date(to)?));
        }
        if let Some(parent) = q.parent_id {
            filters.push("t.parent_id = ?".into());
            args.push(parent.into());
        } else if q.top_level {
            filters.push("t.parent_id IS NULL".into());
        }

        let from = "FROM task t
                    JOIN project p ON p.id = t.project_id
                    JOIN status s ON s.id = t.status_id
                    LEFT JOIN client c ON c.id = p.client_id";
        let where_sql = filters.join(" AND ");

        let total: i64 = self.conn().query_row(
            &format!("SELECT count(*) {from} WHERE {where_sql}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;

        let limit = if q.limit <= 0 {
            DEFAULT_LIMIT
        } else {
            q.limit.min(MAX_LIMIT)
        };
        let sql = format!(
            "SELECT t.id, t.title, t.priority, t.status_id, s.name, s.color, s.category,
                    t.project_id, p.name, p.color, p.client_id, c.name, t.parent_id,
                    t.start_date, t.due_date, t.due_time, t.estimate_minutes, t.completed_at,
                    t.sort_key,
                    (SELECT count(*) FROM task x WHERE x.parent_id = t.id AND x.deleted_at IS NULL),
                    (SELECT count(*) FROM task x WHERE x.parent_id = t.id AND x.deleted_at IS NULL
                       AND x.completed_at IS NOT NULL),
                    (SELECT group_concat(g.name, char(31)) FROM task_tag tt
                       JOIN tag g ON g.id = tt.tag_id WHERE tt.task_id = t.id)
             {from} WHERE {where_sql}
             ORDER BY {} LIMIT ? OFFSET ?",
            order_by(q.sort, q.descending)
        );
        args.push(Value::Integer(limit));
        args.push(Value::Integer(q.offset.max(0)));

        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                let category: String = r.get(6)?;
                let tags: Option<String> = r.get(21)?;
                Ok(TaskRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    priority: Priority::from_i64(r.get(2)?),
                    status_id: r.get(3)?,
                    status_name: r.get(4)?,
                    status_color: r.get(5)?,
                    category: Category::parse(&category).unwrap_or(Category::Todo),
                    project_id: r.get(7)?,
                    project_name: r.get(8)?,
                    project_color: r.get(9)?,
                    client_id: r.get(10)?,
                    client_name: r.get(11)?,
                    parent_id: r.get(12)?,
                    start_date: r.get(13)?,
                    due_date: r.get(14)?,
                    due_time: r.get(15)?,
                    estimate_minutes: r.get(16)?,
                    completed_at: r.get(17)?,
                    sort_key: r.get(18)?,
                    subtasks_total: r.get(19)?,
                    subtasks_done: r.get(20)?,
                    tags: tags
                        .map(|t| t.split(TAG_SEP).map(str::to_owned).collect())
                        .unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(TaskPage { rows, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_is_safe() {
        assert_eq!(
            fts_query("write report").as_deref(),
            Some("\"write\"* \"report\"*")
        );
        assert_eq!(
            fts_query("  \"quote\" OR -x ").as_deref(),
            Some("\"quote\"* \"OR\"* \"x\"*")
        );
        assert_eq!(fts_query("  ***  "), None);
    }
}
