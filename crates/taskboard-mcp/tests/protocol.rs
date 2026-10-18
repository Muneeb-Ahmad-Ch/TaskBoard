//! The MCP server end to end: JSON-RPC framing, the handshake, and every
//! tool against a real (in-memory) database.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use serde_json::{Value, json};
use taskboard_core::Db;
use taskboard_mcp::Server;

fn server() -> Server {
    let mut db = Db::open_in_memory().unwrap();
    db.apply_setup(taskboard_core::Setup::Freelancer).unwrap();
    Server::new(db, None)
}

/// Sends a request and returns the `result` (panics on a JSON-RPC error).
fn call(s: &mut Server, method: &str, params: Value) -> Value {
    let reply = s
        .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .expect("a request gets a response");
    assert_eq!(reply["jsonrpc"], "2.0");
    assert_eq!(reply["id"], 1);
    assert!(reply.get("error").is_none(), "unexpected error: {reply}");
    reply["result"].clone()
}

/// Calls a tool and returns its parsed JSON text content.
fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    let result = call(s, "tools/call", json!({ "name": name, "arguments": args }));
    assert_ne!(result["isError"], true, "tool failed: {result}");
    let text = result["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap_or(Value::String(text.to_owned()))
}

fn tool_err(s: &mut Server, name: &str, args: Value) -> String {
    let result = call(s, "tools/call", json!({ "name": name, "arguments": args }));
    assert_eq!(result["isError"], true, "expected a tool error: {result}");
    result["content"][0]["text"].as_str().unwrap().to_owned()
}

