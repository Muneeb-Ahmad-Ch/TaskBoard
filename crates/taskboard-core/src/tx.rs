//! The single mutation path (SPEC §3).
//!
//! Every write goes through [`Tx`], which applies it in one SQLite transaction,
//! stamps sync columns, and appends a before/after record to `op_log`. Undo and
//! redo replay those records, so they work for every table without per-entity
//! code. The log is append-only: undo writes new records rather than deleting.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde_json::{Map, Value as Json, json};

use crate::clock::Hlc;
use crate::error::{Error, Result};
use crate::id::Id;

/// Every table the log knows how to replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Client,
    Project,
    Status,
    Task,
    Tag,
    TaskTag,
    Event,
    EventException,
    WorkingHours,
    DayOverride,
    CalendarFeed,
    Reminder,
    TimeEntry,
    Attachment,
    SavedView,
    Setting,
}

const ALL_TABLES: [Table; 16] = [
    Table::Client,
    Table::Project,
    Table::Status,
    Table::Task,
    Table::Tag,
    Table::TaskTag,
    Table::Event,
    Table::EventException,
    Table::WorkingHours,
    Table::DayOverride,
    Table::CalendarFeed,
    Table::Reminder,
    Table::TimeEntry,
    Table::Attachment,
    Table::SavedView,
    Table::Setting,
];

/// Columns stamped on every write to a syncable table; never logged, always fresh.
const SYNC_COLS: [&str; 3] = ["updated_at", "hlc", "device_id"];

impl Table {
    pub fn name(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Project => "project",
            Self::Status => "status",
            Self::Task => "task",
            Self::Tag => "tag",
            Self::TaskTag => "task_tag",
            Self::Event => "event",
            Self::EventException => "event_exception",
            Self::WorkingHours => "working_hours",
            Self::DayOverride => "day_override",
            Self::CalendarFeed => "calendar_feed",
            Self::Reminder => "reminder",
            Self::TimeEntry => "time_entry",
            Self::Attachment => "attachment",
            Self::SavedView => "saved_view",
            Self::Setting => "setting",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        ALL_TABLES.into_iter().find(|t| t.name() == name)
    }

    pub(crate) fn key_cols(self) -> &'static [&'static str] {
        match self {
            Self::TaskTag => &["task_id", "tag_id"],
            Self::EventException => &["event_id", "original_start"],
            Self::WorkingHours => &["weekday", "start_time"],
            Self::DayOverride => &["date"],
            Self::Setting => &["key"],
            _ => &["id"],
        }
    }

    fn syncable(self) -> bool {
        matches!(
            self,
            Self::Client | Self::Project | Self::Status | Self::Task | Self::Tag | Self::Event
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpKind {
    Do,
    Undo,
    Redo,
    System,
}

impl OpKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Do => "do",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::System => "system",
        }
    }
}

type Row = Vec<(String, Value)>;

/// A write transaction that records everything it changes.
pub struct Tx<'c> {
    tx: Transaction<'c>,
    hlc: &'c mut Hlc,
    device: Id,
    group: Id,
    kind: OpKind,
    now: i64,
    ops: usize,
}

impl std::fmt::Debug for Tx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tx")
            .field("group", &self.group)
            .field("ops", &self.ops)
            .finish_non_exhaustive()
    }
}

impl<'c> Tx<'c> {
    pub(crate) fn begin(
        conn: &'c mut Connection,
        hlc: &'c mut Hlc,
        device: Id,
        kind: OpKind,
        now: i64,
    ) -> Result<Self> {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        Ok(Self {
            tx,
            hlc,
            device,
            group: Id::new(),
            kind,
            now,
            ops: 0,
        })
    }

    /// Read access inside the transaction (sees this transaction's writes).
    pub fn conn(&self) -> &Connection {
        &self.tx
    }

    /// Wall time captured when the transaction began; use it for every timestamp.
    pub fn now(&self) -> i64 {
        self.now
    }

    pub(crate) fn group(&self) -> Id {
        self.group
    }

    pub(crate) fn ops(&self) -> usize {
        self.ops
    }

    pub(crate) fn commit(self) -> Result<()> {
        self.tx.commit()?;
        Ok(())
    }

    fn stamp(&mut self, table: Table, row: &mut Row) {
        if table.syncable() {
            let hlc = self.hlc.tick(self.now);
            row.push(("updated_at".into(), Value::Integer(self.now)));
            row.push(("hlc".into(), Value::Text(hlc)));
            row.push(("device_id".into(), self.device.into()));
        }
    }

