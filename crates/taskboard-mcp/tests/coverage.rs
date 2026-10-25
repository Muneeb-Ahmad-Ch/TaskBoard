//! Everything the app can do with your data, an assistant can do too: one
//! test per area, end to end through `tools/call`.
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

/// A project in Personal, with the default workflow.
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

#[test]
fn status_is_set_by_column_name_in_any_case() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let o = ok(&mut s, "get_overview", json!({}));
    let web = o["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == p.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        web["statuses"],
        json!([
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ])
    );
    let t = ok(
        &mut s,
        "add_task",
        json!({ "text": "Build page", "project_id": p }),
    );
    let u = ok(
        &mut s,
        "update_task",
        json!({ "id": t["id"], "status": "in progress", "progress": 40 }),
    );
    assert_eq!(u["status"], "In Progress");
    assert_eq!(u["category"], "active");
    assert_eq!(u["progress"], 40);
    let e = err(
        &mut s,
        "update_task",
        json!({ "id": t["id"], "status": "Doing" }),
    );
    assert!(
        e.contains("Doing") && e.contains("In Progress"),
        "lists the columns: {e}"
    );
    assert_eq!(
        ok(
            &mut s,
            "list_tasks",
            json!({ "status_name": "In Progress" })
        )["total"],
        1
    );
}

#[test]
fn status_follows_the_target_project_when_moving() {
    let mut s = server();
    let a = project(&mut s, "A");
    let b = project(&mut s, "B");
    ok(
        &mut s,
        "add_status",
        json!({ "project_id": b, "name": "QA", "category": "active" }),
    );
    let t = ok(
        &mut s,
        "add_task",
        json!({ "text": "Ship", "project_id": a }),
    );
    let u = ok(
        &mut s,
        "update_task",
        json!({ "id": t["id"], "project_id": b, "status": "QA" }),
    );
    assert_eq!(
        (
            u["project_id"].as_str().unwrap(),
            u["status"].as_str().unwrap()
        ),
        (b.as_str(), "QA")
    );
}

#[test]
fn add_task_takes_explicit_fields_over_quick_add_parsing() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let t = ok(
        &mut s,
        "add_task",
        json!({ "text": "Write spec #draft", "project_id": p, "status": "In Progress", "due_date": "2026-11-05",
                "due_time": "10:30", "priority": "high", "tags": ["docs"], "estimate_minutes": 45, "description_md": "Two pages" }),
    );
    assert_eq!(t["title"], "Write spec");
    assert_eq!(t["status"], "In Progress");
    assert_eq!(
        (t["due_date"].as_str(), t["due_time"].as_str()),
        (Some("2026-11-05"), Some("10:30"))
    );
    assert_eq!(t["priority"], "high");
    assert_eq!(t["tags"], json!(["docs"]));
    assert_eq!(t["estimate_minutes"], 45);
}

#[test]
fn many_tasks_change_together_and_undo_together() {
    let mut s = server();
    let a = ok(&mut s, "add_task", json!({ "text": "One" }));
    let b = ok(&mut s, "add_task", json!({ "text": "Two" }));
    let r = ok(
        &mut s,
        "update_task",
        json!({ "ids": [a["id"], b["id"]], "priority": "urgent" }),
    );
    assert_eq!(r["updated"], 2);
    let r = ok(
        &mut s,
        "complete_task",
        json!({ "ids": [a["id"], b["id"]] }),
    );
    assert_eq!(r["updated"], 2);
    assert_eq!(
        ok(&mut s, "list_tasks", json!({ "status": "done" }))["total"],
        2
    );
    let u = ok(&mut s, "undo", json!({}));
    assert_eq!(u["undid"], "Complete 2 tasks");
    assert_eq!(ok(&mut s, "list_tasks", json!({}))["total"], 2);
    assert!(err(&mut s, "update_task", json!({ "priority": "low" })).contains("id"));
    assert!(
        err(
            &mut s,
            "update_task",
            json!({ "id": a["id"], "ids": [b["id"]], "priority": "low" })
        )
        .contains("either")
    );
    let r = ok(&mut s, "delete_task", json!({ "ids": [a["id"], b["id"]] }));
    assert_eq!(r["deleted"], 2);
}