#[test]
fn initialize_echoes_a_supported_protocol_version_and_names_the_server() {
    let mut s = server();
    let r = call(
        &mut s,
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert_eq!(r["protocolVersion"], "2025-03-26");
    assert_eq!(r["serverInfo"]["name"], "taskboard");
    assert!(r["capabilities"]["tools"].is_object());
    assert!(r["instructions"].as_str().unwrap().contains("get_overview"));
    // An unknown version gets the newest one we speak.
    let r = call(
        &mut s,
        "initialize",
        json!({ "protocolVersion": "1999-01-01" }),
    );
    assert_eq!(r["protocolVersion"], taskboard_mcp::LATEST_PROTOCOL);
}

#[test]
fn notifications_get_no_response_and_ping_pongs() {
    let mut s = server();
    assert!(
        s.handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .is_none()
    );
    assert_eq!(call(&mut s, "ping", json!({})), json!({}));
}

#[test]
fn malformed_input_is_a_json_rpc_error() {
    let mut s = server();
    let r = s.handle_text("{not json").unwrap();
    assert_eq!(r["error"]["code"], -32700);
    let r = s
        .handle(json!({ "jsonrpc": "2.0", "id": 7, "method": "no/such" }))
        .unwrap();
    assert_eq!(r["error"]["code"], -32601);
    assert_eq!(r["id"], 7);
    let r = s.handle(json!({ "jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": { "name": "nope" } })).unwrap();
    assert_eq!(r["result"]["isError"], true);
}

#[test]
fn tools_list_has_schemas_for_every_tool() {
    let mut s = server();
    let r = call(&mut s, "tools/list", json!({}));
    let tools = r["tools"].as_array().unwrap();
    assert!(tools.len() >= 20);
    for t in tools {
        assert!(t["name"].is_string());
        assert!(
            !t["description"].as_str().unwrap().is_empty(),
            "{}",
            t["name"]
        );
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in [
        "get_overview",
        "list_tasks",
        "add_task",
        "update_task",
        "complete_task",
        "delete_task",
        "undo",
    ] {
        assert!(names.contains(&n), "missing {n}");
    }
}

#[test]
fn overview_lists_sections_groups_projects_and_counts() {
    let mut s = server();
    let o = tool(&mut s, "get_overview", json!({}));
    let names: Vec<&str> = o["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Clients", "Personal"]);
    assert!(
        o["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["is_inbox"] == true)
    );
    assert!(o["today"].as_str().unwrap().len() == 10);
    assert!(o["counts"]["inbox"].is_number());
}

#[test]
fn add_task_uses_quick_add_syntax_and_an_explicit_project_wins() {
    let mut s = server();
    let o = tool(&mut s, "get_overview", json!({}));
    let personal = o["sections"][1]["id"].as_str().unwrap().to_owned();
    let p = tool(
        &mut s,
        "add_project",
        json!({ "name": "Website", "section_id": personal }),
    );
    let pid = p["id"].as_str().unwrap().to_owned();

    let t = tool(
        &mut s,
        "add_task",
        json!({ "text": "Ship homepage tomorrow 3pm #launch !high ~2h" }),
    );
    assert_eq!(t["title"], "Ship homepage");
    assert_eq!(t["priority"], "high");
    assert_eq!(t["due_time"], "15:00");
    assert_eq!(t["estimate_minutes"], 120);
    assert_eq!(t["tags"], json!(["launch"]));
    assert_eq!(t["project_name"], "Inbox");

    let t2 = tool(
        &mut s,
        "add_task",
        json!({ "text": "Fix footer @website", "project_id": pid }),
    );
    assert_eq!(t2["project_id"], pid);
    let t3 = tool(&mut s, "add_task", json!({ "text": "Write copy @web" }));
    assert_eq!(t3["project_id"], pid, "the @hint resolves by itself");
    let err = tool_err(&mut s, "add_task", json!({ "text": "Nothing @nowhere" }));
    assert!(err.contains("nowhere"), "{err}");
}

#[test]
fn update_complete_and_delete_a_task() {
    let mut s = server();
    let t = tool(&mut s, "add_task", json!({ "text": "Draft report" }));
    let id = t["id"].as_str().unwrap().to_owned();
    let u = tool(
        &mut s,
        "update_task",
        json!({ "id": id, "due_date": "2026-11-02", "priority": "urgent", "description_md": "Two pages." }),
    );
    assert_eq!(u["due_date"], "2026-11-02");
    assert_eq!(u["priority"], "urgent");
    let u = tool(&mut s, "update_task", json!({ "id": id, "due_date": null }));
    assert!(u["due_date"].is_null(), "null clears a field");
    let c = tool(&mut s, "complete_task", json!({ "id": id }));
    assert!(c["completed_at"].is_number());
    let c = tool(&mut s, "complete_task", json!({ "id": id, "done": false }));
    assert!(c["completed_at"].is_null());
    assert_eq!(
        tool(&mut s, "delete_task", json!({ "id": id }))["deleted"],
        true
    );
    let err = tool_err(&mut s, "get_task", json!({ "id": id }));
    assert!(err.to_lowercase().contains("not found"), "{err}");
}

#[test]
fn unknown_fields_and_bad_ids_are_reported_not_ignored() {
    let mut s = server();
    let t = tool(&mut s, "add_task", json!({ "text": "Draft report" }));
    let id = t["id"].as_str().unwrap().to_owned();
    let err = tool_err(
        &mut s,
        "update_task",
        json!({ "id": id, "due": "2026-11-02" }),
    );
    assert!(err.contains("due") && err.contains("due_date"), "{err}");
    let err = tool_err(&mut s, "get_task", json!({ "id": "not-an-id" }));
    assert!(err.contains("id"), "{err}");
    let err = tool_err(&mut s, "add_task", json!({}));
    assert!(err.contains("text"), "{err}");
}

#[test]
fn list_tasks_filters_and_get_task_includes_subtasks() {
    let mut s = server();
    let a = tool(&mut s, "add_task", json!({ "text": "Parent task" }));
    let aid = a["id"].as_str().unwrap().to_owned();
    tool(
        &mut s,
        "add_task",
        json!({ "text": "Child one", "parent_id": aid }),
    );
    tool(&mut s, "add_task", json!({ "text": "Other thing #admin" }));
    let l = tool(&mut s, "list_tasks", json!({}));
    assert_eq!(l["total"], 2, "top level only by default: {l}");
    let l = tool(&mut s, "list_tasks", json!({ "search": "other" }));
    assert_eq!(l["tasks"][0]["title"], "Other thing");
    let l = tool(&mut s, "list_tasks", json!({ "tag": "admin" }));
    assert_eq!(l["total"], 1);
    let g = tool(&mut s, "get_task", json!({ "id": aid }));
    assert_eq!(g["subtasks"][0]["title"], "Child one");
    assert_eq!(g["status"], "To Do");
    tool(&mut s, "complete_task", json!({ "id": aid }));
    assert_eq!(
        tool(&mut s, "list_tasks", json!({ "status": "done" }))["total"],
        1
    );
    assert_eq!(
        tool(&mut s, "list_tasks", json!({ "status": "open" }))["total"],
        1
    );
}

#[test]
fn groups_projects_sections_round_trip() {
    let mut s = server();
    let o = tool(&mut s, "get_overview", json!({}));
    let clients = o["sections"][0]["id"].as_str().unwrap().to_owned();
    let g = tool(
        &mut s,
        "add_group",
        json!({ "name": "Acme", "section_id": clients, "time_zone": "Europe/London" }),
    );
    let gid = g["id"].as_str().unwrap().to_owned();
    let p = tool(
        &mut s,
        "add_project",
        json!({ "name": "Site", "client_id": gid }),
    );
    assert_eq!(
        p["section_id"], clients,
        "a project in a group lives in the group's section"
    );
    let p2 = tool(
        &mut s,
        "update_project",
        json!({ "id": p["id"], "name": "Website", "deadline": "2026-12-01" }),
    );
    assert_eq!(p2["name"], "Website");
    let g2 = tool(
        &mut s,
        "update_group",
        json!({ "id": gid, "contact": "pat@example.com" }),
    );
    assert_eq!(g2["contact"], "pat@example.com");
    let sec = tool(
        &mut s,
        "add_section",
        json!({ "name": "Teams", "item_name": "team" }),
    );
    let sid = sec["id"].as_str().unwrap().to_owned();
    let sec = tool(
        &mut s,
        "update_section",
        json!({ "id": sid, "color": "150" }),
    );
    assert_eq!(sec["color"], "150");
    assert_eq!(
        tool(&mut s, "delete_section", json!({ "id": sid }))["deleted"],
        true
    );
    assert_eq!(
        tool(&mut s, "archive_project", json!({ "id": p["id"] }))["archived"],
        true
    );
    assert_eq!(
        tool(
            &mut s,
            "archive_project",
            json!({ "id": p["id"], "archived": false })
        )["archived"],
        false
    );
    assert_eq!(
        tool(&mut s, "delete_project", json!({ "id": p["id"] }))["deleted"],
        true
    );
    assert_eq!(
        tool(&mut s, "delete_group", json!({ "id": gid }))["deleted"],
        true
    );
}

#[test]
fn events_take_local_iso_times_and_list_as_iso() {
    let mut s = server();
    let e = tool(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Kickoff", "start": "2026-10-20T15:00", "end": "2026-10-20T16:00", "meeting_url": "https://meet.example.com/x" }),
    );
    let id = e["id"].as_str().unwrap().to_owned();
    assert_eq!(e["start"], "2026-10-20T15:00");
    let l = tool(
        &mut s,
        "list_events",
        json!({ "from": "2026-10-20", "to": "2026-10-21" }),
    );
    assert_eq!(l["events"][0]["title"], "Kickoff");
    assert_eq!(l["events"][0]["start"], "2026-10-20T15:00");
    let e = tool(
        &mut s,
        "update_event",
        json!({ "id": id, "title": "Kickoff call", "end": "2026-10-20T16:30" }),
    );
    assert_eq!(e["end"], "2026-10-20T16:30");
    let err = tool_err(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "x", "start": "next tuesday", "end": "2026-10-20T16:00" }),
    );
    assert!(err.contains("start"), "{err}");
    assert_eq!(
        tool(&mut s, "delete_event", json!({ "id": id }))["deleted"],
        true
    );
}

