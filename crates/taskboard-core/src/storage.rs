//! SQLite storage: opening, pragmas, migrations and integrity checks.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use rusqlite_migration::{M, Migrations};
use serde::Serialize;
use ts_rs::TS;

use crate::clock::{Hlc, now_ms};
use crate::error::{Error, Result};
use crate::id::Id;
use crate::tx::{OpKind, Replay, Tx};

/// Forward-only migrations. Never edit a released entry; append a new one.
const MIGRATIONS: &[M<'static>] = &[
    M::up(
        "CREATE TABLE app_meta (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        ) STRICT;",
    ),
    M::up(include_str!("migrations/0002_domain.sql")),
    M::up(include_str!("migrations/0003_query_indexes.sql")),
];

/// Schema version this build writes (`PRAGMA user_version`).
pub const SCHEMA_VERSION: usize = MIGRATIONS.len();

/// Applied to every connection. `synchronous=FULL` keeps committed edits safe
/// across power loss (SPEC §7); writes are rare, so the cost is negligible.
const CONNECTION_PRAGMAS: &str = "
    PRAGMA foreign_keys = ON;
    PRAGMA synchronous = FULL;
    PRAGMA temp_store = MEMORY;
";

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// An open, migrated, integrity-checked database.
#[derive(Debug)]
pub struct Db {
    conn: Connection,
    device: Id,
    hlc: Hlc,
    /// Last transaction time; each transaction gets a strictly later one.
    last_now: i64,
}

/// What the next undo / redo would do, for menus and tooltips.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HistoryState {
    pub undo: Option<String>,
    pub redo: Option<String>,
}

/// Facts about the open database, shown in Settings → Diagnostics.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DbInfo {
    pub sqlite_version: String,
    pub fts5: bool,
    pub journal_mode: String,
    pub schema_version: u32,
}