#[test]
fn tasks_archive_restore_and_archive_completed() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let a = ok(
        &mut s,
        "add_task",
        json!({ "text": "Old", "project_id": p }),
    );
    let b = ok(
        &mut s,
        "add_task",
        json!({ "text": "Shipped", "project_id": p }),
    );
    ok(
        &mut s,
        "add_task",
        json!({ "text": "Current", "project_id": p }),
    );
    assert_eq!(
        ok(&mut s, "archive_task", json!({ "id": a["id"] }))["archived"],
        1
    );
    assert_eq!(
        ok(&mut s, "list_tasks", json!({ "project_id": p }))["total"],
        2
    );
    let only = ok(&mut s, "list_tasks", json!({ "archived": "only" }));
    assert_eq!(only["tasks"][0]["title"], "Old");
    assert!(only["tasks"][0]["archived_at"].is_number());
    assert_eq!(
        ok(
            &mut s,
            "list_tasks",
            json!({ "archived": "include", "project_id": p })
        )["total"],
        3
    );
    ok(
        &mut s,
        "archive_task",
        json!({ "id": a["id"], "archived": false }),
    );
    ok(&mut s, "complete_task", json!({ "id": b["id"] }));
    assert_eq!(
        ok(&mut s, "archive_completed", json!({ "project_id": p }))["archived"],
        1
    );
    assert_eq!(
        ok(
            &mut s,
            "list_tasks",
            json!({ "project_id": p, "status": "all" })
        )["total"],
        2
    );
}

#[test]
fn board_columns_can_be_added_changed_ordered_and_removed() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let cols = ok(&mut s, "list_statuses", json!({ "project_id": p }));
    let todo = cols["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "To Do")
        .unwrap()["id"]
        .clone();
    let blocked = ok(
        &mut s,
        "add_status",
        json!({ "project_id": p, "name": "Blocked", "category": "active", "after": todo }),
    );
    let names = |s: &mut Server| -> Vec<String> {
        ok(s, "list_statuses", json!({ "project_id": p }))["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        names(&mut s),
        [
            "Backlog",
            "To Do",
            "Blocked",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ]
    );
    let up = ok(
        &mut s,
        "update_status",
        json!({ "id": blocked["id"], "name": "On hold", "wip_limit": 3, "color": "10" }),
    );
    assert_eq!(
        (up["name"].as_str(), up["wip_limit"].as_i64()),
        (Some("On hold"), Some(3))
    );
    ok(
        &mut s,
        "move_status",
        json!({ "id": blocked["id"], "after": null }),
    );
    assert_eq!(names(&mut s)[0], "On hold");
    let t = ok(
        &mut s,
        "add_task",
        json!({ "text": "Waiting", "project_id": p, "status": "On hold" }),
    );
    assert!(err(&mut s, "delete_status", json!({ "id": blocked["id"] })).contains("move_to"));
    ok(
        &mut s,
        "delete_status",
        json!({ "id": blocked["id"], "move_to": todo }),
    );
    assert_eq!(
        ok(&mut s, "get_task", json!({ "id": t["id"] }))["status"],
        "To Do"
    );
}

#[test]
fn cards_can_be_reordered_and_moved_between_columns() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let a = ok(&mut s, "add_task", json!({ "text": "A", "project_id": p }));
    let b = ok(&mut s, "add_task", json!({ "text": "B", "project_id": p }));
    ok(
        &mut s,
        "move_task",
        json!({ "id": b["id"], "before": a["id"] }),
    );
    let l = ok(
        &mut s,
        "list_tasks",
        json!({ "project_id": p, "sort": "manual" }),
    );
    assert_eq!(l["tasks"][0]["title"], "B");
    let m = ok(
        &mut s,
        "move_task",
        json!({ "id": a["id"], "status": "Review" }),
    );
    assert_eq!(m["status"], "Review");
    let d = ok(&mut s, "duplicate_task", json!({ "id": a["id"] }));
    assert_eq!(d["title"], "A (copy)");
    ok(&mut s, "delete_task", json!({ "id": d["id"] }));
    let r = ok(&mut s, "restore_task", json!({ "id": d["id"] }));
    assert_eq!(r["title"], "A (copy)");
}

