//! Saved views: named filters the sidebar and command palette can reopen.
//!
//! The spec is a JSON object owned by the UI (scope, search, sort, view…);
//! core only checks that it is a reasonably small object, so the UI can add
//! fields without a migration.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{clean_name, int, text};
use crate::order::key_between;
use crate::storage::Db;
use crate::tx::Table;

/// Upper bound for a view spec; real specs are a few hundred bytes.
const MAX_SPEC_BYTES: usize = 16 * 1024;

const COLS: &str = "id, name, spec_json, sort_key, created_at, updated_at";

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SavedView {
    pub id: Id,
    pub name: String,
    /// JSON object; its shape is defined by the UI.
    pub spec_json: String,
    pub sort_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct SavedViewPatch {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub spec_json: Option<String>,
}

fn map(r: &Row<'_>) -> rusqlite::Result<SavedView> {
    Ok(SavedView {
        id: r.get(0)?,
        name: r.get(1)?,
        spec_json: r.get(2)?,
        sort_key: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn clean_spec(spec: &str) -> Result<String> {
    if spec.len() > MAX_SPEC_BYTES {
        return Err(invalid("view settings are too large"));
    }
    let value: serde_json::Value =
        serde_json::from_str(spec).map_err(|_| invalid("view settings are not valid JSON"))?;
    if !value.is_object() {
        return Err(invalid("view settings must be a JSON object"));
    }
    // Store the compact form so equal specs compare equal.
    Ok(value.to_string())
}

impl Db {
    pub fn create_saved_view(&mut self, name: &str, spec_json: &str) -> Result<SavedView> {
        let name = clean_name(name, "view name")?;
        let spec = clean_spec(spec_json)?;
        let id = Id::new();
        self.transact(&format!("Save view “{name}”"), |tx| {
            let last: Option<String> =
                tx.conn()
                    .query_row("SELECT max(sort_key) FROM saved_view", [], |r| r.get(0))?;
            tx.insert(
                Table::SavedView,
                &[
                    ("id", id.into()),
                    ("name", text(name.clone())),
                    ("spec_json", text(spec.clone())),
                    ("sort_key", text(key_between(last.as_deref(), None)?)),
                    ("created_at", int(tx.now())),
                ],
            )
        })?;
        self.saved_view(id)
    }

    pub fn saved_view(&self, id: Id) -> Result<SavedView> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM saved_view WHERE id = ?1"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("saved view"))
    }

    pub fn saved_views(&self) -> Result<Vec<SavedView>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {COLS} FROM saved_view ORDER BY sort_key"))?;
        let rows = stmt.query_map([], map)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn update_saved_view(&mut self, id: Id, patch: SavedViewPatch) -> Result<SavedView> {
        let current = self.saved_view(id)?;
        let mut changes: Vec<(&str, Value)> = Vec::new();
        if let Some(name) = &patch.name {
            changes.push(("name", text(clean_name(name, "view name")?)));
        }
        if let Some(spec) = &patch.spec_json {
            changes.push(("spec_json", text(clean_spec(spec)?)));
        }
        if changes.is_empty() {
            return Ok(current);
        }
        self.transact(&format!("Edit view “{}”", current.name), |tx| {
            tx.update(Table::SavedView, &[id.into()], &changes)
        })?;
        self.saved_view(id)
    }

    pub fn delete_saved_view(&mut self, id: Id) -> Result<()> {
        let view = self.saved_view(id)?;
        self.transact(&format!("Delete view “{}”", view.name), |tx| {
            tx.delete(Table::SavedView, &[id.into()])
        })
    }
}
