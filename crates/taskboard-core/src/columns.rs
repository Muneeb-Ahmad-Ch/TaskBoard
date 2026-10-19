//! The board columns new projects start with (a setting; built in by
//! default), adding the ones a project is missing, and one-time backfills
//! when a version adds a column to the defaults (Waiting, 0.5.1).
//!
//! Columns belong to each project, so a project can have its own workflow;
//! these are only the starting set and a way to bring projects in line.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Result, invalid};
use crate::id::Id;
use crate::model::{Category, clean_color, clean_name, int, text};
use crate::order::key_between;
use crate::statuses::{siblings, slot_for};
use crate::storage::Db;
use crate::tx::{Table, Tx};

/// One column of the starting workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DefaultColumn {
    pub name: String,
    pub category: Category,
    /// Hue ("230") or "gray".
    pub color: String,
}

const SETTING: &str = "default_columns";
const MAX_COLUMNS: usize = 20;

/// The built-in workflow (SPEC §4.2). Waiting: the ball is in someone
/// else's court (a reply, an approval); still open.
const BUILT_IN: [(&str, Category, &str); 6] = [
    ("Backlog", Category::Todo, "gray"),
    ("To Do", Category::Todo, "230"),
    ("In Progress", Category::Active, "262"),
    ("Waiting", Category::Active, "70"),
    ("Review", Category::Active, "300"),
    ("Done", Category::Done, "150"),
];

pub(crate) fn built_in() -> Vec<DefaultColumn> {
    BUILT_IN
        .iter()
        .map(|(name, category, color)| DefaultColumn {
            name: (*name).to_owned(),
            category: *category,
            color: (*color).to_owned(),
        })
        .collect()
}

/// 1–20 named columns, no name twice (any case), valid colors, somewhere for
/// open work and a Done column (where finished tasks go).
fn clean_columns(cols: Vec<DefaultColumn>) -> Result<Vec<DefaultColumn>> {
    if cols.is_empty() {
        return Err(invalid("keep at least one column"));
    }
    if cols.len() > MAX_COLUMNS {
        return Err(invalid(format!("at most {MAX_COLUMNS} columns")));
    }
    let mut out: Vec<DefaultColumn> = Vec::with_capacity(cols.len());
    for c in cols {
        let name = clean_name(&c.name, "column name")?;
        if out.iter().any(|o| o.name.eq_ignore_ascii_case(&name)) {
            return Err(invalid(format!("“{name}” is in the list twice")));
        }
        let color = clean_color(&c.color)?;
        out.push(DefaultColumn {
            name,
            category: c.category,
            color,
        });
    }
    if !out
        .iter()
        .any(|c| matches!(c.category, Category::Todo | Category::Active))
    {
        return Err(invalid("add a column for open work (to do or in progress)"));
    }
    if !out.iter().any(|c| c.category == Category::Done) {
        return Err(invalid("add a Done column, where finished tasks go"));
    }
    Ok(out)
}

/// The saved list, or the built-in one when none is saved (or it's unusable).
pub(crate) fn default_columns_in(conn: &Connection) -> Vec<DefaultColumn> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [SETTING], |r| {
            r.get(0)
        })
        .optional()
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str::<Option<Vec<DefaultColumn>>>(&s).ok())
        .flatten()
        .and_then(|cols| clean_columns(cols).ok())
        .unwrap_or_else(built_in)
}