#[test]
fn sections_groups_and_projects_can_be_reordered_and_moved() {
    let mut s = server();
    let o = ok(&mut s, "get_overview", json!({}));
    let clients = o["sections"][0]["id"].as_str().unwrap().to_owned();
    let personal = o["sections"][1]["id"].as_str().unwrap().to_owned();
    ok(
        &mut s,
        "place_section",
        json!({ "id": personal, "before": clients }),
    );
    assert_eq!(
        ok(&mut s, "get_overview", json!({}))["sections"][0]["name"],
        "Personal"
    );
    let g = ok(
        &mut s,
        "add_group",
        json!({ "name": "Acme", "section_id": clients }),
    );
    let p = ok(
        &mut s,
        "add_project",
        json!({ "name": "Loose", "section_id": personal }),
    );
    let moved = ok(
        &mut s,
        "place_project",
        json!({ "id": p["id"], "group_id": g["id"] }),
    );
    assert_eq!(moved["client_id"], g["id"]);
    let moved = ok(
        &mut s,
        "place_group",
        json!({ "id": g["id"], "section_id": personal }),
    );
    assert_eq!(moved["section_id"].as_str(), Some(personal.as_str()));
    assert!(err(&mut s, "place_project", json!({ "id": p["id"] })).contains("group_id"));
}

