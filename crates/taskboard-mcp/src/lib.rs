//! Taskboard as an MCP server (Model Context Protocol), so AI assistants —
//! Claude Desktop, Claude Code, Codex, Cursor and any other MCP client — can
//! read and change sections, groups, projects, tasks, meetings and time.
//!
//! Transport: newline-delimited JSON-RPC 2.0 over stdin/stdout, the MCP
//! "stdio" transport. Every write goes through the core like the app's own
//! edits do, so it is undoable and the app refreshes as soon as it lands.
//! Tool failures come back as tool results with `isError`, never as protocol
//! errors, so the assistant can read the message and try again.

mod tools;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use taskboard_core::Db;

/// Protocol revisions this server speaks, newest first.
pub const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const LATEST_PROTOCOL: &str = PROTOCOLS[0];

const SERVER_NAME: &str = "taskboard";
const INSTRUCTIONS: &str = "Taskboard (the user may call it \"tb\") is the user's local planner: sections (e.g. Clients, Personal) hold groups (clients, teams…) and projects; projects hold tasks and subtasks; the Inbox is a project for unfiled tasks. Each project has board columns (statuses such as To Do, In Progress, Waiting, Done): set a task's column with update_task status (the column name) and keep it in step with progress. When the user is waiting on someone (a reply, an approval), move the task to Waiting (add_missing_columns adds default columns a project lacks) and set its due date to the day to follow up. Meetings and time blocks: list_events (filter by kind, group, project or task), add_event, update_event; schedule_task books focus time for a task. Call get_overview first to learn the ids and each project's columns, then list_tasks / get_task for detail. update_task, complete_task, archive_task and delete_task take ids for several tasks at once (one undo step). Archive finished work instead of deleting it. Ids are opaque strings; dates are YYYY-MM-DD; times are HH:MM (24h); event times are local YYYY-MM-DDTHH:MM. add_task understands quick-add syntax (\"Send invoice fri 3pm #admin !high ~30m @website every month\"). Every change is undoable (undo / redo) and shows up in the app at once.";

/// One assistant that has connected, for the app's Settings page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    pub version: String,
    /// Unix time in milliseconds of the latest connection.
    pub last_seen: i64,
    pub connections: u32,
}

const CONNECTIONS_FILE: &str = "mcp-connections.json";

/// Assistants that have connected before (newest first); empty when none or unreadable.
pub fn connections(data_dir: &Path) -> Vec<Connection> {
    let mut list: Vec<Connection> = std::fs::read(data_dir.join(CONNECTIONS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    list.sort_by_key(|c| std::cmp::Reverse(c.last_seen));
    list
}

fn record_connection(data_dir: &Path, name: &str, version: &str) {
    let mut list = connections(data_dir);
    let now = jiff::Timestamp::now().as_millisecond();
    match list.iter_mut().find(|c| c.name == name) {
        Some(c) => {
            c.version = version.to_owned();
            c.last_seen = now;
            c.connections = c.connections.saturating_add(1);
        }
        None => list.push(Connection {
            name: name.to_owned(),
            version: version.to_owned(),
            last_seen: now,
            connections: 1,
        }),
    }
    list.sort_by_key(|c| std::cmp::Reverse(c.last_seen));
    list.truncate(10);
    // Written whole, then renamed, so the app never reads a half-written file.
    let path = data_dir.join(CONNECTIONS_FILE);
    let tmp = data_dir.join(format!("{CONNECTIONS_FILE}.tmp"));
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(&tmp, serde_json::to_vec_pretty(&list)?)?;
        std::fs::rename(&tmp, &path)
    };
    if let Err(err) = write() {
        tracing::warn!(%err, "could not record the MCP connection");
    }
}

/// The server: one database, one client.
#[derive(Debug)]
pub struct Server {
    db: Db,
    /// Where connection records go (the app's data directory); None = don't record.
    data_dir: Option<PathBuf>,
    client: Option<String>,
}

impl Server {
    pub fn new(db: Db, data_dir: Option<PathBuf>) -> Self {
        Self {
            db,
            data_dir,
            client: None,
        }
    }

    /// Handles one raw line; a parse error is answered like JSON-RPC says.
    pub fn handle_text(&mut self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.handle(msg),
            Err(err) => Some(error(Value::Null, -32700, &format!("parse error: {err}"))),
        }
    }

    /// Handles one message; returns the response, or None for a notification.
    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned().filter(|v| !v.is_null());
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return Some(error(
                id.unwrap_or(Value::Null),
                -32600,
                "invalid request: no method",
            ));
        };
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
        if method.starts_with("notifications/") {
            return None;
        }
        let Some(id) = id else {
            // A request without an id is a notification we do not know; stay quiet.
            return None;
        };
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => Ok(self.call_tool(&params)),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = PROTOCOLS
            .iter()
            .copied()
            .find(|v| *v == requested)
            .unwrap_or(LATEST_PROTOCOL);
        let name = params
            .pointer("/clientInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let client_version = params
            .pointer("/clientInfo/version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        tracing::info!(client = %name, version = %client_version, protocol = version, "assistant connected");
        if let Some(dir) = &self.data_dir {
            record_connection(dir, &name, &client_version);
        }
        self.client = Some(name);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER_NAME, "title": "Taskboard (tb)", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn call_tool(&mut self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let outcome = match self.db.app_settings() {
            Ok(s) if !s.ai_access => Err(
                "AI access is turned off in Taskboard (Settings → AI assistants). Ask the user to turn it on."
                    .to_owned(),
            ),
            Ok(_) => tools::call(&mut self.db, name, args),
            Err(err) => Err(err.to_string()),
        };
        match outcome {
            Ok(value) => {
                let text = serde_json::to_string(&value).unwrap_or_else(|_| value.to_string());
                json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value })
            }
            Err(message) => {
                tracing::debug!(tool = name, %message, "tool call failed");
                json!({ "content": [{ "type": "text", "text": message }], "isError": true })
            }
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Runs the server until the input ends: one JSON message per line in, one
/// response per request out. Blank lines are skipped.
pub fn serve(
    mut server: Server,
    input: impl BufRead,
    mut output: impl Write,
) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_text(&line) {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}
