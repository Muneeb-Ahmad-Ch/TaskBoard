//! AI assistants: `taskboard-app.exe --mcp` runs the MCP server on stdio for
//! Claude, Codex, Cursor and friends, against the same database as the app.
//! While the app runs, a watcher notices their commits and refreshes the UI.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use taskboard_core::Db;
use taskboard_core::storage::{default_data_dir, default_db_path};
use taskboard_mcp::{Connection, Server};
use tauri::{AppHandle, Emitter, State};

use crate::{AppState, CmdResult, internal, read};

pub const FLAG: &str = "--mcp";

/// The stdio server. Logs go to stderr: stdout is the protocol stream.
pub fn run_server() -> Result<(), String> {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();
    let path = default_db_path().ok_or("cannot find the data folder (APPDATA is not set)")?;
    let db = Db::open(&path).map_err(|e| format!("opening {}: {e}", path.display()))?;
    tracing::info!(path = %path.display(), "MCP server ready");
    let server = Server::new(db, default_data_dir());
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    taskboard_mcp::serve(server, stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

/// Emits `data-changed` whenever another process (the MCP server, the CLI)
/// commits, so the open views pick the change up within about a second.
pub fn spawn_change_watcher(app: AppHandle, db: Arc<Mutex<Db>>) {
    std::thread::Builder::new()
        .name("change-watcher".into())
        .spawn(move || {
            let mut last: Option<i64> = None;
            let mut autostart: Option<bool> = None;
            loop {
                std::thread::sleep(Duration::from_millis(1200));
                let (version, start) = match db.lock() {
                    Ok(db) => (
                        db.data_version().ok(),
                        db.app_settings().ok().map(|s| s.autostart),
                    ),
                    Err(_) => (None, None),
                };
                let Some(version) = version else { continue };
                if last.is_some_and(|v| v != version) {
                    tracing::debug!("outside change detected");
                    if let Err(err) = app.emit("data-changed", ()) {
                        tracing::warn!(%err, "failed to emit data-changed");
                    }
                    // An assistant turned "Start with Windows" on or off.
                    if let Some(start) = start
                        && autostart.is_some_and(|a| a != start)
                    {
                        crate::desktop::sync_autostart(&app, start);
                    }
                }
                last = Some(version);
                if start.is_some() {
                    autostart = start;
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|err| tracing::warn!(%err, "change watcher unavailable"));
}

/// What Settings → AI assistants shows.
#[derive(Debug, Serialize)]
pub struct McpInfo {
    /// The program an assistant runs (this exe) and its argument.
    pub command: String,
    pub args: Vec<String>,
    pub db_path: String,
    pub connections: Vec<Connection>,
}

#[tauri::command]
pub async fn mcp_info(state: State<'_, AppState>) -> CmdResult<McpInfo> {
    let db_path = read(state, |db| {
        Ok(db
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_default())
    })
    .await?;
    let command = std::env::current_exe()
        .map_err(internal)?
        .display()
        .to_string();
    let connections = default_data_dir()
        .map(|dir| taskboard_mcp::connections(&dir))
        .unwrap_or_default();
    Ok(McpInfo {
        command,
        args: vec![FLAG.to_owned()],
        db_path,
        connections,
    })
}

#[derive(Debug, Serialize)]
pub struct McpTest {
    pub ok: bool,
    pub message: String,
    pub tools: usize,
}

/// Starts this exe with `--mcp`, performs the handshake and lists tools, the
/// way an assistant would, so the user can see it works on this machine.
#[tauri::command]
pub async fn mcp_self_test() -> CmdResult<McpTest> {
    tauri::async_runtime::spawn_blocking(self_test)
        .await
        .map_err(internal)
}

fn self_test() -> McpTest {
    let fail = |message: String| McpTest {
        ok: false,
        message,
        tools: 0,
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return fail(format!("cannot locate the app: {e}")),
    };
    let mut child = match Command::new(&exe)
        .arg(FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(format!("could not start the server: {e}")),
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return fail("no pipes to the server".to_owned());
    };
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let requests = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"taskboard-self-test","version":"1"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n"
    );
    if let Err(e) = stdin
        .write_all(requests.as_bytes())
        .and_then(|()| stdin.flush())
    {
        let _ = child.kill();
        return fail(format!("could not talk to the server: {e}"));
    }
    let mut result = fail("the server did not answer within 10 seconds".to_owned());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut version = String::new();
    while let Ok(line) =
        rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if msg["id"] == 1 {
            version = msg["result"]["serverInfo"]["version"]
                .as_str()
                .unwrap_or("?")
                .to_owned();
        }
        if msg["id"] == 2 {
            let tools = msg["result"]["tools"].as_array().map_or(0, Vec::len);
            result = McpTest {
                ok: tools > 0,
                message: format!("Taskboard {version} answered with {tools} tools."),
                tools,
            };
            break;
        }
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}