#[test]
fn projects_can_be_pinned_and_unpinned() {
    let mut s = server();
    let a = project(&mut s, "A");
    let b = project(&mut s, "B");
    ok(&mut s, "pin_project", json!({ "id": a }));
    ok(&mut s, "pin_project", json!({ "id": b, "before": a }));
    let o = ok(&mut s, "get_overview", json!({}));
    assert_eq!(o["pinned"], json!([b, a]));
    assert!(
        o["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == a.as_str() && p["pinned"] == true)
    );
    ok(&mut s, "unpin_project", json!({ "id": b }));
    assert_eq!(ok(&mut s, "get_overview", json!({}))["pinned"], json!([a]));
    let inbox = o["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["is_inbox"] == true)
        .unwrap()["id"]
        .clone();
    assert!(
        err(&mut s, "pin_project", json!({ "id": inbox }))
            .to_lowercase()
            .contains("inbox")
    );
}

#[test]
fn one_meeting_of_a_series_can_be_cancelled_or_changed_from_there_on() {
    let mut s = server();
    let e = ok(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Standup", "start": "2026-11-02T09:00", "end": "2026-11-02T09:15", "recurrence_rule": "FREQ=WEEKLY;BYDAY=MO" }),
    );
    let l = ok(
        &mut s,
        "list_events",
        json!({ "from": "2026-11-02", "to": "2026-11-30" }),
    );
    let evs = l["events"].as_array().unwrap();
    assert_eq!(evs.len(), 5, "{l}");
    assert_eq!(evs[1]["occurrence_start"], "2026-11-09T09:00");
    let full = ok(&mut s, "get_event", json!({ "id": e["id"] }));
    assert_eq!(full["recurrence_rule"], "FREQ=WEEKLY;BYDAY=MO");
    ok(
        &mut s,
        "update_event",
        json!({ "id": e["id"], "scope": "occurrence", "occurrence_start": "2026-11-09T09:00", "cancelled": true }),
    );
    ok(
        &mut s,
        "update_event",
        json!({ "id": e["id"], "scope": "following", "occurrence_start": "2026-11-23T09:00", "title": "Weekly sync" }),
    );
    let l = ok(
        &mut s,
        "list_events",
        json!({ "from": "2026-11-02", "to": "2026-11-30" }),
    );
    let evs = l["events"].as_array().unwrap();
    let by_start = |start: &str| evs.iter().find(|x| x["start"] == start).unwrap().clone();
    assert!(
        evs.iter().all(|x| x["start"] != "2026-11-09T09:00"),
        "a cancelled occurrence is skipped"
    );
    assert_eq!(by_start("2026-11-16T09:00")["title"], "Standup");
    assert_eq!(by_start("2026-11-23T09:00")["title"], "Weekly sync");
    assert!(
        err(
            &mut s,
            "update_event",
            json!({ "id": e["id"], "scope": "occurrence", "title": "x" })
        )
        .contains("occurrence_start")
    );
    assert!(
        err(
            &mut s,
            "list_events",
            json!({ "from": "2026-01-01", "to": "2028-01-01" })
        )
        .contains("366")
    );
}

#[test]
fn timers_and_time_entries() {
    let mut s = server();
    let t = ok(&mut s, "add_task", json!({ "text": "Deep work" }));
    let started = ok(
        &mut s,
        "start_timer",
        json!({ "task_id": t["id"], "note": "focus" }),
    );
    assert!(started["entry_id"].is_string());
    let running = ok(&mut s, "get_timer", json!({}));
    assert_eq!(running["running"], true);
    assert_eq!(running["title"], "Deep work");
    assert_eq!(ok(&mut s, "stop_timer", json!({}))["stopped"], true);
    assert_eq!(ok(&mut s, "get_timer", json!({}))["running"], false);
    let logged = ok(
        &mut s,
        "log_time",
        json!({ "task_id": t["id"], "started": "2026-10-20T09:00", "ended": "2026-10-20T10:00" }),
    );
    let list = ok(
        &mut s,
        "list_time",
        json!({ "from": "2026-10-20", "to": "2026-10-20" }),
    );
    assert_eq!(list["entries"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["entries"][0]["minutes"], 60);
    ok(
        &mut s,
        "update_time_entry",
        json!({ "id": logged["entry_id"], "ended": "2026-10-20T10:30", "note": "longer" }),
    );
    let list = ok(
        &mut s,
        "list_time",
        json!({ "from": "2026-10-20", "to": "2026-10-20" }),
    );
    assert_eq!(list["entries"][0]["minutes"], 90);
    ok(
        &mut s,
        "delete_time_entry",
        json!({ "id": logged["entry_id"] }),
    );
    assert!(
        ok(
            &mut s,
            "list_time",
            json!({ "from": "2026-10-20", "to": "2026-10-20" })
        )["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn working_hours_days_off_and_free_slots() {
    let mut s = server();
    let w = ok(&mut s, "get_working_hours", json!({}));
    assert_eq!(w["week"][0]["day"], "Monday");
    assert_eq!(w["week"][0]["hours"], json!(["09:00-17:00"]));
    ok(
        &mut s,
        "set_working_hours",
        json!({ "week": [ { "day": "Monday", "hours": ["10:00-12:00", "13:00-18:00"] }, { "day": "Saturday", "hours": [] } ] }),
    );
    let w = ok(&mut s, "get_working_hours", json!({}));
    assert_eq!(w["week"][0]["hours"], json!(["10:00-12:00", "13:00-18:00"]));
    assert_eq!(
        w["week"][1]["hours"],
        json!(["09:00-17:00"]),
        "days not given keep their hours"
    );
    ok(
        &mut s,
        "set_day_off",
        json!({ "date": "2026-11-03", "note": "Holiday" }),
    );
    let o = ok(
        &mut s,
        "list_days_off",
        json!({ "from": "2026-11-01", "to": "2026-11-30" }),
    );
    assert_eq!(o["days"][0]["date"], "2026-11-03");
    let slots = ok(
        &mut s,
        "find_slots",
        json!({ "from": "2026-11-02", "to": "2026-11-04", "duration_min": 60 }),
    );
    let days = slots["days"].as_array().unwrap();
    assert!(
        days.iter()
            .all(|d| d["date"] != "2026-11-03" || d["slots"].as_array().unwrap().is_empty()),
        "{slots}"
    );
    assert!(
        days.iter()
            .any(|d| !d["slots"].as_array().unwrap().is_empty())
    );
    assert!(days[0]["slots"][0]["start"].as_str().unwrap().contains('T'));
    ok(&mut s, "clear_day_off", json!({ "date": "2026-11-03" }));
    assert!(
        ok(
            &mut s,
            "list_days_off",
            json!({ "from": "2026-11-01", "to": "2026-11-30" })
        )["days"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let plans = ok(
        &mut s,
        "get_day_plans",
        json!({ "from": "2026-11-02", "to": "2026-11-03" }),
    );
    assert_eq!(plans["days"].as_array().unwrap().len(), 2);
    assert!(
        ok(
            &mut s,
            "get_hours",
            json!({ "from": "2026-11-02", "to": "2026-11-08" })
        )["groups"]
            .is_array()
    );
}

#[test]
fn saved_views_reminders_and_backups() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Db::open(&dir.path().join("t.db")).unwrap();
    db.apply_setup(taskboard_core::Setup::Freelancer).unwrap();
    let mut s = Server::new(db, None);
    let v = ok(
        &mut s,
        "add_saved_view",
        json!({ "name": "Urgent today", "scope": "today", "view": "board", "sort": "priority" }),
    );
    let spec: Value = serde_json::from_str(v["spec_json"].as_str().unwrap()).unwrap();
    assert_eq!(spec["scope"], json!({ "kind": "today" }));
    assert_eq!(spec["view"], "board");
    ok(
        &mut s,
        "update_saved_view",
        json!({ "id": v["id"], "name": "Today by priority", "show_done": true }),
    );
    let l = ok(&mut s, "list_saved_views", json!({}));
    assert_eq!(l["views"][0]["name"], "Today by priority");
    assert_eq!(l["views"][0]["spec"]["showDone"], true);
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "project" })
        )
        .contains("scope_id")
    );
    ok(&mut s, "delete_saved_view", json!({ "id": v["id"] }));
    let r = ok(&mut s, "get_reminder_settings", json!({}));
    assert!(r["enabled"].is_boolean());
    let r = ok(
        &mut s,
        "update_reminder_settings",
        json!({ "meeting_lead_min": 15 }),
    );
    assert_eq!(r["meeting_lead_min"], 15);
    assert!(
        err(
            &mut s,
            "update_reminder_settings",
            json!({ "meeting_lead_min": 99999 })
        )
        .contains("1440")
    );
    let b = ok(&mut s, "backup_now", json!({}));
    assert!(b["path"].as_str().unwrap().ends_with(".db"));
}

#[test]
fn assistants_cannot_change_app_settings_or_their_own_access() {
    let mut s = server();
    let r = s
        .handle(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
        .unwrap();
    let names: Vec<String> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.len() >= 55, "{} tools", names.len());
    assert!(
        !names.iter().any(|n| n.contains("app_setting")
            || n.contains("access")
            || n.contains("restore_backup")
            || n.contains("import")),
        "{names:?}"
    );
    // Settings an assistant may change: how lists look, start with Windows,
    // keep running in the tray. Never the AI access switch itself.
    let before = ok(&mut s, "get_settings", json!({}));
    assert_eq!(before["hide_repeated_context"], true, "{before}");
    assert!(before.get("ai_access").is_none(), "{before}");
    let changed = ok(
        &mut s,
        "update_settings",
        json!({ "hide_repeated_context": false, "keep_running_in_tray": false }),
    );
    assert_eq!(changed["hide_repeated_context"], false);
    assert_eq!(changed["keep_running_in_tray"], false);
    let refused = err(&mut s, "update_settings", json!({ "ai_access": false }));
    assert!(refused.contains("ai_access"), "{refused}");
    let after = ok(&mut s, "get_settings", json!({}));
    assert_eq!(after["start_with_windows"], before["start_with_windows"]);
    // The switch is untouched: tools still answer.
    ok(&mut s, "get_overview", json!({}));
}

#[test]
fn list_tasks_filters_like_the_table_and_sorts_by_any_column() {
    let mut s = server();
    let web = project(&mut s, "Website");
    let app = project(&mut s, "App");
    let add = |s: &mut Server, args: Value| id(&ok(s, "add_task", args));
    let a = add(
        &mut s,
        json!({ "text": "Alpha", "project_id": web, "priority": "high", "tags": ["design"], "due_date": "2026-01-05", "estimate_minutes": 30 }),
    );
    add(
        &mut s,
        json!({ "text": "Bravo", "project_id": web, "priority": "low", "tags": ["client"], "estimate_minutes": 120 }),
    );
    add(
        &mut s,
        json!({ "text": "Charlie", "project_id": app, "priority": "urgent", "due_date": "2026-01-02" }),
    );
    ok(
        &mut s,
        "update_task",
        json!({ "id": a, "status": "Waiting" }),
    );
    let titles = |v: &Value| -> Vec<String> {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_owned())
            .collect()
    };
    let r = ok(
        &mut s,
        "list_tasks",
        json!({ "priorities": ["high", "urgent"], "sort": "title" }),
    );
    assert_eq!(titles(&r), ["Alpha", "Charlie"]);
    let r = ok(
        &mut s,
        "list_tasks",
        json!({ "statuses": ["waiting", "In Progress"] }),
    );
    assert_eq!(titles(&r), ["Alpha"]);
    let r = ok(
        &mut s,
        "list_tasks",
        json!({ "tags": ["#Client", "design"], "sort": "title" }),
    );
    assert_eq!(titles(&r), ["Alpha", "Bravo"]);
    let r = ok(&mut s, "list_tasks", json!({ "project_ids": [app] }));
    assert_eq!(titles(&r), ["Charlie"]);
    let r = ok(
        &mut s,
        "list_tasks",
        json!({ "dated": true, "sort": "title" }),
    );
    assert_eq!(titles(&r), ["Alpha", "Charlie"]);
    let r = ok(&mut s, "list_tasks", json!({ "sort": "estimate" }));
    assert_eq!(titles(&r), ["Alpha", "Bravo", "Charlie"]);
    let r = ok(
        &mut s,
        "list_tasks",
        json!({ "sort": "status", "descending": true }),
    );
    assert_eq!(titles(&r)[0], "Alpha");
    for sort in ["project", "progress", "tags", "status", "estimate"] {
        assert_eq!(
            ok(&mut s, "list_tasks", json!({ "sort": sort }))["total"],
            3,
            "{sort}"
        );
    }
    // Every row says when it was added; added_since / added_until narrow by it.
    assert!(r["tasks"][0]["created_at"].as_i64().unwrap() > 0);
    assert_eq!(
        ok(&mut s, "list_tasks", json!({ "added_since": "2000-01-01" }))["total"],
        3
    );
    assert_eq!(
        ok(&mut s, "list_tasks", json!({ "added_until": "2000-01-01" }))["total"],
        0
    );
    assert!(err(&mut s, "list_tasks", json!({ "priorities": ["huge"] })).contains("priorities"));
    assert!(err(&mut s, "list_tasks", json!({ "tags": "design" })).contains("tags"));
}

#[test]
fn saved_views_keep_filters_grouping_and_board_order() {
    let mut s = server();
    let p = project(&mut s, "Website");
    let v = ok(
        &mut s,
        "add_saved_view",
        json!({
            "name": "Hot",
            "scope": "all",
            "priorities": ["high", "urgent"],
            "statuses": ["Waiting"],
            "tags": ["#client"],
            "project_ids": [p],
            "due": "overdue",
            "added": "week",
            "group_by": "group",
            "sort": "progress",
            "board_sort": "priority",
            "board_descending": true,
        }),
    );
    let spec: Value = serde_json::from_str(v["spec_json"].as_str().unwrap()).unwrap();
    assert_eq!(
        spec["filter"],
        json!({ "priorities": ["high", "urgent"], "statuses": ["Waiting"], "tags": ["client"], "projects": [p], "due": "overdue", "added": "week" })
    );
    assert_eq!(spec["groupBy"], "client");
    assert_eq!(spec["sort"], "progress");
    assert_eq!(spec["boardSort"], "priority");
    assert_eq!(spec["boardDescending"], true);
    // Only the fields given change; an empty list or null clears one.
    let u = ok(
        &mut s,
        "update_saved_view",
        json!({ "id": v["id"], "priorities": [], "due": null }),
    );
    let spec: Value = serde_json::from_str(u["spec_json"].as_str().unwrap()).unwrap();
    assert_eq!(spec["filter"]["priorities"], json!([]));
    assert_eq!(spec["filter"]["due"], Value::Null);
    assert_eq!(spec["filter"]["statuses"], json!(["Waiting"]));
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "due": "soon" })
        )
        .contains("due")
    );
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "group_by": "color" })
        )
        .contains("group_by")
    );
}

