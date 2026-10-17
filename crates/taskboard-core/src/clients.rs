//! Clients: the customers or employers that projects (jobs) belong to.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row};

use crate::error::{Error, Result};
use crate::id::Id;
use crate::model::{
    Client, ClientPatch, NewClient, boolean, clean_color, clean_name, clean_time, clean_tz, int,
    opt_text, palette_color, text,
};
use crate::order::key_between;
use crate::sections::{default_section, require_section};
use crate::storage::Db;
use crate::tx::Table;

const COLS: &str = "id, name, color, icon, notes_md, contact, time_zone, work_start, work_end,
                    meetings_count_as_time, sort_key, archived_at, created_at, updated_at, section_id";

fn map(r: &Row<'_>) -> rusqlite::Result<Client> {
    Ok(Client {
        id: r.get(0)?,
        name: r.get(1)?,
        color: r.get(2)?,
        icon: r.get(3)?,
        notes_md: r.get(4)?,
        contact: r.get(5)?,
        time_zone: r.get(6)?,
        work_start: r.get(7)?,
        work_end: r.get(8)?,
        meetings_count_as_time: r.get(9)?,
        sort_key: r.get(10)?,
        archived_at: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        section_id: r.get(14)?,
    })
}

impl Db {
    pub fn create_client(&mut self, input: NewClient) -> Result<Client> {
        let name = clean_name(&input.name, "client name")?;
        let time_zone = input.time_zone.as_deref().map(clean_tz).transpose()?;
        let id = Id::new();
        self.transact(&format!("Create client “{name}”"), |tx| {
            let section_id = match input.section_id {
                Some(section) => {
                    require_section(tx, section)?;
                    section
                }
                None => default_section(tx)?,
            };
            let (count, last): (i64, Option<String>) = tx.conn().query_row(
                "SELECT count(*), max(sort_key) FROM client WHERE deleted_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let color = match &input.color {
                Some(c) => clean_color(c)?,
                None => palette_color(count),
            };
            tx.insert(
                Table::Client,
                &[
                    ("id", id.into()),
                    ("name", text(name.clone())),
                    ("color", text(color)),
                    ("notes_md", text(input.notes_md.clone().unwrap_or_default())),
                    ("contact", text(input.contact.clone().unwrap_or_default())),
                    ("time_zone", opt_text(time_zone.clone())),
                    ("section_id", section_id.into()),
                    ("sort_key", text(key_between(last.as_deref(), None)?)),
                    ("created_at", int(tx.now())),
                ],
            )
        })?;
        self.client(id)
    }

    pub fn client(&self, id: Id) -> Result<Client> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM client WHERE id = ?1 AND deleted_at IS NULL"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("client"))
    }

    pub fn clients(&self, include_archived: bool) -> Result<Vec<Client>> {
        let sql = format!(
            "SELECT {COLS} FROM client WHERE deleted_at IS NULL {} ORDER BY sort_key",
            if include_archived {
                ""
            } else {
                "AND archived_at IS NULL"
            }
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt.query_map([], map)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn update_client(&mut self, id: Id, patch: ClientPatch) -> Result<Client> {
        let current = self.client(id)?;
        let mut changes: Vec<(&str, Value)> = Vec::new();
        if let Some(name) = &patch.name {
            changes.push(("name", text(clean_name(name, "client name")?)));
        }
        if let Some(color) = &patch.color {
            changes.push(("color", text(clean_color(color)?)));
        }
        if let Some(contact) = patch.contact {
            changes.push(("contact", text(contact)));
        }
        if let Some(notes) = patch.notes_md {
            changes.push(("notes_md", text(notes)));
        }
        if let Some(tz) = patch.time_zone {
            changes.push((
                "time_zone",
                opt_text(tz.as_deref().map(clean_tz).transpose()?),
            ));
        }
        if let Some(t) = patch.work_start {
            changes.push((
                "work_start",
                opt_text(t.as_deref().map(clean_time).transpose()?),
            ));
        }
        if let Some(t) = patch.work_end {
            changes.push((
                "work_end",
                opt_text(t.as_deref().map(clean_time).transpose()?),
            ));
        }
        if let Some(b) = patch.meetings_count_as_time {
            changes.push(("meetings_count_as_time", boolean(b)));
        }
        self.transact(&format!("Edit client “{}”", current.name), |tx| {
            // Moving a group to another section takes its projects along;
            // it goes last among the target section's groups.
            if let Some(section) = patch.section_id
                && Some(section) != current.section_id
            {
                require_section(tx, section)?;
                let last: Option<String> = tx.conn().query_row(
                    "SELECT max(sort_key) FROM client
                     WHERE deleted_at IS NULL AND section_id = ?1 AND id <> ?2",
                    rusqlite::params![section, id],
                    |r| r.get(0),
                )?;
                changes.push(("section_id", section.into()));
                changes.push(("sort_key", text(key_between(last.as_deref(), None)?)));
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
            tx.update(Table::Client, &[id.into()], &changes)
        })?;
        self.client(id)
    }

    /// Archiving a client archives its active projects in the same undoable
    /// action; unarchiving restores exactly the projects archived with it.
    pub fn set_client_archived(&mut self, id: Id, archived: bool) -> Result<Client> {
        let client = self.client(id)?;
        let label = if archived { "Archive" } else { "Unarchive" };
        self.transact(&format!("{label} client “{}”", client.name), |tx| {
            let now = tx.now();
            let projects: Vec<Id> = if archived {
                let mut stmt = tx.conn().prepare(
                    "SELECT id FROM project WHERE client_id = ?1 AND deleted_at IS NULL
                     AND archived_at IS NULL",
                )?;
                stmt.query_map([id], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            } else {
                let mut stmt = tx.conn().prepare(
                    "SELECT id FROM project WHERE client_id = ?1 AND deleted_at IS NULL
                     AND archived_at = ?2",
                )?;
                stmt.query_map(rusqlite::params![id, client.archived_at], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            let stamp = if archived { int(now) } else { Value::Null };
            for project in projects {
                tx.update(
                    Table::Project,
                    &[project.into()],
                    &[("archived_at", stamp.clone())],
                )?;
            }
            tx.update(Table::Client, &[id.into()], &[("archived_at", stamp)])
        })?;
        self.client(id)
    }

    /// Soft-deletes a client. Refuses while it still has projects: move or
    /// delete those first, so no work disappears by accident.
    pub fn delete_client(&mut self, id: Id) -> Result<()> {
        let client = self.client(id)?;
        let projects: i64 = self.conn().query_row(
            "SELECT count(*) FROM project WHERE client_id = ?1 AND deleted_at IS NULL",
            [id],
            |r| r.get(0),
        )?;
        if projects > 0 {
            return Err(Error::Conflict(format!(
                "“{}” still has {projects} project(s); move or delete them first",
                client.name
            )));
        }
        self.transact(&format!("Delete client “{}”", client.name), |tx| {
            let now = tx.now();
            tx.update(Table::Client, &[id.into()], &[("deleted_at", int(now))])
        })?;
        Ok(())
    }
}
