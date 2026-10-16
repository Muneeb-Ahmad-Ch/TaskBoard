//! Tasks and subtasks: create, edit, complete, reorder, delete, restore.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, params};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{
    Category, NewTask, Priority, Task, TaskPatch, clean_date, clean_name, clean_time, int, opt_id,
    opt_int, opt_text, text,
};
use crate::order::key_between;
use crate::storage::Db;
use crate::tx::{Table, Tx};

const COLS: &str = "id, project_id, parent_id, status_id, title, description_md, priority,
                    start_date, due_date, due_time, estimate_minutes, sort_key, recurrence_rule,
                    completed_at, archived_at, deleted_at, created_at, updated_at";

fn map(r: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        project_id: r.get(1)?,
        parent_id: r.get(2)?,
        status_id: r.get(3)?,
        title: r.get(4)?,
        description_md: r.get(5)?,
        priority: Priority::from_i64(r.get(6)?),
        start_date: r.get(7)?,
        due_date: r.get(8)?,
        due_time: r.get(9)?,
        estimate_minutes: r.get(10)?,
        sort_key: r.get(11)?,
        recurrence_rule: r.get(12)?,
        completed_at: r.get(13)?,
        archived_at: r.get(14)?,
        deleted_at: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        tags: Vec::new(),
    })
}

/// Where to put a task in its column: between the task shown above it and the
/// one below it. Both `None` = end of the column.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct MoveTask {
    #[serde(default)]
    #[ts(optional)]
    pub status_id: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub above: Option<Id>,
    #[serde(default)]
    #[ts(optional)]
    pub below: Option<Id>,
}

// ---------- helpers that run inside a transaction ----------

fn load(tx: &Tx<'_>, id: Id) -> Result<Task> {
    tx.conn()
        .query_row(&format!("SELECT {COLS} FROM task WHERE id = ?1"), [id], map)
        .optional()?
        .ok_or(Error::NotFound("task"))
}