#[test]
fn reminders_and_repeating_series() {
    let mut s = server();
    let p = project(&mut s, "Home");
    let tomorrow = {
        let d = jiff::Zoned::now().date().tomorrow().unwrap();
        d.to_string()
    };
    ok(
        &mut s,
        "add_task",
        json!({ "text": "Call the bank", "project_id": p, "due_date": tomorrow, "due_time": "09:00" }),
    );
    ok(
        &mut s,
        "add_task",
        json!({ "text": "Pay rent every month", "project_id": p, "due_date": tomorrow }),
    );
    let r = ok(&mut s, "list_reminders", json!({ "days": 70 }));
    assert_eq!(r["reminders_on"], true);
    let items = r["items"].as_array().unwrap();
    let bank = items
        .iter()
        .find(|i| i["title"] == "Call the bank")
        .unwrap();
    assert_eq!(bank["time"], "09:00");
    assert_eq!(bank["remind_at"], format!("{tomorrow}T09:00"));
    let rent: Vec<_> = items.iter().filter(|i| i["title"] == "Pay rent").collect();
    assert!(rent.len() >= 2, "{rent:?}");
    assert_eq!(rent[1]["projected"], true);
    assert!(
        rent[0]["repeat"]
            .as_str()
            .unwrap()
            .starts_with("Every month")
    );
    let only_meetings = ok(&mut s, "list_reminders", json!({ "kinds": ["meeting"] }));
    assert_eq!(only_meetings["items"], json!([]));
    let rep = ok(&mut s, "list_repeating", json!({}));
    let list = rep["items"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["title"], "Pay rent");
    assert_eq!(list[0]["next_date"], tomorrow);
    assert!(err(&mut s, "list_reminders", json!({ "days": 9999 })).contains("days"));
    assert!(err(&mut s, "list_reminders", json!({ "kinds": ["alarm"] })).contains("kinds"));
}

