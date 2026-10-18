//! Moving sections, groups (clients) and projects in the sidebar.
//!
//! `move_*` swaps an item with its neighbour among the rows the sidebar shows
//! (live, not archived). `place_*` is drag and drop: put an item before
//! another one (or at the end), possibly in another section or group. Each
//! is one undoable action.

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::error::{Result, invalid};
use crate::id::Id;
use crate::model::text;
use crate::order::key_between;
use crate::sections::require_section;
use crate::storage::Db;
use crate::tx::{Table, Tx};

/// Where a dragged project lands: inside a group, or directly in a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectPlace {
    Group { id: Id },
    Section { id: Id },
}

const SECTION_ROWS: &str =
    "SELECT id, sort_key FROM section WHERE deleted_at IS NULL ORDER BY sort_key, id";
const CLIENT_ROWS: &str = "SELECT id, sort_key FROM client
     WHERE deleted_at IS NULL AND archived_at IS NULL AND section_id = ?1
     ORDER BY sort_key, id";
const PROJECT_ROWS: &str = "SELECT id, sort_key FROM project
     WHERE deleted_at IS NULL AND archived_at IS NULL AND is_inbox = 0
       AND client_id IS ?1 AND section_id IS ?2
     ORDER BY sort_key, id";

/// Which way to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
}

impl Db {
    /// Moves a client one place up or down. Returns false when it is already
    /// first / last (nothing changes).
    pub fn move_client(&mut self, id: Id, direction: Direction) -> Result<bool> {
        let client = self.client(id)?;
        let label = format!("Move client “{}” {}", client.name, word(direction));
        self.transact(&label, |tx| {
            let siblings = siblings(
                tx,
                "SELECT id, sort_key FROM client
                 WHERE deleted_at IS NULL AND archived_at IS NULL
                 ORDER BY sort_key, id",
                &[],
            )?;
            step(tx, Table::Client, siblings, id, direction)
        })
    }

    /// Moves a section one place up or down in the sidebar.
    pub fn move_section(&mut self, id: Id, direction: Direction) -> Result<bool> {
        let section = self.section(id)?;
        let label = format!("Move section “{}” {}", section.name, word(direction));
        self.transact(&label, |tx| {
            let siblings = siblings(
                tx,
                "SELECT id, sort_key FROM section WHERE deleted_at IS NULL ORDER BY sort_key, id",
                &[],
            )?;
            step(tx, Table::Section, siblings, id, direction)
        })
    }

    /// Moves a project one place up or down among its client's projects (or
    /// the Personal ones). The Inbox has a fixed place and cannot move.
    pub fn move_project(&mut self, id: Id, direction: Direction) -> Result<bool> {
        let project = self.project(id)?;
        if project.is_inbox {
            return Ok(false);
        }
        let label = format!("Move project “{}” {}", project.name, word(direction));
        self.transact(&label, |tx| {
            let siblings = siblings(
                tx,
                "SELECT id, sort_key FROM project
                 WHERE deleted_at IS NULL AND archived_at IS NULL AND is_inbox = 0
                   AND client_id IS ?1 AND section_id IS ?2
                 ORDER BY sort_key, id",
                &[
                    project.client_id.map_or(Value::Null, Value::from),
                    project.section_id.map_or(Value::Null, Value::from),
                ],
            )?;
            step(tx, Table::Project, siblings, id, direction)
        })
    }
}

impl Db {
    /// Drag and drop: puts a section before `before` (None = last).
    /// Returns false when it is already there (nothing changes).
    pub fn place_section(&mut self, id: Id, before: Option<Id>) -> Result<bool> {
        let section = self.section(id)?;
        if self.already_placed(SECTION_ROWS, &[], id, before)? {
            return Ok(false);
        }
        self.transact(&format!("Move section “{}”", section.name), |tx| {
            let rows = siblings(tx, SECTION_ROWS, &[])?;
            place(tx, Table::Section, rows, id, before, &[])
        })?;
        Ok(true)
    }

