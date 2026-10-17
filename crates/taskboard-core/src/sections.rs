//! Sections: user-named areas of the sidebar ("Clients", "Teams", "Family"…).
//! A section holds groups (stored in the `client` table) and projects; the
//! Inbox belongs to no section.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::clock::{Hlc, now_ms};
use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{clean_color, clean_name, int, palette_color, text};
use crate::order::key_between;
use crate::projects::delete_project_rows;
use crate::storage::Db;
use crate::tx::{Table, Tx};

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

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct NewSection {
    pub name: String,
    /// Singular name of one group ("team"); empty or missing means "group".
    #[serde(default)]
    #[ts(optional)]
    pub item_name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct SectionPatch {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub item_name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub color: Option<String>,
}

/// What happens to a section's groups and projects when it is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SectionDelete {
    /// Move everything to another section.
    MoveTo { section_id: Id },
    /// Delete the groups, projects and tasks too (undoable).
    Everything,
}

fn clean_item_name(item: Option<&str>) -> String {
    item.map(|i| i.trim().to_owned()).unwrap_or_default()
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

    pub fn create_section(&mut self, input: NewSection) -> Result<Section> {
        let name = clean_name(&input.name, "section name")?;
        let item = clean_item_name(input.item_name.as_deref());
        let id = Id::new();
        self.transact(&format!("Create section “{name}”"), |tx| {
            let (count, last): (i64, Option<String>) = tx.conn().query_row(
                "SELECT count(*), max(sort_key) FROM section WHERE deleted_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let color = match &input.color {
                Some(c) => clean_color(c)?,
                None => palette_color(count + 2),
            };
            tx.insert(
                Table::Section,
                &[
                    ("id", id.into()),
                    ("name", text(name.clone())),
                    ("item_name", text(item.clone())),
                    ("color", text(color)),
                    ("sort_key", text(key_between(last.as_deref(), None)?)),
                    ("created_at", int(tx.now())),
                ],
            )
        })?;
        self.section(id)
    }

    pub fn update_section(&mut self, id: Id, patch: SectionPatch) -> Result<Section> {
        let current = self.section(id)?;
        let mut changes: Vec<(&str, Value)> = Vec::new();
        if let Some(name) = &patch.name {
            changes.push(("name", text(clean_name(name, "section name")?)));
        }
        if let Some(item) = &patch.item_name {
            changes.push(("item_name", text(clean_item_name(Some(item)))));
        }
        if let Some(color) = &patch.color {
            changes.push(("color", text(clean_color(color)?)));
        }
        self.transact(&format!("Edit section “{}”", current.name), |tx| {
            tx.update(Table::Section, &[id.into()], &changes)
        })?;
        self.section(id)
    }

    /// Deletes a section, moving its groups and projects to another section
    /// or deleting them with it. One undo step either way. The last section
    /// can't be deleted: new projects need somewhere to go.
    pub fn delete_section(&mut self, id: Id, how: SectionDelete) -> Result<()> {
        let section = self.section(id)?;
        if self.sections()?.len() <= 1 {
            return Err(invalid("Keep at least one section"));
        }
        if let SectionDelete::MoveTo { section_id } = how {
            if section_id == id {
                return Err(invalid("choose a different section to move things to"));
            }
            self.section(section_id)?;
        }
        self.transact(&format!("Delete section “{}”", section.name), |tx| {
            let clients = ids(
                tx,
                "SELECT id FROM client WHERE section_id = ?1 AND deleted_at IS NULL ORDER BY sort_key, id",
                id,
            )?;
            let direct = ids(
                tx,
                "SELECT id FROM project WHERE section_id = ?1 AND client_id IS NULL
                   AND deleted_at IS NULL ORDER BY sort_key, id",
                id,
            )?;
            let grouped = ids(
                tx,
                "SELECT id FROM project WHERE section_id = ?1 AND client_id IS NOT NULL
                   AND deleted_at IS NULL",
                id,
            )?;
            match how {
                SectionDelete::MoveTo { section_id: target } => {
                    // Appended after the target's own groups / direct projects.
                    let mut last: Option<String> = tx.conn().query_row(
                        "SELECT max(sort_key) FROM client WHERE section_id = ?1 AND deleted_at IS NULL",
                        [target],
                        |r| r.get(0),
                    )?;
                    for c in clients {
                        let key = key_between(last.as_deref(), None)?;
                        tx.update(
                            Table::Client,
                            &[c.into()],
                            &[("section_id", target.into()), ("sort_key", text(key.clone()))],
                        )?;
                        last = Some(key);
                    }
                    let mut last: Option<String> = tx.conn().query_row(
                        "SELECT max(sort_key) FROM project
                         WHERE section_id = ?1 AND client_id IS NULL AND deleted_at IS NULL",
                        [target],
                        |r| r.get(0),
                    )?;
                    for p in direct {
                        let key = key_between(last.as_deref(), None)?;
                        tx.update(
                            Table::Project,
                            &[p.into()],
                            &[("section_id", target.into()), ("sort_key", text(key.clone()))],
                        )?;
                        last = Some(key);
                    }
                    for p in grouped {
                        tx.update(Table::Project, &[p.into()], &[("section_id", target.into())])?;
                    }
                }
                SectionDelete::Everything => {
                    for p in direct.into_iter().chain(grouped) {
                        delete_project_rows(tx, p)?;
                    }
                    let now = tx.now();
                    for c in clients {
                        tx.update(Table::Client, &[c.into()], &[("deleted_at", int(now))])?;
                    }
                }
            }
            let now = tx.now();
            tx.update(Table::Section, &[id.into()], &[("deleted_at", int(now))])?;
            Ok(())
        })
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

/// The section new groups and projects go to when none is given: the
/// first one, or a new "My work" section on a database that has none.
pub(crate) fn default_section(tx: &mut Tx<'_>) -> Result<Id> {
    let first: Option<Id> = tx
        .conn()
        .query_row(
            "SELECT id FROM section WHERE deleted_at IS NULL ORDER BY sort_key, id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = first {
        return Ok(id);
    }
    let id = Id::new();
    tx.insert(
        Table::Section,
        &[
            ("id", id.into()),
            ("name", text("My work")),
            ("item_name", text("")),
            ("color", text(palette_color(2))),
            ("sort_key", text(key_between(None, None)?)),
            ("created_at", int(tx.now())),
        ],
    )?;
    Ok(id)
}

/// Fails unless `id` is a live section.
pub(crate) fn require_section(tx: &Tx<'_>, id: Id) -> Result<()> {
    let live: bool = tx.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM section WHERE id = ?1 AND deleted_at IS NULL)",
        [id],
        |r| r.get(0),
    )?;
    if live {
        Ok(())
    } else {
        Err(Error::NotFound("section"))
    }
}

fn ids(tx: &Tx<'_>, sql: &str, section: Id) -> Result<Vec<Id>> {
    let mut stmt = tx.conn().prepare(sql)?;
    let rows = stmt
        .query_map([section], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
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
