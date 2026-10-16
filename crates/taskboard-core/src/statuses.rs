//! Per-project workflows: the statuses that become Kanban columns.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use ts_rs::TS;

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{Category, Status, clean_color, clean_name, int, opt_int, text};
use crate::order::key_between;
use crate::projects::{STATUS_COLS, map_status};
use crate::storage::Db;
use crate::tx::{Table, Tx};

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewStatus {
    pub project_id: Id,
    pub name: String,
    pub category: Category,
    #[serde(default)]
    #[ts(optional)]
    pub color: Option<String>,
    /// Insert after this status; default: at the end.
    #[serde(default)]
    #[ts(optional)]
    pub after: Option<Id>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct StatusPatch {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub color: Option<String>,
    #[ts(optional)]
    pub category: Option<Category>,
    #[serde(default, deserialize_with = "crate::model::nullable")]
    #[ts(optional, type = "number | null")]
    pub wip_limit: Option<Option<i64>>,
}

fn load(tx: &Tx<'_>, id: Id) -> Result<Status> {
    tx.conn()
        .query_row(
            &format!("SELECT {STATUS_COLS} FROM status WHERE id = ?1 AND deleted_at IS NULL"),
            [id],
            map_status,
        )
        .optional()?
        .ok_or(Error::NotFound("status"))
}

fn siblings(tx: &Tx<'_>, project: Id) -> Result<Vec<Status>> {
    let mut stmt = tx.conn().prepare(&format!(
        "SELECT {STATUS_COLS} FROM status WHERE project_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_key"
    ))?;
    let rows = stmt
        .query_map([project], map_status)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn check_unique_name(tx: &Tx<'_>, project: Id, name: &str, except: Option<Id>) -> Result<()> {
    let clash = siblings(tx, project)?
        .iter()
        .any(|s| Some(s.id) != except && s.name.eq_ignore_ascii_case(name));
    if clash {
        return Err(invalid(format!(
            "this project already has a “{name}” column"
        )));
    }
    Ok(())
}

/// Keeps `completed_at` consistent after a status changes between open and closed.
fn sync_completion(tx: &mut Tx<'_>, status: Id, closed: bool) -> Result<()> {
    let tasks: Vec<(Id, Option<i64>)> = {
        let mut stmt = tx.conn().prepare(
            "SELECT id, completed_at FROM task WHERE status_id = ?1 AND deleted_at IS NULL",
        )?;
        stmt.query_map([status], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let now = tx.now();
    for (task, completed_at) in tasks {
        let value = match (closed, completed_at) {
            (true, None) => int(now),
            (false, Some(_)) => Value::Null,
            _ => continue,
        };
        tx.update(Table::Task, &[task.into()], &[("completed_at", value)])?;
    }
    Ok(())
}

impl Db {
    /// Every live status of live projects (small; used to build boards).
    pub fn all_statuses(&self) -> Result<Vec<Status>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {STATUS_COLS} FROM status WHERE deleted_at IS NULL
               AND project_id IN (SELECT id FROM project WHERE deleted_at IS NULL)
             ORDER BY sort_key"
        ))?;
        let rows = stmt
            .query_map([], map_status)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn status(&self, id: Id) -> Result<Status> {
        self.conn()
            .query_row(
                &format!("SELECT {STATUS_COLS} FROM status WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map_status,
            )
            .optional()?
            .ok_or(Error::NotFound("status"))
    }

    pub fn create_status(&mut self, input: NewStatus) -> Result<Status> {
        let name = clean_name(&input.name, "column name")?;
        let color = input.color.as_deref().map(clean_color).transpose()?;
        let id = Id::new();
        self.transact(&format!("Add column “{name}”"), |tx| {
            check_unique_name(tx, input.project_id, &name, None)?;
            let all = siblings(tx, input.project_id)?;
            if all.is_empty() {
                return Err(Error::NotFound("project"));
            }
            let (before, after) = match input.after {
                Some(after) => {
                    let at = all
                        .iter()
                        .position(|s| s.id == after)
                        .ok_or(Error::NotFound("status"))?;
                    (
                        all.get(at).map(|s| s.sort_key.clone()),
                        all.get(at + 1).map(|s| s.sort_key.clone()),
                    )
                }
                None => (all.last().map(|s| s.sort_key.clone()), None),
            };
            let default_color = match input.category {
                Category::Todo => "230",
                Category::Active => "262",
                Category::Done => "150",
                Category::Cancelled => "gray",
            };
            tx.insert(
                Table::Status,
                &[
                    ("id", id.into()),
                    ("project_id", input.project_id.into()),
                    ("name", text(name.clone())),
                    (
                        "color",
                        text(color.clone().unwrap_or_else(|| default_color.into())),
                    ),
                    ("category", text(input.category.as_str())),
                    (
                        "sort_key",
                        text(key_between(before.as_deref(), after.as_deref())?),
                    ),
                    ("created_at", int(tx.now())),
                ],
            )
        })?;
        self.status(id)
    }

    pub fn update_status(&mut self, id: Id, patch: StatusPatch) -> Result<Status> {
        let current = self.status(id)?;
        self.transact(&format!("Edit column “{}”", current.name), |tx| {
            let mut changes: Vec<(&str, Value)> = Vec::new();
            if let Some(name) = &patch.name {
                let name = clean_name(name, "column name")?;
                check_unique_name(tx, current.project_id, &name, Some(id))?;
                changes.push(("name", text(name)));
            }
            if let Some(color) = &patch.color {
                changes.push(("color", text(clean_color(color)?)));
            }
            if let Some(limit) = patch.wip_limit {
                if limit.is_some_and(|l| l < 1) {
                    return Err(invalid("WIP limit must be at least 1"));
                }
                changes.push(("wip_limit", opt_int(limit)));
            }
            if let Some(category) = patch.category
                && category != current.category
            {
                if current.category == Category::Done && category != Category::Done {
                    let other_done = siblings(tx, current.project_id)?
                        .iter()
                        .any(|s| s.id != id && s.category == Category::Done);
                    if !other_done {
                        return Err(invalid("a project needs at least one Done column"));
                    }
                }
                changes.push(("category", text(category.as_str())));
                if category.is_closed() != current.category.is_closed() {
                    sync_completion(tx, id, category.is_closed())?;
                }
            }
            tx.update(Table::Status, &[id.into()], &changes)?;
            Ok(())
        })?;
        self.status(id)
    }

    /// Moves a column to just after `after` (`None` = first).
    pub fn move_status(&mut self, id: Id, after: Option<Id>) -> Result<Status> {
        let current = self.status(id)?;
        self.transact(&format!("Move column “{}”", current.name), |tx| {
            let all: Vec<Status> = siblings(tx, current.project_id)?
                .into_iter()
                .filter(|s| s.id != id)
                .collect();
            let at = match after {
                Some(after) => {
                    all.iter()
                        .position(|s| s.id == after)
                        .ok_or(Error::NotFound("status"))?
                        + 1
                }
                None => 0,
            };
            let before = at
                .checked_sub(1)
                .and_then(|i| all.get(i))
                .map(|s| s.sort_key.as_str());
            let next = all.get(at).map(|s| s.sort_key.as_str());
            let key = key_between(before, next)?;
            tx.update(Table::Status, &[id.into()], &[("sort_key", text(key))])?;
            Ok(())
        })?;
        self.status(id)
    }

    /// Deletes a column, moving its tasks to `move_to` in the same project.
    /// A project always keeps at least one open and one Done column.
    pub fn delete_status(&mut self, id: Id, move_to: Id) -> Result<()> {
        let current = self.status(id)?;
        if move_to == id {
            return Err(invalid("choose another column for its tasks"));
        }
        self.transact(&format!("Delete column “{}”", current.name), |tx| {
            let target = load(tx, move_to)?;
            if target.project_id != current.project_id {
                return Err(invalid(
                    "tasks can only move to a column in the same project",
                ));
            }
            let rest: Vec<Status> = siblings(tx, current.project_id)?
                .into_iter()
                .filter(|s| s.id != id)
                .collect();
            if !rest.iter().any(|s| !s.category.is_closed()) {
                return Err(invalid("a project needs at least one open column"));
            }
            if !rest.iter().any(|s| s.category == Category::Done) {
                return Err(invalid("a project needs at least one Done column"));
            }
            let tasks: Vec<(Id, Option<i64>)> = {
                let mut stmt = tx.conn().prepare(
                    "SELECT id, completed_at FROM task WHERE status_id = ?1 ORDER BY sort_key",
                )?;
                stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            };
            let closed = target.category.is_closed();
            let now = tx.now();
            let mut last: Option<String> = tx.conn().query_row(
                "SELECT max(sort_key) FROM task WHERE status_id = ?1 AND deleted_at IS NULL",
                params![move_to],
                |r| r.get(0),
            )?;
            for (task, completed_at) in tasks {
                let key = key_between(last.as_deref(), None)?;
                let completed = match (closed, completed_at) {
                    (true, Some(at)) => int(at),
                    (true, None) => int(now),
                    (false, _) => Value::Null,
                };
                tx.update(
                    Table::Task,
                    &[task.into()],
                    &[
                        ("status_id", move_to.into()),
                        ("sort_key", text(key.clone())),
                        ("completed_at", completed),
                    ],
                )?;
                last = Some(key);
            }
            tx.update(Table::Status, &[id.into()], &[("deleted_at", int(now))])?;
            Ok(())
        })
    }
}
