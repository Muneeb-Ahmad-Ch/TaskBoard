//! Meetings and time blocks through MCP: filtering, the task a block is for,
//! booking time for a task, all-day events, and moves that keep the length.
//! Also the Waiting column and the "tb" nickname.
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

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    s.handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }))
        .unwrap()["result"]
        .clone()
}

fn ok(s: &mut Server, name: &str, args: Value) -> Value {
    let r = call(s, name, args);
    assert_ne!(
        r["isError"], true,
        "{name} failed: {}",
        r["content"][0]["text"]
    );
    serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn err(s: &mut Server, name: &str, args: Value) -> String {
    let r = call(s, name, args);
    assert_eq!(r["isError"], true, "{name} should fail: {r}");
    r["content"][0]["text"].as_str().unwrap().to_owned()
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_owned()
}

fn project(s: &mut Server, name: &str) -> String {
    let o = ok(s, "get_overview", json!({}));
    let personal = o["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Personal")
        .unwrap()["id"]
        .clone();
    id(&ok(
        s,
        "add_project",
        json!({ "name": name, "section_id": personal }),
    ))
}

const DAY: &str = "2031-03-04";

fn events(s: &mut Server, filter: Value) -> Vec<Value> {
    let mut args = json!({ "from": DAY, "to": "2031-03-06" });
    for (k, v) in filter.as_object().unwrap() {
        args[k] = v.clone();
    }
    ok(s, "list_events", args)["events"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn time_blocks_are_booked_for_a_task_and_listed_with_it() {
    let mut s = server();
    let p = project(&mut s, "Site");
    let t = id(&ok(
        &mut s,
        "add_task",
        json!({ "text": "Write copy", "project_id": p, "estimate_minutes": 90 }),
    ));
    // Length from the task's estimate; title and project from the task.
    let block = ok(
        &mut s,
        "schedule_task",
        json!({ "task_id": t, "start": format!("{DAY}T10:00") }),
    );
    assert_eq!(block["kind"], "block");
    assert_eq!(block["title"], "Write copy");
    assert_eq!(block["task_id"], t.as_str());
    assert_eq!(block["project_id"], p.as_str());
    assert_eq!(block["start"], format!("{DAY}T10:00"));
    assert_eq!(block["end"], format!("{DAY}T11:30"));
    // An explicit length wins.
    let short = ok(
        &mut s,
        "schedule_task",
        json!({ "task_id": t, "start": format!("{DAY}T14:00"), "minutes": 30 }),
    );
    assert_eq!(short["end"], format!("{DAY}T14:30"));
    err(
        &mut s,
        "schedule_task",
        json!({ "task_id": t, "start": format!("{DAY}T14:00"), "minutes": 2 }),
    );

    ok(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Kickoff", "project_id": p, "start": format!("{DAY}T09:00"), "end": format!("{DAY}T09:30") }),
    );
    assert_eq!(events(&mut s, json!({})).len(), 3);
    let blocks = events(&mut s, json!({ "kind": "block" }));
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| b["task_id"] == t.as_str()));
    assert_eq!(
        events(&mut s, json!({ "kind": "meeting" }))[0]["title"],
        "Kickoff"
    );
    assert_eq!(events(&mut s, json!({ "task_id": t })).len(), 2);
    assert_eq!(events(&mut s, json!({ "project_id": p })).len(), 3);
    let other = project(&mut s, "Other");
    assert!(events(&mut s, json!({ "project_id": other })).is_empty());
    err(&mut s, "list_events", json!({ "kind": "lunch" }));
}

#[test]
fn all_day_events_take_plain_dates_and_moves_keep_the_length() {
    let mut s = server();
    let off = ok(
        &mut s,
        "add_event",
        json!({ "kind": "block", "title": "Conference", "start": DAY, "end": "2031-03-05" }),
    );
    assert_eq!(off["all_day_start"], DAY);
    assert_eq!(off["all_day_end"], "2031-03-05");
    let one = ok(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Holiday", "start": "2031-03-05" }),
    );
    assert_eq!(one["all_day_end"], "2031-03-05");
    let listed = events(&mut s, json!({}));
    assert!(listed.iter().all(|e| e["all_day"] == true));
    err(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Mixed", "start": DAY, "end": format!("{DAY}T10:00") }),
    );
    err(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Back", "start": "2031-03-05", "end": DAY }),
    );

    // A timed event without an end lasts an hour; moving its start keeps the length.
    let m = ok(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Call", "start": format!("{DAY}T10:00") }),
    );
    assert_eq!(m["end"], format!("{DAY}T11:00"));
    let moved = ok(
        &mut s,
        "update_event",
        json!({ "id": m["id"], "start": format!("{DAY}T15:00") }),
    );
    assert_eq!(
        (moved["start"].clone(), moved["end"].clone()),
        (json!(format!("{DAY}T15:00")), json!(format!("{DAY}T16:00")))
    );
    // Timed → all-day and back.
    let day = ok(
        &mut s,
        "update_event",
        json!({ "id": m["id"], "start": DAY }),
    );
    assert_eq!(day["all_day_start"], DAY);
    assert!(day["start_at"].is_null());
    let timed = ok(
        &mut s,
        "update_event",
        json!({ "id": m["id"], "start": format!("{DAY}T08:00"), "end": format!("{DAY}T08:45") }),
    );
    assert!(timed["all_day_start"].is_null());
    assert_eq!(timed["end"], format!("{DAY}T08:45"));
}

#[test]
fn waiting_columns_can_be_added_to_every_project() {
    let mut s = server();
    let p = project(&mut s, "Site");
    let cols = ok(&mut s, "list_statuses", json!({ "project_id": p }))["statuses"].clone();
    let waiting = cols
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Waiting")
        .unwrap()
        .clone();
    assert_eq!(waiting["category"], "active");
    // Remove it, then add it back everywhere it is missing.
    let todo = cols
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "To Do")
        .unwrap()["id"]
        .clone();
    ok(
        &mut s,
        "delete_status",
        json!({ "id": waiting["id"], "move_to": todo }),
    );
    let r = ok(
        &mut s,
        "add_status_everywhere",
        json!({ "name": "Waiting", "category": "active" }),
    );
    assert_eq!(r["projects_updated"], 1);
    let r = ok(
        &mut s,
        "add_status_everywhere",
        json!({ "name": "Waiting", "category": "active" }),
    );
    assert_eq!(r["projects_updated"], 0);
    let t = id(&ok(
        &mut s,
        "add_task",
        json!({ "text": "Ask for logo", "project_id": p }),
    ));
    let moved = ok(
        &mut s,
        "update_task",
        json!({ "id": t, "status": "waiting" }),
    );
    assert_eq!(
        (moved["status"].clone(), moved["category"].clone()),
        (json!("Waiting"), json!("active"))
    );
}