#[test]
fn log_time_and_dashboard() {
    let mut s = server();
    let t = tool(&mut s, "add_task", json!({ "text": "Deep work" }));
    let r = tool(
        &mut s,
        "log_time",
        json!({ "task_id": t["id"], "started": "2026-10-20T09:00", "ended": "2026-10-20T10:30" }),
    );
    assert_eq!(r["minutes"], 90);
    let d = tool(&mut s, "get_dashboard", json!({}));
    assert!(
        d["due_today"].is_number() && d["projects"].is_array(),
        "{d}"
    );
}

#[test]
fn undo_and_redo_report_what_they_did() {
    let mut s = server();
    tool(&mut s, "add_task", json!({ "text": "Draft report" }));
    let u = tool(&mut s, "undo", json!({}));
    assert!(u["undid"].as_str().unwrap().contains("Draft report"));
    assert_eq!(tool(&mut s, "list_tasks", json!({}))["total"], 0);
    let r = tool(&mut s, "redo", json!({}));
    assert!(r["redid"].as_str().unwrap().contains("Draft report"));
    assert_eq!(tool(&mut s, "list_tasks", json!({}))["total"], 1);
    let err = tool_err(&mut s, "redo", json!({}));
    assert!(!err.is_empty());
}

#[test]
fn ai_access_switch_blocks_tools_but_not_the_handshake() {
    let mut db = Db::open_in_memory().unwrap();
    let mut settings = db.app_settings().unwrap();
    settings.ai_access = false;
    db.set_app_settings(settings).unwrap();
    let mut s = Server::new(db, None);
    call(
        &mut s,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    );
    let err = tool_err(&mut s, "list_tasks", json!({}));
    assert!(err.contains("Settings"), "{err}");
}

#[test]
fn connections_are_recorded_for_the_settings_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Server::new(
        Db::open_in_memory().unwrap(),
        Some(dir.path().to_path_buf()),
    );
    call(
        &mut s,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "clientInfo": { "name": "claude-desktop", "version": "1.2" } }),
    );
    call(
        &mut s,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "clientInfo": { "name": "claude-desktop", "version": "1.3" } }),
    );
    let log = taskboard_mcp::connections(dir.path());
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].name, "claude-desktop");
    assert_eq!(log[0].version, "1.3");
    assert_eq!(log[0].connections, 2);
}

#[test]
fn serve_reads_newline_delimited_requests_and_skips_blank_lines() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n",
    );
    let mut out = Vec::new();
    let s = Server::new(Db::open_in_memory().unwrap(), None);
    taskboard_mcp::serve(s, input.as_bytes(), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "one response per request, none for the notification"
    );
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], 2);
}