impl Db {
    /// Opens (creating if needed) the database at `path`, migrates it to the
    /// latest schema, and runs a quick integrity check.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn, Some(path))
    }

    /// In-memory database for tests and benchmarks.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, path: Option<&Path>) -> Result<Self> {
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // journal_mode returns a row, so it can't go in execute_batch.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch(CONNECTION_PRAGMAS)?;

        let version = check_not_newer(&conn)?;
        if let Some(path) = path
            && version > 0
            && version < MIGRATIONS.len()
        {
            backup_before_migration(&conn, path, version)?;
        }
        Migrations::from_slice(MIGRATIONS).to_latest(&mut conn)?;
        quick_check(&conn)?;
        // Refresh planner statistics when stale or missing (cheap, bounded).
        conn.execute_batch("PRAGMA optimize = 0x10002;")?;

        let device = load_device_id(&conn)?;
        let last: Option<String> =
            conn.query_row("SELECT max(hlc) FROM op_log", [], |r| r.get(0))?;
        let hlc = last.as_deref().and_then(Hlc::decode).unwrap_or_default();

        let last_now = hlc.ms();
        let mut db = Self {
            conn,
            device,
            hlc,
            last_now,
        };
        db.ensure_inbox()?;
        Ok(db)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Wall time for a new transaction, strictly after the previous one. This
    /// makes `archived_at` / `deleted_at` stamps unique per action (cascades
    /// are undone by matching them) and survives the clock stepping back.
    fn next_now(&mut self) -> i64 {
        self.last_now = now_ms().max(self.last_now + 1);
        self.last_now
    }

    /// Runs `f` as one undoable user action named `label`.
    pub fn transact<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Tx<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut tx = {
            let now = self.next_now();
            Tx::begin(&mut self.conn, &mut self.hlc, self.device, OpKind::Do, now)?
        };
        let out = f(&mut tx)?;
        if tx.ops() > 0 {
            tx.conn()
                .execute("DELETE FROM history WHERE undone = 1", [])?;
            tx.conn().execute(
                "INSERT INTO history (group_id, label, at) VALUES (?1, ?2, ?3)",
                rusqlite::params![tx.group(), label, tx.now()],
            )?;
        }
        tx.commit()?;
        Ok(out)
    }

    /// Runs `f` as a logged change that is not on the undo stack (seeding, maintenance).
    pub(crate) fn transact_system<T>(
        &mut self,
        f: impl FnOnce(&mut Tx<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut tx = {
            let now = self.next_now();
            Tx::begin(
                &mut self.conn,
                &mut self.hlc,
                self.device,
                OpKind::System,
                now,
            )?
        };
        let out = f(&mut tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Reverts the most recent action. Returns its label.
    pub fn undo(&mut self) -> Result<String> {
        let entry: Option<(Id, String)> = self
            .conn
            .query_row(
                "SELECT group_id, label FROM history WHERE undone = 0 ORDER BY pos DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (group, label) = entry.ok_or(Error::NothingTo("undo"))?;
        let mut tx = {
            let now = self.next_now();
            Tx::begin(
                &mut self.conn,
                &mut self.hlc,
                self.device,
                OpKind::Undo,
                now,
            )?
        };
        tx.replay(group, Replay::Undo)?;
        tx.conn()
            .execute("UPDATE history SET undone = 1 WHERE group_id = ?1", [group])?;
        tx.commit()?;
        Ok(label)
    }

    /// Re-applies the most recently undone action. Returns its label.
    pub fn redo(&mut self) -> Result<String> {
        let entry: Option<(Id, String)> = self
            .conn
            .query_row(
                "SELECT group_id, label FROM history WHERE undone = 1 ORDER BY pos ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (group, label) = entry.ok_or(Error::NothingTo("redo"))?;
        let mut tx = {
            let now = self.next_now();
            Tx::begin(
                &mut self.conn,
                &mut self.hlc,
                self.device,
                OpKind::Redo,
                now,
            )?
        };
        tx.replay(group, Replay::Redo)?;
        tx.conn()
            .execute("UPDATE history SET undone = 0 WHERE group_id = ?1", [group])?;
        tx.commit()?;
        Ok(label)
    }

    pub fn history_state(&self) -> Result<HistoryState> {
        let undo = self
            .conn
            .query_row(
                "SELECT label FROM history WHERE undone = 0 ORDER BY pos DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let redo = self
            .conn
            .query_row(
                "SELECT label FROM history WHERE undone = 1 ORDER BY pos ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(HistoryState { undo, redo })
    }

    pub fn info(&self) -> Result<DbInfo> {
        let sqlite_version = rusqlite::version().to_owned();
        let journal_mode: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let schema_version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let fts5: bool =
            self.conn
                .query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |r| {
                    r.get(0)
                })?;
        Ok(DbInfo {
            sqlite_version,
            fts5,
            journal_mode,
            schema_version,
        })
    }
}

/// Refuse to open a database written by a newer app version (SPEC §5).
/// Returns the current schema version.
fn check_not_newer(conn: &Connection) -> Result<usize> {
    let found: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let found = found as usize;
    let supported = MIGRATIONS.len();
    if found > supported {
        return Err(Error::SchemaTooNew { found, supported });
    }
    Ok(found)
}

/// Copies the database to `backups/` next to it before upgrading the schema.
fn backup_before_migration(conn: &Connection, path: &Path, version: usize) -> Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new(".")).join("backups");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("pre-migrate-v{version}-{}.db", now_ms()));
    let target_str = target
        .to_str()
        .ok_or_else(|| Error::InvalidInput("backup path is not valid UTF-8".into()))?;
    conn.execute("VACUUM INTO ?1", [target_str])?;
    tracing::info!(path = %target.display(), "backup before migration");
    Ok(target)
}

/// Stable per-database device id, used to attribute changes for sync.
fn load_device_id(conn: &Connection) -> Result<Id> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM app_meta WHERE key = 'device_id'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing.and_then(|s| s.parse().ok()) {
        return Ok(id);
    }
    let id = Id::new();
    conn.execute(
        "INSERT OR REPLACE INTO app_meta (key, value) VALUES ('device_id', ?1)",
        [id.to_string()],
    )?;
    Ok(id)
}

fn quick_check(conn: &Connection) -> Result<()> {
    let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(Error::Integrity(result))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        Migrations::from_slice(MIGRATIONS).validate().unwrap();
    }

    #[test]
    fn opens_file_db_in_wal_with_fts5() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("nested/taskboard.db")).unwrap();
        let info = db.info().unwrap();
        assert_eq!(info.journal_mode, "wal");
        assert!(info.fts5, "bundled SQLite must include FTS5");
        assert_eq!(info.schema_version as usize, MIGRATIONS.len());
    }

    #[test]
    fn reopening_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("taskboard.db");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.info().unwrap().schema_version as usize, MIGRATIONS.len());
    }

    #[test]
    fn refuses_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("taskboard.db");
        drop(Db::open(&path).unwrap());
        let future = MIGRATIONS.len() as i64 + 1;
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", future)
            .unwrap();
        let err = Db::open(&path).unwrap_err();
        assert_eq!(err.code(), "schema_too_new");
    }
}