#[test]
fn the_server_answers_to_tb() {
    let mut s = server();
    let init = s
        .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }))
        .unwrap();
    let r = &init["result"];
    assert!(r["serverInfo"]["title"].as_str().unwrap().contains("tb"));
    assert!(r["instructions"].as_str().unwrap().contains("\"tb\""));
}

#[test]
fn starter_columns_can_be_read_changed_and_applied() {
    let mut s = server();
    let cols = ok(&mut s, "get_default_columns", json!({}));
    let names: Vec<&str> = cols["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ]
    );
    // Change them (a color may be left out); new projects follow.
    ok(
        &mut s,
        "set_default_columns",
        json!({ "columns": [
            { "name": "Ideas", "category": "todo" },
            { "name": "Doing", "category": "active", "color": "262" },
            { "name": "Shipped", "category": "done" }
        ] }),
    );
    let p = project(&mut s, "Fresh");
    let fresh = ok(&mut s, "list_statuses", json!({ "project_id": p }))["statuses"].clone();
    let fresh: Vec<&str> = fresh
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(fresh, ["Ideas", "Doing", "Shipped"]);
    // A list with nowhere for finished work is refused.
    err(
        &mut s,
        "set_default_columns",
        json!({ "columns": [{ "name": "Only", "category": "todo" }] }),
    );
    // Older projects get the missing ones in one step.
    let r = ok(&mut s, "add_missing_columns", json!({}));
    assert!(r["projects_updated"].as_u64().unwrap() >= 1, "{r}");
    assert_eq!(
        ok(&mut s, "add_missing_columns", json!({}))["projects_updated"],
        0
    );
}
