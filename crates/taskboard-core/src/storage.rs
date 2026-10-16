//! SQLite storage: opening, pragmas, migrations and integrity checks.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use serde::Serialize;
use ts_rs::TS;

use crate::error::{Error, Result};

/// Forward-only migrations. Never edit a released entry; append a new one.
/// M0 only creates `app_meta`; the domain schema arrives in M1 (docs/DATA_MODEL.md).
const MIGRATIONS: &[M<'static>] = &[M::up(
    "CREATE TABLE app_meta (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    ) STRICT;",
)];

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
        Self::init(conn)
    }

    /// In-memory database for tests and benchmarks.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // journal_mode returns a row, so it can't go in execute_batch.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch(CONNECTION_PRAGMAS)?;

        check_not_newer(&conn)?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut conn)?;
        quick_check(&conn)?;

        Ok(Self { conn })
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
fn check_not_newer(conn: &Connection) -> Result<()> {
    let found: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let found = found as usize;
    let supported = MIGRATIONS.len();
    if found > supported {
        return Err(Error::SchemaTooNew { found, supported });
    }
    Ok(())
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
