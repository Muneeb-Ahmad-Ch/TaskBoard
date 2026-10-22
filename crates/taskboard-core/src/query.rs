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
    /// Board column: to do, then active, then finished; board order within.
    Status,
    /// Group name, then project name.
    Project,
    Progress,
    /// Tasks without an estimate come last either way.
    Estimate,
    /// By the first tag alphabetically; untagged tasks come last either way.
    Tags,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct TaskQuery {
    /// Just this task (e.g. to read its computed progress).
    #[ts(optional)]
    pub id: Option<Id>,
    #[ts(optional)]
    pub client_id: Option<Id>,
    #[ts(optional)]
    pub project_id: Option<Id>,
    /// Every project in this sidebar section (grouped or not).
    #[ts(optional)]
    pub section_id: Option<Id>,
    /// Empty = every category.
    #[ts(as = "Option<Vec<Category>>", optional)]
    pub categories: Vec<Category>,
    #[ts(optional)]
    pub search: Option<String>,
    #[ts(optional)]
    pub tag: Option<String>,
    /// Only tasks whose status has this name (case-insensitive). Board columns
    /// merge same-named statuses across projects.
    #[ts(optional)]
    pub status_name: Option<String>,
    /// Only tasks in any of these columns (by name, case-insensitive).
    #[ts(as = "Option<Vec<String>>", optional)]
    pub status_names: Vec<String>,
    /// Only tasks with any of these priorities.
    #[ts(as = "Option<Vec<Priority>>", optional)]
    pub priorities: Vec<Priority>,
    /// Only tasks with any of these tags (case-insensitive, leading # ignored).
    #[ts(as = "Option<Vec<String>>", optional)]
    pub tags: Vec<String>,
    /// Only tasks in any of these projects.
    #[ts(as = "Option<Vec<Id>>", optional)]
    pub project_ids: Vec<Id>,
    #[ts(optional)]
    pub due_from: Option<String>,
    #[ts(optional)]
    pub due_to: Option<String>,
    /// Only tasks without a due date (the calendar's "unscheduled" list).
    #[ts(as = "Option<bool>", optional)]
    pub undated: bool,
    /// Only tasks with a due date.
    #[ts(as = "Option<bool>", optional)]
    pub dated: bool,
    /// Only direct subtasks of this task.
    #[ts(optional)]
    pub parent_id: Option<Id>,
    /// Only tasks without a parent.
    #[ts(as = "Option<bool>", optional)]
    pub top_level: bool,
    #[ts(as = "Option<bool>", optional)]
    pub include_archived: bool,
    /// Only tasks archived on their own (Settings → Data → Archived).
    #[ts(as = "Option<bool>", optional)]
    pub archived_only: bool,
    /// Only tasks completed at or after this moment (Unix ms), e.g. "done today".
    #[ts(optional)]
    pub completed_from: Option<i64>,
    /// Only tasks added at or after / at or before this moment (Unix ms).
    #[ts(optional)]
    pub created_from: Option<i64>,
    #[ts(optional)]
    pub created_to: Option<i64>,
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
    /// 0–100: 100 when closed; else the average of direct subtasks (done = 100,
    /// open = their manual progress); else the task's own manual progress.
    pub progress: i64,
    pub recurring: bool,
    /// Title of the parent task, for subtasks shown outside their parent.
    pub parent_title: Option<String>,
    /// When the task was archived (only seen with `include_archived` / `archived_only`).
    pub archived_at: Option<i64>,
    /// When the task was added (Unix ms).
    pub created_at: i64,
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

/// A task's 0–100 progress (see `TaskRow::progress`), over `task t`.
const PROGRESS: &str = "CASE WHEN t.completed_at IS NOT NULL THEN 100
     ELSE coalesce(
       (SELECT CAST(round(avg(CASE WHEN x.completed_at IS NOT NULL THEN 100
                                   ELSE coalesce(x.progress, 0) END)) AS INTEGER)
          FROM task x WHERE x.parent_id = t.id AND x.deleted_at IS NULL),
       t.progress, 0)
END";

/// ORDER BY clauses written to match the migration-3 indexes exactly, so
/// SQLite can walk an index in order and stop after one page.
fn order_by(sort: TaskSort, desc: bool) -> String {
    let dir = if desc { "DESC" } else { "ASC" };
    match sort {
        TaskSort::Manual => format!("t.sort_key {dir}, t.rid {dir}"),
        TaskSort::Due => format!(
            "coalesce(t.due_date, '~') {dir}, coalesce(t.due_time, '~') {dir}, t.sort_key {dir}"
        ),
        TaskSort::Priority => {
            let (p, rest) = if desc {
                ("ASC", "DESC")
            } else {
                ("DESC", "ASC")
            };
            format!("t.priority {p}, coalesce(t.due_date, '~') {rest}, t.sort_key {rest}")
        }
        TaskSort::Created => format!("t.created_at {dir}, t.rid {dir}"),
        TaskSort::Updated => format!("t.updated_at {dir}, t.rid {dir}"),
        TaskSort::Title => format!("t.title COLLATE NOCASE {dir}, t.rid {dir}"),
        // The rest sort by values outside `task`, so they can't walk an index.
        TaskSort::Status => format!(
            "(SELECT CASE s.category WHEN 'todo' THEN 0 WHEN 'active' THEN 1
                                     WHEN 'done' THEN 2 ELSE 3 END
                FROM status s WHERE s.id = t.status_id) {dir},
             (SELECT s.sort_key FROM status s WHERE s.id = t.status_id) {dir},
             (SELECT s.name FROM status s WHERE s.id = t.status_id) COLLATE NOCASE {dir},
             t.sort_key {dir}, t.rid {dir}"
        ),
        TaskSort::Project => format!(
            "(SELECT coalesce(c.name, '') FROM project p LEFT JOIN client c ON c.id = p.client_id
                WHERE p.id = t.project_id) COLLATE NOCASE {dir},
             (SELECT p.name FROM project p WHERE p.id = t.project_id) COLLATE NOCASE {dir},
             t.project_id {dir}, t.sort_key {dir}, t.rid {dir}"
        ),
        TaskSort::Progress => format!("{PROGRESS} {dir}, t.sort_key {dir}, t.rid {dir}"),
        TaskSort::Estimate => format!(
            "t.estimate_minutes IS NULL, t.estimate_minutes {dir}, t.sort_key {dir}, t.rid {dir}"
        ),
        TaskSort::Tags => {
            let first = "(SELECT min(g.name COLLATE NOCASE) FROM task_tag tt
                            JOIN tag g ON g.id = tt.tag_id WHERE tt.task_id = t.id)";
            format!("{first} IS NULL, {first} {dir}, t.sort_key {dir}, t.rid {dir}")
        }
    }
}