    pub fn insert<S: AsRef<str>>(&mut self, table: Table, cols: &[(S, Value)]) -> Result<()> {
        let logged: Row = cols
            .iter()
            .filter(|(c, _)| !SYNC_COLS.contains(&c.as_ref()))
            .map(|(c, v)| (c.as_ref().to_owned(), v.clone()))
            .collect();
        let key = key_of(table, &logged)?;
        let mut row = logged.clone();
        self.stamp(table, &mut row);

        let names: Vec<&str> = row.iter().map(|(c, _)| c.as_str()).collect();
        let marks = vec!["?"; names.len()].join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({marks})",
            table.name(),
            names.join(", ")
        );
        self.tx
            .execute(&sql, params_from_iter(row.iter().map(|(_, v)| v)))?;
        self.log(table, &key, "insert", None, Some(&logged))
    }

    /// Updates the given columns; columns whose value is unchanged are skipped.
    /// Returns false (and logs nothing) when nothing actually changed.
    pub fn update<S: AsRef<str>>(
        &mut self,
        table: Table,
        key: &[Value],
        changes: &[(S, Value)],
    ) -> Result<bool> {
        let names: Vec<&str> = changes.iter().map(|(c, _)| c.as_ref()).collect();
        let current = self.select_cols(table, key, &names)?;
        let mut before = Row::new();
        let mut after = Row::new();
        for ((name, new), old) in changes.iter().zip(current) {
            if SYNC_COLS.contains(&name.as_ref()) || *new == old {
                continue;
            }
            before.push((name.as_ref().to_owned(), old));
            after.push((name.as_ref().to_owned(), new.clone()));
        }
        if after.is_empty() {
            return Ok(false);
        }
        let mut set = after.clone();
        self.stamp(table, &mut set);

        let assignments: Vec<String> = set.iter().map(|(c, _)| format!("{c} = ?")).collect();
        let sql = format!(
            "UPDATE {} SET {} WHERE {}",
            table.name(),
            assignments.join(", "),
            where_key(table)
        );
        let values = set.iter().map(|(_, v)| v).chain(key.iter());
        self.tx.execute(&sql, params_from_iter(values))?;
        self.log(table, key, "update", Some(&before), Some(&after))?;
        Ok(true)
    }

    /// Hard delete, logging the full row so undo can restore it.
    pub fn delete(&mut self, table: Table, key: &[Value]) -> Result<()> {
        let row = self.snapshot(table, key)?;
        let sql = format!("DELETE FROM {} WHERE {}", table.name(), where_key(table));
        self.tx.execute(&sql, params_from_iter(key.iter()))?;
        self.log(table, key, "delete", Some(&row), None)
    }

    fn select_cols(&self, table: Table, key: &[Value], cols: &[&str]) -> Result<Vec<Value>> {
        let sql = format!(
            "SELECT {} FROM {} WHERE {}",
            cols.join(", "),
            table.name(),
            where_key(table)
        );
        self.tx
            .query_row(&sql, params_from_iter(key.iter()), |r| {
                (0..cols.len()).map(|i| r.get::<_, Value>(i)).collect()
            })
            .optional()?
            .ok_or(Error::NotFound(table.name()))
    }

    fn snapshot(&self, table: Table, key: &[Value]) -> Result<Row> {
        let sql = format!("SELECT * FROM {} WHERE {}", table.name(), where_key(table));
        let mut stmt = self.tx.prepare(&sql)?;
        let names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let values: Vec<Value> = stmt
            .query_row(params_from_iter(key.iter()), |r| {
                (0..names.len()).map(|i| r.get::<_, Value>(i)).collect()
            })
            .optional()?
            .ok_or(Error::NotFound(table.name()))?;
        Ok(names
            .into_iter()
            .zip(values)
            .filter(|(c, _)| !SYNC_COLS.contains(&c.as_str()))
            .collect())
    }

    fn log(
        &mut self,
        table: Table,
        key: &[Value],
        op: &str,
        before: Option<&Row>,
        after: Option<&Row>,
    ) -> Result<()> {
        let hlc = self.hlc.tick(self.now);
        let key_json = Json::Array(key.iter().map(encode).collect()).to_string();
        self.tx.execute(
            "INSERT INTO op_log (group_id, kind, hlc, device_id, entity, key_json, op,
                                 before_json, after_json, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                self.group,
                self.kind.as_str(),
                hlc,
                self.device,
                table.name(),
                key_json,
                op,
                before.map(row_json),
                after.map(row_json),
                self.now,
            ],
        )?;
        self.ops += 1;
        Ok(())
    }

    /// Applies the inverse (undo) or the original (redo) of every op in `group`.
    pub(crate) fn replay(&mut self, group: Id, direction: Replay) -> Result<()> {
        let ops = load_ops(&self.tx, group)?;
        let ordered: Box<dyn Iterator<Item = &LoggedOp>> = match direction {
            Replay::Undo => Box::new(ops.iter().rev()),
            Replay::Redo => Box::new(ops.iter()),
        };
        for op in ordered {
            match (direction, op.op.as_str()) {
                (Replay::Undo, "insert") | (Replay::Redo, "delete") => {
                    self.delete(op.table, &op.key)?;
                }
                (Replay::Undo, "delete") => self.insert(op.table, &op.before)?,
                (Replay::Redo, "insert") => self.insert(op.table, &op.after)?,
                (Replay::Undo, "update") => {
                    self.update(op.table, &op.key, &op.before)?;
                }
                (Replay::Redo, "update") => {
                    self.update(op.table, &op.key, &op.after)?;
                }
                (_, other) => return Err(Error::Log(format!("unknown op {other}"))),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Replay {
    Undo,
    Redo,
}

struct LoggedOp {
    table: Table,
    key: Vec<Value>,
    op: String,
    before: Row,
    after: Row,
}

fn load_ops(conn: &Connection, group: Id) -> Result<Vec<LoggedOp>> {
    let mut stmt = conn.prepare(
        "SELECT entity, key_json, op, before_json, after_json
         FROM op_log WHERE group_id = ?1 ORDER BY seq",
    )?;
    let raw = stmt
        .query_map([group], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter()
        .map(|(entity, key, op, before, after)| {
            let table = Table::from_name(&entity)
                .ok_or_else(|| Error::Log(format!("unknown table {entity}")))?;
            let key = match serde_json::from_str::<Json>(&key)? {
                Json::Array(items) => items.iter().map(decode).collect::<Result<_>>()?,
                _ => return Err(Error::Log("key is not an array".into())),
            };
            Ok(LoggedOp {
                table,
                key,
                op,
                before: before
                    .map(|s| parse_row(&s))
                    .transpose()?
                    .unwrap_or_default(),
                after: after
                    .map(|s| parse_row(&s))
                    .transpose()?
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn where_key(table: Table) -> String {
    table
        .key_cols()
        .iter()
        .map(|c| format!("{c} = ?"))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn key_of(table: Table, row: &Row) -> Result<Vec<Value>> {
    table
        .key_cols()
        .iter()
        .map(|k| {
            row.iter()
                .find(|(c, _)| c == k)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Error::Log(format!("{} insert without {k}", table.name())))
        })
        .collect()
}

// JSON encoding of SQLite values that round-trips exactly:
// integers as numbers, text as strings, reals as {"f": x}, blobs as {"b": hex}.
fn encode(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Integer(i) => json!(i),
        Value::Real(f) => json!({ "f": f }),
        Value::Text(s) => Json::String(s.clone()),
        Value::Blob(b) => json!({ "b": hex(b) }),
    }
}

fn decode(json: &Json) -> Result<Value> {
    match json {
        Json::Null => Ok(Value::Null),
        Json::Number(n) => n
            .as_i64()
            .map(Value::Integer)
            .ok_or_else(|| Error::Log(format!("non-integer number {n}"))),
        Json::String(s) => Ok(Value::Text(s.clone())),
        Json::Object(map) => {
            if let Some(f) = map.get("f").and_then(Json::as_f64) {
                Ok(Value::Real(f))
            } else if let Some(h) = map.get("b").and_then(Json::as_str) {
                unhex(h).map(Value::Blob)
            } else {
                Err(Error::Log("unknown tagged value".into()))
            }
        }
        _ => Err(Error::Log("unexpected JSON value".into())),
    }
}

fn row_json(row: &Row) -> String {
    let map: Map<String, Json> = row.iter().map(|(c, v)| (c.clone(), encode(v))).collect();
    Json::Object(map).to_string()
}

fn parse_row(s: &str) -> Result<Row> {
    match serde_json::from_str::<Json>(s)? {
        Json::Object(map) => map.into_iter().map(|(c, v)| Ok((c, decode(&v)?))).collect(),
        _ => Err(Error::Log("row is not an object".into())),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(Error::Log("odd hex length".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            s.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| Error::Log("bad hex".into()))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn value_json_round_trip() {
        let values = [
            Value::Null,
            Value::Integer(-42),
            Value::Real(1.0),
            Value::Real(0.25),
            Value::Text("héllo".into()),
            Value::Blob(vec![0, 1, 254, 255]),
        ];
        for v in values {
            assert_eq!(decode(&encode(&v)).unwrap(), v);
        }
    }

    #[test]
    fn every_table_name_resolves() {
        for t in ALL_TABLES {
            assert_eq!(Table::from_name(t.name()), Some(t));
        }
    }
}