    /// Drag and drop: puts a group in `section` before `before` (None =
    /// last). A group that changes section takes its projects along.
    pub fn place_client(&mut self, id: Id, section: Id, before: Option<Id>) -> Result<bool> {
        let client = self.client(id)?;
        self.section(section)?;
        let args = [Value::from(section)];
        if client.section_id == Some(section)
            && self.already_placed(CLIENT_ROWS, &args, id, before)?
        {
            return Ok(false);
        }
        self.transact(&format!("Move “{}”", client.name), |tx| {
            let rows = siblings(tx, CLIENT_ROWS, &args)?;
            place(
                tx,
                Table::Client,
                rows,
                id,
                before,
                &[("section_id", section.into())],
            )?;
            if client.section_id != Some(section) {
                let projects: Vec<Id> = {
                    let mut stmt = tx.conn().prepare(
                        "SELECT id FROM project WHERE client_id = ?1 AND deleted_at IS NULL",
                    )?;
                    stmt.query_map([id], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for p in projects {
                    tx.update(
                        Table::Project,
                        &[p.into()],
                        &[("section_id", section.into())],
                    )?;
                }
            }
            Ok(())
        })?;
        Ok(true)
    }

    /// Drag and drop: puts a project into a group or directly into a section,
    /// before `before` (None = last). A project in a group lives in the
    /// group's section. The Inbox has a fixed place and cannot move.
    pub fn place_project(&mut self, id: Id, to: ProjectPlace, before: Option<Id>) -> Result<bool> {
        let project = self.project(id)?;
        if project.is_inbox {
            return Ok(false);
        }
        let (client, section) = match to {
            ProjectPlace::Group { id: group } => (Some(group), self.client(group)?.section_id),
            ProjectPlace::Section { id: section } => {
                self.section(section)?;
                (None, Some(section))
            }
        };
        let args = [
            client.map_or(Value::Null, Value::from),
            section.map_or(Value::Null, Value::from),
        ];
        let same = project.client_id == client && project.section_id == section;
        if same && self.already_placed(PROJECT_ROWS, &args, id, before)? {
            return Ok(false);
        }
        self.transact(&format!("Move project “{}”", project.name), |tx| {
            if let Some(s) = section {
                require_section(tx, s)?;
            }
            let rows = siblings(tx, PROJECT_ROWS, &args)?;
            place(
                tx,
                Table::Project,
                rows,
                id,
                before,
                &[
                    ("client_id", client.map_or(Value::Null, Value::from)),
                    ("section_id", section.map_or(Value::Null, Value::from)),
                ],
            )
        })?;
        Ok(true)
    }

    /// True when `id` already sits right before `before` (or last, for None)
    /// among the rows `sql` selects.
    fn already_placed(
        &self,
        sql: &str,
        args: &[Value],
        id: Id,
        before: Option<Id>,
    ) -> Result<bool> {
        if before == Some(id) {
            return Ok(true);
        }
        let mut stmt = self.conn().prepare(sql)?;
        let rows: Vec<Id> = stmt
            .query_map(params_from_iter(args.iter()), |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let Some(i) = rows.iter().position(|r| *r == id) else {
            return Ok(false);
        };
        Ok(rows.get(i + 1).copied() == before)
    }
}

/// Gives `id` a key that puts it before `before` (or last) among `rows` (its
/// new siblings), along with any other `changes` (its new container). Rows
/// with repeated keys are renumbered first, in the same action.
fn place(
    tx: &mut Tx<'_>,
    table: Table,
    rows: Vec<(Id, String)>,
    id: Id,
    before: Option<Id>,
    changes: &[(&str, Value)],
) -> Result<()> {
    let mut rows: Vec<(Id, String)> = rows.into_iter().filter(|(row, _)| *row != id).collect();
    if rows.windows(2).any(|w| w[0].1 == w[1].1) {
        let mut prev: Option<String> = None;
        for (row, key) in &mut rows {
            let fresh = key_between(prev.as_deref(), None)?;
            tx.update(
                table,
                &[(*row).into()],
                &[("sort_key", text(fresh.clone()))],
            )?;
            *key = fresh.clone();
            prev = Some(fresh);
        }
    }
    let at = match before {
        Some(b) => rows
            .iter()
            .position(|(row, _)| *row == b)
            .ok_or_else(|| invalid("the item to drop before is not in that place"))?,
        None => rows.len(),
    };
    let lower = at
        .checked_sub(1)
        .and_then(|j| rows.get(j))
        .map(|(_, k)| k.as_str());
    let upper = rows.get(at).map(|(_, k)| k.as_str());
    let mut all: Vec<(&str, Value)> = changes.to_vec();
    all.push(("sort_key", text(key_between(lower, upper)?)));
    tx.update(table, &[id.into()], &all)?;
    Ok(())
}

fn word(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

fn siblings(tx: &Tx<'_>, sql: &str, args: &[Value]) -> Result<Vec<(Id, String)>> {
    let mut stmt = tx.conn().prepare(sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Gives `id` a key between its new neighbours. Groups with repeated keys
/// (e.g. after projects moved between clients) are renumbered first, in the
/// same action, so a key always fits.
fn step(
    tx: &mut Tx<'_>,
    table: Table,
    mut rows: Vec<(Id, String)>,
    id: Id,
    direction: Direction,
) -> Result<bool> {
    let Some(i) = rows.iter().position(|(row, _)| *row == id) else {
        return Ok(false);
    };
    let at_end = match direction {
        Direction::Up => i == 0,
        Direction::Down => i + 1 == rows.len(),
    };
    if at_end {
        return Ok(false);
    }
    if rows.windows(2).any(|w| w[0].1 == w[1].1) {
        let mut prev: Option<String> = None;
        for (row, key) in &mut rows {
            let fresh = key_between(prev.as_deref(), None)?;
            tx.update(
                table,
                &[(*row).into()],
                &[("sort_key", text(fresh.clone()))],
            )?;
            *key = fresh.clone();
            prev = Some(fresh);
        }
    }
    let key = |j: usize| rows.get(j).map(|(_, k)| k.as_str());
    let new_key = match direction {
        Direction::Up => key_between(i.checked_sub(2).and_then(key), key(i - 1))?,
        Direction::Down => key_between(key(i + 1), key(i + 2))?,
    };
    tx.update(table, &[id.into()], &[("sort_key", text(new_key))])?;
    Ok(true)
}
