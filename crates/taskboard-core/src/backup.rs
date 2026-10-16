//! Backups (SPEC §5 Data safety): verified copies in `backups/` next to the
//! database, automatic daily backups with retention, and restore.

use std::path::{Path, PathBuf};

use jiff::civil::Date;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use ts_rs::TS;

use crate::clock::now_ms;
use crate::error::{Error, Result, invalid};
use crate::events::local_zone;
use crate::storage::Db;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    /// `daily`, `manual`, `pre-migrate`, `pre-import` or `pre-restore`.
    pub kind: String,
    pub created_at: i64,
    pub size_bytes: i64,
}

/// File names: `taskboard-<kind>-<YYYYMMDD-HHMMSS>.db` (and older
/// `pre-migrate-v<n>-<ms>.db` from migrations).
fn describe(path: &Path) -> Option<BackupInfo> {
    let name = path.file_name()?.to_str()?.to_owned();
    if !name.ends_with(".db") {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    let created_at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0);
    let kind = if let Some(rest) = name.strip_prefix("taskboard-") {
        rest.split('-')
            .take_while(|p| !p.chars().all(|c| c.is_ascii_digit()))
            .collect::<Vec<_>>()
            .join("-")
    } else if name.starts_with("pre-migrate") {
        "pre-migrate".into()
    } else {
        "other".into()
    };
    Some(BackupInfo {
        file_name: name,
        path: path.to_string_lossy().into_owned(),
        kind,
        created_at,
        size_bytes: i64::try_from(meta.len()).unwrap_or(i64::MAX),
    })
}

/// Opens a backup read-only and runs `quick_check`.
pub fn verify(path: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(Error::Integrity(result))
    }
}

fn local_date(ms: i64) -> Option<Date> {
    let (tz, _) = local_zone();
    jiff::Timestamp::from_millisecond(ms)
        .ok()
        .map(|t| t.to_zoned(tz).date())
}

impl Db {
    pub fn backup_dir(&self) -> Result<PathBuf> {
        let path = self
            .path()
            .ok_or_else(|| invalid("in-memory databases have no backups"))?;
        Ok(path.parent().unwrap_or(Path::new(".")).join("backups"))
    }

    /// Writes a consistent copy (`VACUUM INTO`) and verifies it.
    pub fn backup_now(&self, kind: &str) -> Result<BackupInfo> {
        let dir = self.backup_dir()?;
        std::fs::create_dir_all(&dir)?;
        let (tz, _) = local_zone();
        let stamp = jiff::Timestamp::from_millisecond(now_ms())
            .map(|t| t.to_zoned(tz).strftime("%Y%m%d-%H%M%S").to_string())
            .map_err(|e| invalid(e.to_string()))?;
        let mut target = dir.join(format!("taskboard-{kind}-{stamp}.db"));
        let mut n = 1;
        while target.exists() {
            target = dir.join(format!("taskboard-{kind}-{stamp}-{n}.db"));
            n += 1;
        }
        let target_str = target
            .to_str()
            .ok_or_else(|| invalid("backup path is not valid UTF-8"))?;
        self.conn().execute("VACUUM INTO ?1", [target_str])?;
        if let Err(err) = verify(&target) {
            let _ = std::fs::remove_file(&target);
            return Err(err);
        }
        describe(&target).ok_or_else(|| invalid("backup was not written"))
    }

    /// All backups, newest first.
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let dir = self.backup_dir()?;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut out: Vec<BackupInfo> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| describe(&e.path()))
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(out)
    }

    /// Makes today's automatic backup if there isn't one yet, then prunes.
    pub fn ensure_daily_backup(&self) -> Result<Option<BackupInfo>> {
        if self.path().is_none() {
            return Ok(None);
        }
        let today = local_date(now_ms());
        let have_today = self
            .list_backups()?
            .iter()
            .any(|b| b.kind == "daily" && local_date(b.created_at) == today);
        if have_today {
            return Ok(None);
        }
        let made = self.backup_now("daily")?;
        self.prune_backups()?;
        Ok(Some(made))
    }

    /// Retention: daily backups keep the newest per day for 7 days, per week
    /// for 4 weeks, per month for 12 months. Other kinds are kept 90 days.
    /// Returns how many files were removed.
    pub fn prune_backups(&self) -> Result<usize> {
        let now = now_ms();
        let day = 86_400_000;
        let mut buckets = std::collections::HashSet::new();
        let mut removed = 0;
        for b in self.list_backups()? {
            let age = now - b.created_at;
            let keep = if b.kind == "daily" {
                let Some(d) = local_date(b.created_at) else {
                    continue;
                };
                let week = d.iso_week_date();
                let key = if age <= 7 * day {
                    format!("d{d}")
                } else if age <= 35 * day {
                    format!("w{}-{}", week.year(), week.week())
                } else if age <= 366 * day {
                    format!("m{}-{}", d.year(), d.month())
                } else {
                    String::new()
                };
                // Newest first, so the first backup per bucket is the one kept.
                !key.is_empty() && buckets.insert(key)
            } else {
                age <= 90 * day
            };
            if !keep && std::fs::remove_file(&b.path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// Replaces the database at `target` with `backup` (after verifying it).
/// The caller must have closed every connection to `target` first.
pub fn restore_file(backup: &Path, target: &Path) -> Result<()> {
    verify(backup)?;
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", target.display()));
        if side.exists() {
            std::fs::remove_file(side)?;
        }
    }
    let tmp = target.with_extension("restore-tmp");
    std::fs::copy(backup, &tmp)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}
