//! Sections: user-named areas of the sidebar ("Clients", "Teams", "Family"…).
//! A section holds groups (stored in the `client` table) and projects; the
//! Inbox belongs to no section.

use rusqlite::{OptionalExtension, Row, Transaction};
use serde::Serialize;
use ts_rs::TS;

use crate::clock::{Hlc, now_ms};
use crate::error::{Error, Result};
use crate::id::Id;
use crate::model::palette_color;
use crate::order::key_between;
use crate::storage::Db;

const COLS: &str = "id, name, item_name, color, sort_key, created_at, updated_at";

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Section {
    pub id: Id,
    pub name: String,
    /// What one group in this section is called ("client", "team"); empty
    /// means the UI says "group".
    pub item_name: String,
    pub color: String,
    pub sort_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

fn map(r: &Row<'_>) -> rusqlite::Result<Section> {
    Ok(Section {
        id: r.get(0)?,
        name: r.get(1)?,
        item_name: r.get(2)?,
        color: r.get(3)?,
        sort_key: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

impl Db {
    /// Live sections in sidebar order.
    pub fn sections(&self) -> Result<Vec<Section>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {COLS} FROM section WHERE deleted_at IS NULL ORDER BY sort_key, id"
        ))?;
        let rows = stmt.query_map([], map)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn section(&self, id: Id) -> Result<Section> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM section WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("section"))
    }
}

/// Migration 7 hook: databases that already have clients or projects get a
/// "Clients" section (every client and its projects) and a "Personal"
/// section (projects without a client), so the sidebar looks as before.
/// Archived and soft-deleted rows are assigned too, so undo / unarchive
/// bring them back in place. Not an undo step (no op-log entries).
pub(crate) fn assign_existing(tx: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let has_data: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM client) OR EXISTS (SELECT 1 FROM project WHERE is_inbox = 0)",
        [],
        |r| r.get(0),
    )?;
    if !has_data {
        return Ok(());
    }
    let hook_err = |e: Error| rusqlite_migration::HookError::Hook(e.to_string());
    let device = crate::storage::load_device_id(tx).map_err(hook_err)?;
    let now = now_ms();
    let mut hlc = Hlc::default();
    let first = key_between(None, None).map_err(hook_err)?;
    let second = key_between(Some(&first), None).map_err(hook_err)?;
    let (clients, personal) = (Id::new(), Id::new());
    for (id, name, item, color, key) in [
        (clients, "Clients", "client", palette_color(2), &first),
        (personal, "Personal", "", palette_color(9), &second),
    ] {
        tx.execute(
            "INSERT INTO section (id, name, item_name, color, sort_key, created_at, updated_at, hlc, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8)",
            rusqlite::params![id, name, item, color, key, now, hlc.tick(now), device],
        )?;
    }
    tx.execute("UPDATE client SET section_id = ?1", [clients])?;
    tx.execute(
        "UPDATE project SET section_id = CASE WHEN client_id IS NULL THEN ?2 ELSE ?1 END
         WHERE is_inbox = 0",
        [clients, personal],
    )?;
    Ok(())
}