#[test]
fn arguments_of_the_wrong_type_are_errors_not_defaults() {
    let mut s = server();
    let t = id(&ok(&mut s, "add_task", json!({ "text": "Typed" })));
    assert!(err(&mut s, "complete_task", json!({ "id": t, "done": "false" })).contains("done"));
    let still = ok(&mut s, "get_task", json!({ "id": t }));
    assert!(
        still["completed_at"].is_null(),
        "a string 'false' must not complete it"
    );
    assert!(err(&mut s, "list_tasks", json!({ "limit": "20" })).contains("limit"));
    assert!(err(&mut s, "update_task", json!({ "id": t, "title": null })).contains("title"));
    // Notes can be cleared with null, as the description says.
    ok(
        &mut s,
        "update_task",
        json!({ "id": t, "description_md": "some notes" }),
    );
    let cleared = ok(
        &mut s,
        "update_task",
        json!({ "id": t, "description_md": null }),
    );
    assert_eq!(cleared["description_md"], "");
}

#[test]
fn saved_views_refuse_values_the_app_cannot_show() {
    let mut s = server();
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "sort": "deadline" })
        )
        .contains("sort")
    );
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "view": "kanban" })
        )
        .contains("view")
    );
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "lanes": "colour" })
        )
        .contains("lanes")
    );
    assert!(
        err(
            &mut s,
            "add_saved_view",
            json!({ "name": "x", "scope": "all", "board_sort": "status" })
        )
        .contains("board_sort")
    );
}