fn status_info(tx: &Tx<'_>, status: Id) -> Result<(Id, Category)> {
    let (project, category): (Id, String) = tx
        .conn()
        .query_row(
            "SELECT project_id, category FROM status WHERE id = ?1 AND deleted_at IS NULL",
            [status],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(Error::NotFound("status"))?;
    Ok((
        project,
        Category::parse(&category).unwrap_or(Category::Todo),
    ))
}

/// The status new tasks get: the last `todo` status ("To Do" by default).
fn default_status(tx: &Tx<'_>, project: Id) -> Result<Id> {
    tx.conn()
        .query_row(
            "SELECT id FROM status WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY category = 'todo' DESC, (CASE WHEN category = 'todo' THEN sort_key END) DESC,
                      sort_key
             LIMIT 1",
            [project],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(Error::NotFound("status"))
}

/// First status in `project` with `category`, else the default status.
fn status_for_category(tx: &Tx<'_>, project: Id, category: Category) -> Result<Id> {
    if category == Category::Todo {
        return default_status(tx, project);
    }
    let found: Option<Id> = tx
        .conn()
        .query_row(
            "SELECT id FROM status WHERE project_id = ?1 AND category = ?2 AND deleted_at IS NULL
             ORDER BY sort_key LIMIT 1",
            params![project, category.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    match found {
        Some(id) => Ok(id),
        None => default_status(tx, project),
    }
}

fn end_key(tx: &Tx<'_>, project: Id, status: Id) -> Result<String> {
    let last: Option<String> = tx.conn().query_row(
        "SELECT max(sort_key) FROM task WHERE project_id = ?1 AND status_id = ?2
         AND deleted_at IS NULL",
        params![project, status],
        |r| r.get(0),
    )?;
    key_between(last.as_deref(), None)
}

fn require_live_project(tx: &Tx<'_>, project: Id) -> Result<()> {
    let ok: bool = tx.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM project WHERE id = ?1 AND deleted_at IS NULL)",
        [project],
        |r| r.get(0),
    )?;
    if ok {
        Ok(())
    } else {
        Err(Error::NotFound("project"))
    }
}

/// Parent must be a live task in the same project and must not be `task` or
/// one of its descendants.
fn check_parent(tx: &Tx<'_>, task: Option<Id>, parent: Id, project: Id) -> Result<()> {
    let parent_task = load(tx, parent)?;
    if parent_task.deleted_at.is_some() {
        return Err(Error::NotFound("parent task"));
    }
    if parent_task.project_id != project {
        return Err(invalid(
            "a subtask must be in the same project as its parent",
        ));
    }
    if let Some(task) = task {
        let mut cursor = Some(parent);
        let mut hops = 0;
        while let Some(current) = cursor {
            if current == task {
                return Err(invalid("a task cannot be nested under itself"));
            }
            hops += 1;
            if hops > 10_000 {
                return Err(Error::Conflict("subtask chain is too deep".into()));
            }
            cursor = tx.conn().query_row(
                "SELECT parent_id FROM task WHERE id = ?1",
                [current],
                |r| r.get(0),
            )?;
        }
    }
    Ok(())
}

fn descendants(tx: &Tx<'_>, id: Id) -> Result<Vec<Id>> {
    let mut stmt = tx.conn().prepare(
        "WITH RECURSIVE d(id) AS (
            SELECT id FROM task WHERE parent_id = ?1
            UNION SELECT t.id FROM task t JOIN d ON t.parent_id = d.id
         ) SELECT id FROM d",
    )?;
    let ids = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

fn normalize_tags(names: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim().trim_start_matches('#').trim();
        if name.is_empty() {
            continue;
        }
        let name = clean_name(name, "tag")?;
        if !out.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    Ok(out)
}

fn set_tags(tx: &mut Tx<'_>, task: Id, names: &[String]) -> Result<()> {
    let wanted = normalize_tags(names)?;
    let mut wanted_ids = Vec::new();
    for name in &wanted {
        let existing: Option<Id> = tx
            .conn()
            .query_row(
                "SELECT id FROM tag WHERE name = ?1 COLLATE NOCASE AND deleted_at IS NULL",
                [name],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => id,
            None => {
                let id = Id::new();
                tx.insert(
                    Table::Tag,
                    &[
                        ("id", id.into()),
                        ("name", text(name.clone())),
                        ("color", text("gray")),
                        ("created_at", int(tx.now())),
                    ],
                )?;
                id
            }
        };
        wanted_ids.push(id);
    }
    let current: Vec<Id> = {
        let mut stmt = tx
            .conn()
            .prepare("SELECT tag_id FROM task_tag WHERE task_id = ?1")?;
        stmt.query_map([task], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for tag in current.iter().filter(|t| !wanted_ids.contains(t)) {
        tx.delete(Table::TaskTag, &[task.into(), (*tag).into()])?;
    }
    for tag in wanted_ids.iter().filter(|t| !current.contains(t)) {
        tx.insert(
            Table::TaskTag,
            &[("task_id", task.into()), ("tag_id", (*tag).into())],
        )?;
    }
    Ok(())
}

/// Changes a task's status, keeping `completed_at` consistent with the category.
fn status_changes(tx: &Tx<'_>, task: &Task, status: Id) -> Result<Vec<(&'static str, Value)>> {
    let (_, category) = status_info(tx, status)?;
    let completed_at = match (category.is_closed(), task.completed_at) {
        (true, Some(at)) => int(at),
        (true, None) => int(tx.now()),
        (false, _) => Value::Null,
    };
    Ok(vec![
        ("status_id", status.into()),
        ("completed_at", completed_at),
    ])
}

fn label_title(title: &str) -> String {
    let short: String = title.chars().take(40).collect();
    if short.len() < title.len() {
        format!("{short}…")
    } else {
        short
    }
}

impl Db {
    /// A live task with its tags.
    pub fn task(&self, id: Id) -> Result<Task> {
        let mut task = self
            .conn()
            .query_row(
                &format!("SELECT {COLS} FROM task WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("task"))?;
        let mut stmt = self.conn().prepare(
            "SELECT g.name FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
             WHERE tt.task_id = ?1 ORDER BY g.name COLLATE NOCASE",
        )?;
        task.tags = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(task)
    }

    pub fn create_task(&mut self, input: NewTask) -> Result<Task> {
        let title = clean_name(&input.title, "task title")?;
        let start = input.start_date.as_deref().map(clean_date).transpose()?;
        let due = input.due_date.as_deref().map(clean_date).transpose()?;
        let due_time = input.due_time.as_deref().map(clean_time).transpose()?;
        if due_time.is_some() && due.is_none() {
            return Err(invalid("a due time needs a due date"));
        }
        if let (Some(s), Some(d)) = (&start, &due)
            && s > d
        {
            return Err(invalid("start date is after the due date"));
        }
        if input.estimate_minutes.is_some_and(|m| m < 0) {
            return Err(invalid("estimate cannot be negative"));
        }
        let inbox = self.inbox_id()?;
        let id = Id::new();
        self.transact(&format!("Create task “{}”", label_title(&title)), |tx| {
            let project = match (input.parent_id, input.project_id) {
                (Some(parent), _) => load(tx, parent)?.project_id,
                (None, Some(project)) => project,
                (None, None) => inbox,
            };
            require_live_project(tx, project)?;
            if let Some(parent) = input.parent_id {
                check_parent(tx, None, parent, project)?;
            }
            let status = match input.status_id {
                Some(status) => {
                    let (status_project, _) = status_info(tx, status)?;
                    if status_project != project {
                        return Err(invalid("status belongs to a different project"));
                    }
                    status
                }
                None => default_status(tx, project)?,
            };
            let (_, category) = status_info(tx, status)?;
            let sort_key = end_key(tx, project, status)?;
            tx.insert(
                Table::Task,
                &[
                    ("id", id.into()),
                    ("project_id", project.into()),
                    ("parent_id", opt_id(input.parent_id)),
                    ("status_id", status.into()),
                    ("title", text(title.clone())),
                    (
                        "description_md",
                        text(input.description_md.clone().unwrap_or_default()),
                    ),
                    (
                        "priority",
                        int(input.priority.unwrap_or(Priority::None).as_i64()),
                    ),
                    ("start_date", opt_text(start.clone())),
                    ("due_date", opt_text(due.clone())),
                    ("due_time", opt_text(due_time.clone())),
                    ("estimate_minutes", opt_int(input.estimate_minutes)),
                    ("sort_key", text(sort_key)),
                    (
                        "completed_at",
                        if category.is_closed() {
                            int(tx.now())
                        } else {
                            Value::Null
                        },
                    ),
                    ("created_at", int(tx.now())),
                ],
            )?;
            set_tags(tx, id, &input.tags)
        })?;
        self.task(id)
    }

    pub fn update_task(&mut self, id: Id, patch: TaskPatch) -> Result<Task> {
        let current = self.task(id)?;
        self.transact(
            &format!("Edit task “{}”", label_title(&current.title)),
            |tx| {
                let mut changes: Vec<(&str, Value)> = Vec::new();
                if let Some(title) = &patch.title {
                    changes.push(("title", text(clean_name(title, "task title")?)));
                }
                if let Some(desc) = &patch.description_md {
                    changes.push(("description_md", text(desc.clone())));
                }
                if let Some(priority) = patch.priority {
                    changes.push(("priority", int(priority.as_i64())));
                }

                let start = match &patch.start_date {
                    Some(d) => d.as_deref().map(clean_date).transpose()?,
                    None => current.start_date.clone(),
                };
                let due = match &patch.due_date {
                    Some(d) => d.as_deref().map(clean_date).transpose()?,
                    None => current.due_date.clone(),
                };
                let due_time = match &patch.due_time {
                    Some(t) => t.as_deref().map(clean_time).transpose()?,
                    // Clearing the due date clears its time too.
                    None if due.is_none() => None,
                    None => current.due_time.clone(),
                };
                if due_time.is_some() && due.is_none() {
                    return Err(invalid("a due time needs a due date"));
                }
                if let (Some(s), Some(d)) = (&start, &due)
                    && s > d
                {
                    return Err(invalid("start date is after the due date"));
                }
                changes.push(("start_date", opt_text(start)));
                changes.push(("due_date", opt_text(due)));
                changes.push(("due_time", opt_text(due_time)));

                if let Some(estimate) = patch.estimate_minutes {
                    if estimate.is_some_and(|m| m < 0) {
                        return Err(invalid("estimate cannot be negative"));
                    }
                    changes.push(("estimate_minutes", opt_int(estimate)));
                }

                // Project move: map status by category; subtasks follow; detach from
                // a parent left behind in the old project.
                let project = patch.project_id.unwrap_or(current.project_id);
                if project != current.project_id {
                    require_live_project(tx, project)?;
                    let (_, category) = status_info(tx, current.status_id)?;
                    let status = status_for_category(tx, project, category)?;
                    changes.push(("project_id", project.into()));
                    changes.extend(status_changes(tx, &current, status)?);
                    changes.push(("sort_key", text(end_key(tx, project, status)?)));
                    if patch.parent_id.is_none() && current.parent_id.is_some() {
                        changes.push(("parent_id", Value::Null));
                    }
                    for child in descendants(tx, id)? {
                        let child_task = load(tx, child)?;
                        let (_, child_category) = status_info(tx, child_task.status_id)?;
                        let child_status = status_for_category(tx, project, child_category)?;
                        let mut child_changes = vec![("project_id", project.into())];
                        child_changes.extend(status_changes(tx, &child_task, child_status)?);
                        tx.update(Table::Task, &[child.into()], &child_changes)?;
                    }
                }

                if let Some(status) = patch.status_id {
                    let (status_project, _) = status_info(tx, status)?;
                    if status_project != project {
                        return Err(invalid("status belongs to a different project"));
                    }
                    changes.retain(|(c, _)| *c != "status_id" && *c != "completed_at");
                    changes.extend(status_changes(tx, &current, status)?);
                }

                if let Some(parent) = patch.parent_id {
                    if let Some(parent) = parent {
                        check_parent(tx, Some(id), parent, project)?;
                    }
                    changes.retain(|(c, _)| *c != "parent_id");
                    changes.push(("parent_id", opt_id(parent)));
                }

                tx.update(Table::Task, &[id.into()], &changes)?;
                if let Some(tags) = &patch.tags {
                    set_tags(tx, id, tags)?;
                }
                Ok(())
            },
        )?;
        self.task(id)
    }

    /// Marks a task done (first `done` status) or reopens it (default status).
    pub fn set_task_done(&mut self, id: Id, done: bool) -> Result<Task> {
        let current = self.task(id)?;
        let verb = if done { "Complete" } else { "Reopen" };
        self.transact(
            &format!("{verb} “{}”", label_title(&current.title)),
            |tx| {
                let status = if done {
                    status_for_category(tx, current.project_id, Category::Done)?
                } else {
                    default_status(tx, current.project_id)?
                };
                let changes = status_changes(tx, &current, status)?;
                tx.update(Table::Task, &[id.into()], &changes)
            },
        )?;
        self.task(id)
    }

    /// Reorders a task within its column or moves it to another status column.
    pub fn move_task(&mut self, id: Id, to: MoveTask) -> Result<Task> {
        let current = self.task(id)?;
        self.transact(
            &format!("Move “{}”", label_title(&current.title)),
            |tx| {
                let mut changes: Vec<(&str, Value)> = Vec::new();
                let status = to.status_id.unwrap_or(current.status_id);
                if status != current.status_id {
                    let (status_project, _) = status_info(tx, status)?;
                    if status_project != current.project_id {
                        return Err(invalid("status belongs to a different project"));
                    }
                    changes.extend(status_changes(tx, &current, status)?);
                }
                let neighbour_key = |n: Option<Id>| -> Result<Option<String>> {
                    n.map(|n| load(tx, n).map(|t| t.sort_key)).transpose()
                };
                let above = neighbour_key(to.above)?;
                let below = neighbour_key(to.below)?;
                let key = match (&above, &below) {
                    (None, None) => end_key(tx, current.project_id, status)?,
                    _ => key_between(above.as_deref(), below.as_deref())?,
                };
                changes.push(("sort_key", text(key)));
                tx.update(Table::Task, &[id.into()], &changes)
            },
        )?;
        self.task(id)
    }

    /// Soft-deletes a task and its subtasks as one undoable action.
    pub fn delete_task(&mut self, id: Id) -> Result<()> {
        let current = self.task(id)?;
        self.transact(
            &format!("Delete “{}”", label_title(&current.title)),
            |tx| {
                let now = tx.now();
                for child in descendants(tx, id)? {
                    let child_task = load(tx, child)?;
                    if child_task.deleted_at.is_none() {
                        tx.update(Table::Task, &[child.into()], &[("deleted_at", int(now))])?;
                    }
                }
                tx.update(Table::Task, &[id.into()], &[("deleted_at", int(now))])
            },
        )?;
        Ok(())
    }

    /// Restores a soft-deleted task and the subtasks deleted with it.
    pub fn restore_task(&mut self, id: Id) -> Result<Task> {
        self.transact("Restore task", |tx| {
            let task = load(tx, id)?;
            let Some(deleted_at) = task.deleted_at else {
                return Ok(());
            };
            require_live_project(tx, task.project_id)?;
            if let Some(parent) = task.parent_id
                && load(tx, parent)?.deleted_at.is_some()
            {
                return Err(Error::Conflict("restore its parent task first".into()));
            }
            for child in descendants(tx, id)? {
                if load(tx, child)?.deleted_at == Some(deleted_at) {
                    tx.update(Table::Task, &[child.into()], &[("deleted_at", Value::Null)])?;
                }
            }
            tx.update(Table::Task, &[id.into()], &[("deleted_at", Value::Null)])?;
            Ok(())
        })?;
        self.task(id)
    }

    /// Copies a task (not its subtasks) right after the original.
    pub fn duplicate_task(&mut self, id: Id) -> Result<Task> {
        let original = self.task(id)?;
        let copy = Id::new();
        self.transact(
            &format!("Duplicate “{}”", label_title(&original.title)),
            |tx| {
                let next: Option<String> = tx.conn().query_row(
                    "SELECT min(sort_key) FROM task WHERE project_id = ?1 AND status_id = ?2
                 AND deleted_at IS NULL AND sort_key > ?3",
                    params![original.project_id, original.status_id, original.sort_key],
                    |r| r.get(0),
                )?;
                let key = key_between(Some(&original.sort_key), next.as_deref())?;
                let (_, category) = status_info(tx, original.status_id)?;
                tx.insert(
                    Table::Task,
                    &[
                        ("id", copy.into()),
                        ("project_id", original.project_id.into()),
                        ("parent_id", opt_id(original.parent_id)),
                        ("status_id", original.status_id.into()),
                        (
                            "title",
                            text(clean_name(
                                &format!("{} (copy)", original.title),
                                "task title",
                            )?),
                        ),
                        ("description_md", text(original.description_md.clone())),
                        ("priority", int(original.priority.as_i64())),
                        ("start_date", opt_text(original.start_date.clone())),
                        ("due_date", opt_text(original.due_date.clone())),
                        ("due_time", opt_text(original.due_time.clone())),
                        ("estimate_minutes", opt_int(original.estimate_minutes)),
                        ("sort_key", text(key)),
                        (
                            "recurrence_rule",
                            opt_text(original.recurrence_rule.clone()),
                        ),
                        (
                            "completed_at",
                            if category.is_closed() {
                                int(tx.now())
                            } else {
                                Value::Null
                            },
                        ),
                        ("created_at", int(tx.now())),
                    ],
                )?;
                set_tags(tx, copy, &original.tags)
            },
        )?;
        self.task(copy)
    }
}

impl Db {
    /// Finds the project an `@hint` refers to: exact name, then `client/name`,
    /// then name prefix, then name substring (case-insensitive). Ties go to
    /// sidebar order.
    pub fn resolve_project(&self, hint: &str) -> Result<Option<Id>> {
        Ok(self.find_project(hint)?.map(|p| p.project.id))
    }

    /// Like [`Db::resolve_project`], returning the matched project's summary.
    pub fn find_project(&self, hint: &str) -> Result<Option<crate::model::ProjectSummary>> {
        let norm = |s: &str| {
            s.replace(['-', '_'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        };
        let hint = norm(hint);
        if hint.is_empty() {
            return Ok(None);
        }
        let projects = self.projects(false)?;
        let full = |p: &crate::model::ProjectSummary| {
            p.client_name
                .as_deref()
                .map(|c| norm(&format!("{c}/{}", p.project.name)))
        };
        let tiers: [&dyn Fn(&crate::model::ProjectSummary) -> bool; 4] = [
            &|p| norm(&p.project.name) == hint,
            &|p| full(p).as_deref() == Some(hint.as_str()),
            &|p| norm(&p.project.name).starts_with(&hint),
            &|p| norm(&p.project.name).contains(&hint),
        ];
        Ok(tiers
            .iter()
            .find_map(|tier| projects.iter().find(|p| tier(p)))
            .cloned())
    }

    /// Creates a task from a Quick Add line (see [`crate::quickadd`]).
    pub fn quick_add(&mut self, line: &str, today: jiff::civil::Date) -> Result<Task> {
        let parsed = crate::quickadd::parse(line, today);
        if parsed.title.trim().is_empty() {
            return Err(invalid("type a task title"));
        }
        let project_id = match &parsed.project_hint {
            Some(hint) => Some(
                self.resolve_project(hint)?
                    .ok_or_else(|| invalid(format!("no project matches “@{hint}”")))?,
            ),
            None => None,
        };
        self.create_task(NewTask {
            title: parsed.title,
            project_id,
            priority: parsed.priority,
            due_date: parsed.due_date,
            due_time: parsed.due_time,
            estimate_minutes: parsed.estimate_minutes,
            tags: parsed.tags,
            ..Default::default()
        })
    }
}
