//! Moving clients and projects up or down in the sidebar.
//!
//! A move swaps an item with its neighbour among the rows the sidebar shows
//! (live, not archived): all clients, or the projects of one client (or of
//! Personal). It is one undoable action.

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::error::Result;
use crate::id::Id;
use crate::model::text;
use crate::order::key_between;
use crate::storage::Db;
use crate::tx::{Table, Tx};

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