/// For each live project (archived ones too), the indexes of the default
/// columns it lacks, by name in any case.
fn missing(conn: &Connection, defaults: &[DefaultColumn]) -> Result<Vec<(Id, Vec<usize>)>> {
    let projects: Vec<Id> = {
        let mut stmt = conn.prepare(
            "SELECT p.id FROM project p WHERE p.deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM status s WHERE s.project_id = p.id AND s.deleted_at IS NULL)
             ORDER BY p.sort_key",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut names =
        conn.prepare("SELECT name FROM status WHERE project_id = ?1 AND deleted_at IS NULL")?;
    let mut out = Vec::new();
    for project in projects {
        let have: Vec<String> = names
            .query_map([project], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let lack: Vec<usize> = defaults
            .iter()
            .enumerate()
            .filter(|(_, c)| !have.iter().any(|h| h.eq_ignore_ascii_case(&c.name)))
            .map(|(i, _)| i)
            .collect();
        if !lack.is_empty() {
            out.push((project, lack));
        }
    }
    Ok(out)
}

/// Adds default column `i` to `project`: after the column that comes before
/// it in the defaults when the project has that one, else where its
/// category belongs.
fn insert_column(tx: &mut Tx<'_>, project: Id, defaults: &[DefaultColumn], i: usize) -> Result<()> {
    let Some(col) = defaults.get(i) else {
        return Ok(());
    };
    let all = siblings(tx, project)?;
    let previous = i.checked_sub(1).and_then(|j| defaults.get(j));
    let at = previous.and_then(|p| {
        all.iter()
            .position(|s| s.name.eq_ignore_ascii_case(&p.name))
    });
    let (before, after) = match at {
        Some(k) => (
            all.get(k).map(|s| s.sort_key.clone()),
            all.get(k + 1).map(|s| s.sort_key.clone()),
        ),
        None => slot_for(&all, col.category),
    };
    tx.insert(
        Table::Status,
        &[
            ("id", Id::new().into()),
            ("project_id", project.into()),
            ("name", text(col.name.clone())),
            ("color", text(col.color.clone())),
            ("category", text(col.category.as_str())),
            (
                "sort_key",
                text(key_between(before.as_deref(), after.as_deref())?),
            ),
            ("created_at", int(tx.now())),
        ],
    )
}

fn marked(conn: &Connection, key: &str) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM app_meta WHERE key = ?1", [key], |_| Ok(()))
        .optional()?
        .is_some())
}

impl Db {
    /// The columns new projects start with.
    pub fn default_columns(&self) -> Result<Vec<DefaultColumn>> {
        Ok(default_columns_in(self.conn()))
    }

    /// Sets the columns new projects start with. Existing projects keep
    /// theirs; [`Db::add_missing_columns`] brings them in line. Not an undo
    /// step (a setting, like the others).
    pub fn set_default_columns(&mut self, cols: Vec<DefaultColumn>) -> Result<Vec<DefaultColumn>> {
        let cols = clean_columns(cols)?;
        self.set_setting(SETTING, &Some(&cols))?;
        Ok(cols)
    }

    /// Back to the built-in columns.
    pub fn reset_default_columns(&mut self) -> Result<Vec<DefaultColumn>> {
        self.set_setting(SETTING, &None::<Vec<DefaultColumn>>)?;
        Ok(built_in())
    }

    /// How many projects lack at least one of the default columns.
    pub fn projects_missing_columns(&self) -> Result<usize> {
        Ok(missing(self.conn(), &default_columns_in(self.conn()))?.len())
    }

    /// Adds the default columns each project is missing (nothing is removed
    /// or renamed). One undo step; returns how many projects changed (0 adds
    /// no undo step).
    pub fn add_missing_columns(&mut self) -> Result<usize> {
        let defaults = default_columns_in(self.conn());
        let todo = missing(self.conn(), &defaults)?;
        if todo.is_empty() {
            return Ok(0);
        }
        let n = todo.len();
        let label = format!(
            "Add missing columns to {n} project{}",
            if n == 1 { "" } else { "s" }
        );
        self.transact(&label, |tx| {
            for (project, lack) in &todo {
                for &i in lack {
                    insert_column(tx, *project, &defaults, i)?;
                }
            }
            Ok(())
        })?;
        Ok(n)
    }

    /// One-time updates to data made by older versions, each run once per
    /// database and not on the undo stack. 0.5.1: projects made before
    /// Waiting was a default column get it (unless the defaults no longer
    /// include it). Deleting it afterwards is respected.
    pub(crate) fn run_backfills(&mut self) -> Result<()> {
        const WAITING: &str = "backfill:waiting-column";
        if marked(self.conn(), WAITING)? {
            return Ok(());
        }
        let defaults = default_columns_in(self.conn());
        self.transact_system(|tx| {
            // Checked again under the write lock: processes opening at once add it once.
            if marked(tx.conn(), WAITING)? {
                return Ok(());
            }
            if let Some(i) = defaults
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case("waiting"))
            {
                for (project, lack) in missing(tx.conn(), &defaults)? {
                    if lack.contains(&i) {
                        insert_column(tx, project, &defaults, i)?;
                    }
                }
            }
            tx.conn().execute(
                "INSERT OR REPLACE INTO app_meta (key, value) VALUES (?1, 'done')",
                [WAITING],
            )?;
            Ok(())
        })
    }
}