const OPEN: [Category; 2] = [Category::Todo, Category::Active];
const CLOSED: [Category; 2] = [Category::Done, Category::Cancelled];

impl Db {
    pub fn list_tasks(&self, q: &TaskQuery) -> Result<TaskPage> {
        let (where_sql, mut args) = self.task_filter(q)?;
        self.task_page(q, &where_sql, &mut args)
    }

    /// Tags on live, unarchived tasks, most used first (for filter pickers).
    pub fn tag_names(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare(
            "SELECT g.name FROM tag g
               JOIN task_tag tt ON tt.tag_id = g.id
               JOIN task t ON t.id = tt.task_id
              WHERE g.deleted_at IS NULL AND t.deleted_at IS NULL AND t.archived_at IS NULL
              GROUP BY g.id
              ORDER BY count(*) DESC, g.name COLLATE NOCASE",
        )?;
        let names = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(names)
    }

    /// WHERE clause (over `task t`) and its arguments for a query.
    pub(crate) fn task_filter(&self, q: &TaskQuery) -> Result<(String, Vec<Value>)> {
        // Filters reference only `task t` (plus small subqueries), so the
        // planner can use the partial indexes; names are joined per page below.
        let mut filters = vec!["t.deleted_at IS NULL".to_owned()];
        let mut args: Vec<Value> = Vec::new();

        let include_archived = q.include_archived || q.archived_only;
        if q.archived_only {
            filters.push("t.archived_at IS NOT NULL".into());
        } else if !include_archived {
            filters.push("t.archived_at IS NULL".into());
        }
        if q.client_id.is_some() || q.section_id.is_some() {
            // Selective: drive the query from the matching projects.
            let mut project_filter = vec!["deleted_at IS NULL".to_owned()];
            if !include_archived {
                project_filter.push("archived_at IS NULL".into());
            }
            if let Some(client) = q.client_id {
                project_filter.push("client_id = ?".into());
                args.push(client.into());
            }
            if let Some(section) = q.section_id {
                project_filter.push("section_id = ?".into());
                args.push(section.into());
            }
            filters.push(format!(
                "t.project_id IN (SELECT id FROM project WHERE {})",
                project_filter.join(" AND ")
            ));
        } else if !include_archived {
            // Deleting a project deletes its tasks, so only archived projects
            // need excluding. NOT IN over that short list keeps the planner on
            // the sort-order index instead of looking tasks up per project.
            filters.push(
                "t.project_id NOT IN (SELECT id FROM project WHERE archived_at IS NOT NULL)".into(),
            );
        }
        if let Some(project) = q.project_id {
            filters.push("t.project_id = ?".into());
            args.push(project.into());
        }

        // completed_at mirrors the closed categories, so open/closed filters
        // need no status lookup.
        let cats = &q.categories;
        let all_open = !cats.is_empty() && cats.iter().all(|c| OPEN.contains(c));
        let all_closed = !cats.is_empty() && cats.iter().all(|c| CLOSED.contains(c));
        if all_open {
            filters.push("t.completed_at IS NULL".into());
        } else if all_closed {
            filters.push("t.completed_at IS NOT NULL".into());
        }
        let covers = |set: &[Category]| set.iter().all(|c| cats.contains(c));
        let exact = (all_open && covers(&OPEN)) || (all_closed && covers(&CLOSED));
        if !cats.is_empty() && !exact {
            let marks = vec!["?"; cats.len()].join(", ");
            filters.push(format!(
                "t.status_id IN (SELECT id FROM status WHERE category IN ({marks}))"
            ));
            args.extend(cats.iter().map(|c| text(c.as_str())));
        }

        if let Some(search) = q.search.as_deref().and_then(fts_query) {
            filters.push("t.rid IN (SELECT rowid FROM task_fts WHERE task_fts MATCH ?)".into());
            args.push(text(search));
        }
        if let Some(id) = q.id {
            filters.push("t.id = ?".into());
            args.push(id.into());
        }
        if let Some(name) = q.status_name.as_deref() {
            filters.push(
                "t.status_id IN (SELECT id FROM status WHERE name = ? COLLATE NOCASE
                                 AND deleted_at IS NULL)"
                    .into(),
            );
            args.push(text(name.trim()));
        }
        if !q.status_names.is_empty() {
            let marks = vec!["?"; q.status_names.len()].join(", ");
            filters.push(format!(
                "t.status_id IN (SELECT id FROM status WHERE name COLLATE NOCASE IN ({marks})
                                 AND deleted_at IS NULL)"
            ));
            args.extend(q.status_names.iter().map(|n| text(n.trim())));
        }
        if !q.priorities.is_empty() {
            let marks = vec!["?"; q.priorities.len()].join(", ");
            filters.push(format!("t.priority IN ({marks})"));
            args.extend(q.priorities.iter().map(|p| Value::Integer(p.as_i64())));
        }
        if !q.tags.is_empty() {
            let marks = vec!["?"; q.tags.len()].join(", ");
            filters.push(format!(
                "t.id IN (SELECT tt.task_id FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                          WHERE g.name COLLATE NOCASE IN ({marks}))"
            ));
            args.extend(
                q.tags
                    .iter()
                    .map(|t| text(t.trim().trim_start_matches('#'))),
            );
        }
        if !q.project_ids.is_empty() {
            let marks = vec!["?"; q.project_ids.len()].join(", ");
            filters.push(format!("t.project_id IN ({marks})"));
            args.extend(q.project_ids.iter().map(|&p| Value::from(p)));
        }
        if let Some(tag) = q.tag.as_deref().map(|t| t.trim().trim_start_matches('#')) {
            filters.push(
                "t.id IN (SELECT tt.task_id FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                          WHERE g.name = ? COLLATE NOCASE)"
                    .into(),
            );
            args.push(text(tag));
        }
        // Written over coalesce(due_date, '~') to match the migration 3 indexes,
        // so date-range counts are index-only. '~' sorts after every date, so
        // `<= date` already excludes undated tasks and `< '~'` excludes them
        // from open-ended ranges.
        if let Some(from) = &q.due_from {
            filters
                .push("coalesce(t.due_date, '~') >= ? AND coalesce(t.due_date, '~') < '~'".into());
            args.push(text(clean_date(from)?));
        }
        if let Some(to) = &q.due_to {
            filters.push("coalesce(t.due_date, '~') <= ?".into());
            args.push(text(clean_date(to)?));
        }
        if q.undated {
            filters.push("coalesce(t.due_date, '~') = '~'".into());
        }
        if q.dated {
            filters.push("coalesce(t.due_date, '~') < '~'".into());
        }
        if let Some(from) = q.created_from {
            filters.push("t.created_at >= ?".into());
            args.push(Value::Integer(from));
        }
        if let Some(to) = q.created_to {
            filters.push("t.created_at <= ?".into());
            args.push(Value::Integer(to));
        }
        if let Some(from) = q.completed_from {
            filters.push("t.completed_at >= ?".into());
            args.push(Value::Integer(from));
        }
        if let Some(parent) = q.parent_id {
            filters.push("t.parent_id = ?".into());
            args.push(parent.into());
        } else if q.top_level {
            filters.push("t.parent_id IS NULL".into());
        }

        Ok((filters.join(" AND "), args))
    }

    fn task_page(&self, q: &TaskQuery, where_sql: &str, args: &mut Vec<Value>) -> Result<TaskPage> {
        let total: i64 = self.conn().query_row(
            &format!("SELECT count(*) FROM task t WHERE {where_sql}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;

        let limit = if q.limit <= 0 {
            DEFAULT_LIMIT
        } else {
            q.limit.min(MAX_LIMIT)
        };
        let order = order_by(q.sort, q.descending);
        // Pick the page's rows first (index walk + LIMIT), then join names for
        // just those rows. MATERIALIZED stops SQLite flattening the CTE.
        let sql = format!(
            "WITH page AS MATERIALIZED (
                SELECT t.rid FROM task t WHERE {where_sql} ORDER BY {order} LIMIT ? OFFSET ?
             )
             SELECT t.id, t.title, t.priority, t.status_id, s.name, s.color, s.category,
                    t.project_id, p.name, p.color, p.client_id, c.name, t.parent_id,
                    t.start_date, t.due_date, t.due_time, t.estimate_minutes, t.completed_at,
                    t.sort_key,
                    (SELECT count(*) FROM task x WHERE x.parent_id = t.id AND x.deleted_at IS NULL),
                    (SELECT count(*) FROM task x WHERE x.parent_id = t.id AND x.deleted_at IS NULL
                       AND x.completed_at IS NOT NULL),
                    (SELECT group_concat(g.name, char(31)) FROM task_tag tt
                       JOIN tag g ON g.id = tt.tag_id WHERE tt.task_id = t.id),
                    {PROGRESS},
                    t.recurrence_rule IS NOT NULL,
                    (SELECT x.title FROM task x WHERE x.id = t.parent_id),
                    t.archived_at, t.created_at
             FROM page
             JOIN task t ON t.rid = page.rid
             JOIN project p ON p.id = t.project_id
             JOIN status s ON s.id = t.status_id
             LEFT JOIN client c ON c.id = p.client_id
             ORDER BY {order}"
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
                    progress: r.get(22)?,
                    recurring: r.get(23)?,
                    parent_title: r.get(24)?,
                    archived_at: r.get(25)?,
                    created_at: r.get(26)?,
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