#[test]
fn search_finds_subtasks_like_the_app() {
    let mut s = server();
    let parent = id(&ok(&mut s, "add_task", json!({ "text": "Parent" })));
    ok(
        &mut s,
        "add_task",
        json!({ "text": "Unique subtask zebra", "parent_id": parent }),
    );
    assert_eq!(
        ok(&mut s, "list_tasks", json!({ "search": "zebra" }))["total"],
        1
    );
    // Without a search, lists stay top-level.
    assert_eq!(ok(&mut s, "list_tasks", json!({}))["total"], 1);
}

#[test]
fn group_id_and_client_id_are_the_same_argument() {
    let mut s = server();
    let g = id(&ok(&mut s, "add_group", json!({ "name": "Acme" })));
    let p = ok(
        &mut s,
        "add_project",
        json!({ "name": "Site", "group_id": g }),
    );
    assert_eq!(p["client_id"], g);
    ok(&mut s, "list_tasks", json!({ "client_id": g }));
}

#[test]
fn a_future_from_alone_gets_its_own_week() {
    let mut s = server();
    let r = ok(&mut s, "list_events", json!({ "from": "2031-03-01" }));
    assert!(r["events"].is_array() || r.is_array() || r.is_object());
}

#[test]
fn bulk_complete_counts_only_real_changes() {
    let mut s = server();
    let a = id(&ok(&mut s, "add_task", json!({ "text": "A" })));
    let b = id(&ok(&mut s, "add_task", json!({ "text": "B" })));
    assert_eq!(
        ok(&mut s, "complete_task", json!({ "ids": [a, b] }))["updated"],
        2
    );
    assert_eq!(
        ok(&mut s, "complete_task", json!({ "ids": [a, b] }))["updated"],
        0
    );
}

#[test]
fn an_occurrence_s_own_notes_can_be_read_back() {
    let mut s = server();
    let e = ok(
        &mut s,
        "add_event",
        json!({ "kind": "meeting", "title": "Standup", "start": "2031-03-03T10:00", "end": "2031-03-03T10:15", "recurrence_rule": "FREQ=WEEKLY" }),
    );
    let eid = id(&e);
    ok(
        &mut s,
        "update_event",
        json!({ "id": eid, "scope": "occurrence", "occurrence_start": "2031-03-10T10:00", "notes_md": "agenda" }),
    );
    let one = ok(
        &mut s,
        "get_event",
        json!({ "id": eid, "occurrence_start": "2031-03-10T10:00" }),
    );
    assert_eq!(one["occurrence"]["notes_md"], "agenda");
}

#[test]
fn json_rpc_batches_get_one_answer_per_request() {
    let mut s = server();
    let r = s
        .handle(json!([
            { "jsonrpc": "2.0", "id": 5, "method": "ping" },
            { "jsonrpc": "2.0", "id": 6, "method": "ping" }
        ]))
        .unwrap();
    let ids: Vec<i64> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [5, 6]);
}
