#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use proptest::prelude::*;
use rusqlite::Connection;
use taskboard_core::*;

fn db() -> Db {
    Db::open_in_memory().unwrap()
}

fn client(db: &mut Db, name: &str) -> Client {
    db.create_client(NewClient {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
}

fn project(db: &mut Db, name: &str, client: Option<Id>) -> Project {
    db.create_project(NewProject {
        name: name.into(),
        client_id: client,
        ..Default::default()
    })
    .unwrap()
}

fn task(db: &mut Db, title: &str, project: Option<Id>) -> Task {
    db.create_task(NewTask {
        title: title.into(),
        project_id: project,
        ..Default::default()
    })
    .unwrap()
}

fn titles(page: &TaskPage) -> Vec<&str> {
    page.rows.iter().map(|r| r.title.as_str()).collect()
}

// ---------- structure ----------

#[test]
fn inbox_is_created_once_with_default_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let first = Db::open(&path).unwrap().inbox_id().unwrap();
    let db = Db::open(&path).unwrap();
    assert_eq!(db.inbox_id().unwrap(), first);
    let names: Vec<String> = db
        .statuses(first)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
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
    // Seeding is not an undoable user action.
    assert_eq!(db.history_state().unwrap().undo, None);
}

#[test]
fn tasks_default_to_inbox_and_to_do_and_append_in_order() {
    let mut db = db();
    let a = task(&mut db, "first", None);
    let b = task(&mut db, "second", None);
    assert_eq!(a.project_id, db.inbox_id().unwrap());
    let status = db.statuses(a.project_id).unwrap();
    assert_eq!(
        status.iter().find(|s| s.id == a.status_id).unwrap().name,
        "To Do"
    );
    assert!(a.sort_key < b.sort_key);
}

#[test]
fn client_project_hierarchy_and_sidebar_order() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let north = client(&mut db, "Northwind");
    project(&mut db, "Site", Some(acme.id));
    project(&mut db, "App", Some(north.id));
    project(&mut db, "Portfolio", None);
    let names: Vec<String> = db
        .projects(false)
        .unwrap()
        .into_iter()
        .map(|p| p.project.name)
        .collect();
    assert_eq!(names, ["Inbox", "Site", "App", "Portfolio"]);
    assert_ne!(acme.color, north.color, "palette assigns different colors");
}

#[test]
fn archiving_a_client_archives_and_restores_exactly_its_projects() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let live = project(&mut db, "Live", Some(acme.id));
    let old = project(&mut db, "Old", Some(acme.id));
    db.set_project_archived(old.id, true).unwrap();

    db.set_client_archived(acme.id, true).unwrap();
    assert!(db.project(live.id).unwrap().archived_at.is_some());

    db.set_client_archived(acme.id, false).unwrap();
    assert!(db.project(live.id).unwrap().archived_at.is_none());
    assert!(
        db.project(old.id).unwrap().archived_at.is_some(),
        "stays archived"
    );
}

#[test]
fn client_with_projects_cannot_be_deleted() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let p = project(&mut db, "Site", Some(acme.id));
    assert_eq!(db.delete_client(acme.id).unwrap_err().code(), "conflict");
    db.delete_project(p.id).unwrap();
    db.delete_client(acme.id).unwrap();
    assert_eq!(db.client(acme.id).unwrap_err().code(), "not_found");
}

#[test]
fn inbox_is_protected() {
    let mut db = db();
    let inbox = db.inbox_id().unwrap();
    assert!(db.delete_project(inbox).is_err());
    assert!(db.set_project_archived(inbox, true).is_err());
    let acme = client(&mut db, "Acme");
    let patch = ProjectPatch {
        client_id: Some(Some(acme.id)),
        ..Default::default()
    };
    assert!(db.update_project(inbox, patch).is_err());
}

// ---------- tasks ----------

#[test]
fn completing_sets_and_reopening_clears_completed_at() {
    let mut db = db();
    let t = task(&mut db, "ship", None);
    let done = db.set_task_done(t.id, true).unwrap();
    assert!(done.completed_at.is_some());
    let statuses = db.statuses(t.project_id).unwrap();
    assert_eq!(
        statuses
            .iter()
            .find(|s| s.id == done.status_id)
            .unwrap()
            .name,
        "Done"
    );
    let open = db.set_task_done(t.id, false).unwrap();
    assert!(open.completed_at.is_none());
    assert_eq!(open.status_id, t.status_id);
}

#[test]
fn moving_to_a_done_column_completes_the_task() {
    let mut db = db();
    let t = task(&mut db, "ship", None);
    let done = db
        .statuses(t.project_id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Done")
        .unwrap();
    let moved = db.move_task(
        t.id,
        MoveTask {
            status_id: Some(done.id),
            ..Default::default()
        },
    );
    assert!(moved.unwrap().completed_at.is_some());
}

#[test]
fn date_rules() {
    let mut db = db();
    let bad = NewTask {
        title: "x".into(),
        due_time: Some("10:00".into()),
        ..Default::default()
    };
    assert_eq!(db.create_task(bad).unwrap_err().code(), "invalid_input");
    let t = db
        .create_task(NewTask {
            title: "x".into(),
            due_date: Some("2026-10-20".into()),
            due_time: Some("10:00".into()),
            ..Default::default()
        })
        .unwrap();
    // Clearing the due date clears the time.
    let t = db
        .update_task(
            t.id,
            TaskPatch {
                due_date: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!((t.due_date, t.due_time), (None, None));
    let backwards = TaskPatch {
        start_date: Some(Some("2026-10-21".into())),
        due_date: Some(Some("2026-10-20".into())),
        ..Default::default()
    };
    assert!(db.update_task(t.id, backwards).is_err());
}

#[test]
fn subtasks_reject_cycles_and_cross_project_parents() {
    let mut db = db();
    let a = task(&mut db, "a", None);
    let b = db
        .create_task(NewTask {
            title: "b".into(),
            parent_id: Some(a.id),
            ..Default::default()
        })
        .unwrap();
    let c = db
        .create_task(NewTask {
            title: "c".into(),
            parent_id: Some(b.id),
            ..Default::default()
        })
        .unwrap();
    let cycle = TaskPatch {
        parent_id: Some(Some(c.id)),
        ..Default::default()
    };
    assert_eq!(
        db.update_task(a.id, cycle).unwrap_err().code(),
        "invalid_input"
    );

    let other = project(&mut db, "Other", None);
    let elsewhere = task(&mut db, "elsewhere", Some(other.id));
    let cross = TaskPatch {
        parent_id: Some(Some(elsewhere.id)),
        ..Default::default()
    };
    assert!(db.update_task(a.id, cross).is_err());
}

#[test]
fn delete_and_restore_cascade_to_subtasks() {
    let mut db = db();
    let parent = task(&mut db, "parent", None);
    let child = db
        .create_task(NewTask {
            title: "child".into(),
            parent_id: Some(parent.id),
            ..Default::default()
        })
        .unwrap();
    db.delete_task(parent.id).unwrap();
    assert!(db.task(child.id).is_err());
    db.restore_task(parent.id).unwrap();
    assert!(db.task(child.id).is_ok());
}

#[test]
fn moving_to_another_project_maps_status_and_carries_subtasks() {
    let mut db = db();
    let src = project(&mut db, "Src", None);
    let dst = project(&mut db, "Dst", None);
    let parent = task(&mut db, "parent", Some(src.id));
    let child = db
        .create_task(NewTask {
            title: "child".into(),
            parent_id: Some(parent.id),
            ..Default::default()
        })
        .unwrap();
    db.set_task_done(child.id, true).unwrap();

    db.update_task(
        parent.id,
        TaskPatch {
            project_id: Some(dst.id),
            ..Default::default()
        },
    )
    .unwrap();
    let dst_statuses = db.statuses(dst.id).unwrap();
    let parent = db.task(parent.id).unwrap();
    let child = db.task(child.id).unwrap();
    assert_eq!((parent.project_id, child.project_id), (dst.id, dst.id));
    assert!(
        dst_statuses
            .iter()
            .any(|s| s.id == parent.status_id && s.name == "To Do")
    );
    assert!(
        dst_statuses
            .iter()
            .any(|s| s.id == child.status_id && s.name == "Done")
    );
    assert_eq!(child.parent_id, Some(parent.id));
}

#[test]
fn reorder_between_neighbours() {
    let mut db = db();
    let a = task(&mut db, "a", None);
    let b = task(&mut db, "b", None);
    let c = task(&mut db, "c", None);
    db.move_task(
        c.id,
        MoveTask {
            above: Some(a.id),
            below: Some(b.id),
            ..Default::default()
        },
    )
    .unwrap();
    let page = db.list_tasks(&TaskQuery::default()).unwrap();
    assert_eq!(titles(&page), ["a", "c", "b"]);
}

#[test]
fn tags_are_normalized_and_searchable() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "Write report".into(),
            tags: vec!["#Work".into(), "work".into(), " urgent ".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(t.tags, ["urgent", "Work"]);
    let by_tag = db.list_tasks(&TaskQuery {
        tag: Some("#work".into()),
        ..Default::default()
    });
    assert_eq!(by_tag.unwrap().total, 1);
    let by_text = db.list_tasks(&TaskQuery {
        search: Some("urg".into()),
        ..Default::default()
    });
    assert_eq!(by_text.unwrap().total, 1, "tags are in the search index");
}

#[test]
fn search_finds_prefixes_and_ignores_deleted() {
    let mut db = db();
    let t = task(&mut db, "Quarterly report for Acme", None);
    task(&mut db, "Unrelated", None);
    let q = TaskQuery {
        search: Some("quart acm".into()),
        ..Default::default()
    };
    assert_eq!(
        titles(&db.list_tasks(&q).unwrap()),
        ["Quarterly report for Acme"]
    );
    db.delete_task(t.id).unwrap();
    assert_eq!(db.list_tasks(&q).unwrap().total, 0);
    db.undo().unwrap();
    assert_eq!(
        db.list_tasks(&q).unwrap().total,
        1,
        "undo restores the index entry"
    );
    db.update_task(
        t.id,
        TaskPatch {
            title: Some("Annual summary".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        db.list_tasks(&q).unwrap().total,
        0,
        "renames update the index"
    );
}

#[test]
fn queries_filter_by_client_personal_category_and_paginate() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let site = project(&mut db, "Site", Some(acme.id));
    let mine = project(&mut db, "Mine", None);
    for i in 0..5 {
        task(&mut db, &format!("acme {i}"), Some(site.id));
    }
    let done = task(&mut db, "personal", Some(mine.id));
    db.set_task_done(done.id, true).unwrap();
    task(&mut db, "inbox", None);

    let q = |q: TaskQuery| db.list_tasks(&q).unwrap();
    assert_eq!(
        q(TaskQuery {
            client_id: Some(acme.id),
            ..Default::default()
        })
        .total,
        5
    );
    assert_eq!(
        titles(&q(TaskQuery {
            section_id: db.project(mine.id).unwrap().section_id,
            ..Default::default()
        })),
        ["acme 0", "acme 1", "acme 2", "acme 3", "acme 4", "personal"],
        "one default section holds the group's and the loose project's tasks"
    );
    let open = q(TaskQuery {
        categories: vec![Category::Todo, Category::Active],
        ..Default::default()
    });
    assert_eq!(open.total, 6);
    let page = q(TaskQuery {
        limit: 2,
        offset: 2,
        ..Default::default()
    });
    assert_eq!((page.rows.len(), page.total), (2, 7));
    let row = &q(TaskQuery {
        client_id: Some(acme.id),
        limit: 1,
        ..Default::default()
    })
    .rows[0];
    assert_eq!(row.client_name.as_deref(), Some("Acme"));
    assert_eq!(row.project_name, "Site");
}

#[test]
fn sort_by_due_puts_undated_last() {
    let mut db = db();
    for (title, due) in [
        ("none", None),
        ("late", Some("2026-12-01")),
        ("soon", Some("2026-10-20")),
    ] {
        db.create_task(NewTask {
            title: title.into(),
            due_date: due.map(Into::into),
            ..Default::default()
        })
        .unwrap();
    }
    let page = db
        .list_tasks(&TaskQuery {
            sort: TaskSort::Due,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(titles(&page), ["soon", "late", "none"]);
}

// ---------- undo / redo ----------

#[test]
fn undo_redo_create_edit_delete() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "a".into(),
            tags: vec!["x".into()],
            ..Default::default()
        })
        .unwrap();
    db.update_task(
        t.id,
        TaskPatch {
            title: Some("b".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Edit task “a”")
    );

    db.undo().unwrap();
    assert_eq!(db.task(t.id).unwrap().title, "a");
    db.undo().unwrap();
    assert!(db.task(t.id).is_err());
    assert_eq!(db.undo().unwrap_err().code(), "nothing_to_do");

    db.redo().unwrap();
    let back = db.task(t.id).unwrap();
    assert_eq!(
        (back.title.as_str(), back.tags.clone()),
        ("a", vec!["x".to_owned()])
    );
    db.redo().unwrap();
    assert_eq!(db.task(t.id).unwrap().title, "b");
}

#[test]
fn a_new_action_clears_redo() {
    let mut db = db();
    task(&mut db, "a", None);
    db.undo().unwrap();
    assert!(db.history_state().unwrap().redo.is_some());
    task(&mut db, "b", None);
    assert!(db.history_state().unwrap().redo.is_none());
}

#[test]
fn deleting_a_project_is_one_undo() {
    let mut db = db();
    let p = project(&mut db, "Site", None);
    let t = task(&mut db, "t", Some(p.id));
    db.delete_project(p.id).unwrap();
    assert!(db.task(t.id).is_err());
    db.undo().unwrap();
    assert!(db.project(p.id).is_ok() && db.task(t.id).is_ok());
}

#[test]
fn no_op_edits_are_not_recorded() {
    let mut db = db();
    let t = task(&mut db, "same", None);
    db.update_task(
        t.id,
        TaskPatch {
            title: Some("same".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Create task “same”")
    );
}

// ---------- migrations ----------

#[test]
fn upgrading_an_existing_database_backs_it_up_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    {
        // A schema-v1 database, as created by the M0 build.
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let db = Db::open(&path).unwrap();
    assert_eq!(
        db.info().unwrap().schema_version as usize,
        storage::SCHEMA_VERSION
    );
    let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .collect();
    assert_eq!(backups.len(), 1);
}

// ---------- property: undo everything returns to the start ----------

#[derive(Debug, Clone)]
enum Op {
    Create(u8),
    Rename(usize, u8),
    Done(usize, bool),
    Delete(usize),
    Tag(usize, u8),
    Nest(usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<u8>().prop_map(Op::Create),
        (any::<usize>(), any::<u8>()).prop_map(|(i, n)| Op::Rename(i, n)),
        (any::<usize>(), any::<bool>()).prop_map(|(i, d)| Op::Done(i, d)),
        any::<usize>().prop_map(Op::Delete),
        (any::<usize>(), any::<u8>()).prop_map(|(i, n)| Op::Tag(i, n)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Nest(a, b)),
    ]
}

/// Every user-visible table, minus sync stamps (which legitimately change).
fn dump(db: &Db) -> String {
    let page = db
        .list_tasks(&TaskQuery {
            include_archived: true,
            limit: 1000,
            ..Default::default()
        })
        .unwrap();
    let mut out: Vec<String> = page
        .rows
        .iter()
        .map(|r| {
            format!(
                "{} {} {:?} {} {:?} {:?} {}",
                r.id,
                r.title,
                r.category,
                r.sort_key,
                r.parent_id,
                r.tags,
                r.completed_at.is_some()
            )
        })
        .collect();
    out.sort();
    out.join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn undo_all_then_redo_all_round_trips(ops in prop::collection::vec(op(), 1..25)) {
        let mut db = db();
        let start = dump(&db);
        let mut ids: Vec<Id> = Vec::new();
        for op in ops {
            let pick = |i: usize| (!ids.is_empty()).then(|| ids[i % ids.len()]);
            // Invalid operations (e.g. on deleted tasks, cycles) may fail; that's fine.
            let _ = match op {
                Op::Create(n) => db
                    .create_task(NewTask { title: format!("t{n}"), ..Default::default() })
                    .map(|t| ids.push(t.id)),
                Op::Rename(i, n) => match pick(i) {
                    Some(id) => db
                        .update_task(id, TaskPatch { title: Some(format!("r{n}")), ..Default::default() })
                        .map(drop),
                    None => Ok(()),
                },
                Op::Done(i, d) => match pick(i) {
                    Some(id) => db.set_task_done(id, d).map(drop),
                    None => Ok(()),
                },
                Op::Delete(i) => match pick(i) {
                    Some(id) => db.delete_task(id),
                    None => Ok(()),
                },
                Op::Tag(i, n) => match pick(i) {
                    Some(id) => db
                        .update_task(id, TaskPatch { tags: Some(vec![format!("g{}", n % 4)]), ..Default::default() })
                        .map(drop),
                    None => Ok(()),
                },
                Op::Nest(a, b) => match (pick(a), pick(b)) {
                    (Some(a), Some(b)) => db
                        .update_task(a, TaskPatch { parent_id: Some(Some(b)), ..Default::default() })
                        .map(drop),
                    _ => Ok(()),
                },
            };
        }
        let end = dump(&db);

        while db.undo().is_ok() {}
        prop_assert_eq!(dump(&db), start);
        while db.redo().is_ok() {}
        prop_assert_eq!(dump(&db), end);
    }
}

// ---------- quick add ----------

#[test]
fn quick_add_resolves_projects_by_name_and_client() {
    let mut db = db();
    let acme = client(&mut db, "Acme Corp");
    let site = project(&mut db, "Website redesign", Some(acme.id));
    let north = client(&mut db, "Northwind");
    let app = project(&mut db, "Website", Some(north.id));
    let today = jiff::civil::date(2026, 10, 15);

    let t = db
        .quick_add(
            "Wireframes tomorrow 3pm @website-re #design !high ~2h",
            today,
        )
        .unwrap();
    assert_eq!(t.project_id, site.id, "prefix match");
    assert_eq!(t.title, "Wireframes");
    assert_eq!(
        (t.due_date.as_deref(), t.due_time.as_deref()),
        (Some("2026-10-16"), Some("15:00"))
    );
    assert_eq!(
        (t.priority, t.estimate_minutes, t.tags.clone()),
        (Priority::High, Some(120), vec!["design".to_owned()])
    );

    let t = db.quick_add("Deploy @website", today).unwrap();
    assert_eq!(t.project_id, app.id, "exact name beats prefix");
    let t = db
        .quick_add("Audit @\"acme corp/website redesign\"", today)
        .unwrap();
    assert_eq!(t.project_id, site.id, "client/project form");

    assert_eq!(
        db.quick_add("x @nowhere", today).unwrap_err().code(),
        "invalid_input"
    );
    assert_eq!(
        db.quick_add("#only-a-tag", today).unwrap_err().code(),
        "invalid_input"
    );
    assert_eq!(
        db.quick_add("Inbox item", today).unwrap().project_id,
        db.inbox_id().unwrap()
    );
}

#[test]
fn archived_projects_are_hidden_from_lists_unless_asked() {
    let mut db = db();
    let old = project(&mut db, "Old", None);
    task(&mut db, "hidden", Some(old.id));
    task(&mut db, "visible", None);
    db.set_project_archived(old.id, true).unwrap();
    assert_eq!(
        titles(&db.list_tasks(&TaskQuery::default()).unwrap()),
        ["visible"]
    );
    let all = TaskQuery {
        include_archived: true,
        ..Default::default()
    };
    assert_eq!(db.list_tasks(&all).unwrap().total, 2);
    let open = TaskQuery {
        categories: vec![Category::Todo, Category::Active],
        ..Default::default()
    };
    assert_eq!(db.list_tasks(&open).unwrap().total, 1);
}

#[test]
fn sidebar_counts_and_quick_add_preview() {
    let mut db = db();
    let today = jiff::civil::date(2026, 10, 15);
    db.quick_add("overdue yesterday", today).ok();
    db.quick_add("overdue 2026-10-14", today).unwrap();
    db.quick_add("now today", today).unwrap();
    db.quick_add("soon in 3 days", today).unwrap();
    db.quick_add("later in 3 weeks", today).unwrap();
    db.quick_add("undated", today).unwrap();
    let c = db.sidebar_counts(today).unwrap();
    assert_eq!((c.today, c.overdue, c.upcoming), (3, 2, 1));
    assert_eq!(c.inbox, 6);

    let acme = client(&mut db, "Acme");
    project(&mut db, "Website", Some(acme.id));
    let p = db.preview_quick_add("Fix nav @web !high", today).unwrap();
    let project = p.project.unwrap();
    assert_eq!(
        (project.name.as_str(), project.client_name.as_deref()),
        ("Website", Some("Acme"))
    );
    assert!(!p.project_missing);
    assert!(
        db.preview_quick_add("x @nope", today)
            .unwrap()
            .project_missing
    );
}

#[test]
fn date_filters_handle_undated_tasks() {
    let mut db = db();
    let today = jiff::civil::date(2026, 10, 15);
    db.quick_add("past 2026-10-01", today).unwrap();
    db.quick_add("future 2026-12-01", today).unwrap();
    db.quick_add("undated", today).unwrap();
    let count = |db: &Db, q: TaskQuery| db.list_tasks(&q).unwrap().total;
    let from = |d: &str| TaskQuery {
        due_from: Some(d.into()),
        ..Default::default()
    };
    // Open-ended "from" must not pick up undated tasks.
    assert_eq!(count(&db, from("2026-10-15")), 1);
    assert_eq!(count(&db, from("2000-01-01")), 2);
    let to = TaskQuery {
        due_to: Some("2099-01-01".into()),
        ..Default::default()
    };
    assert_eq!(count(&db, to), 2);
    let undated = TaskQuery {
        undated: true,
        ..Default::default()
    };
    let page = db.list_tasks(&undated).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].title, "undated");
}

#[test]
fn project_counts_skip_subtasks_and_deleted_tasks() {
    let mut db = db();
    let web = project(&mut db, "Website", None);
    let a = task(&mut db, "a", Some(web.id));
    let b = task(&mut db, "b", Some(web.id));
    let gone = task(&mut db, "gone", Some(web.id));
    db.create_task(NewTask {
        title: "sub".into(),
        project_id: Some(web.id),
        parent_id: Some(a.id),
        ..Default::default()
    })
    .unwrap();
    db.set_task_done(b.id, true).unwrap();
    db.delete_task(gone.id).unwrap();
    project(&mut db, "Empty", None);

    let projects = db.projects(false).unwrap();
    let find = |name: &str| projects.iter().find(|p| p.project.name == name).unwrap();
    let w = find("Website");
    assert_eq!((w.open_tasks, w.total_tasks, w.done_tasks), (1, 2, 1));
    let e = find("Empty");
    assert_eq!((e.open_tasks, e.total_tasks, e.done_tasks), (0, 0, 0));
}

#[test]
fn summary_counts_done_and_estimates_over_the_whole_query() {
    let mut db = db();
    let today = jiff::civil::date(2026, 10, 15);
    let a = db.quick_add("a ~30m", today).unwrap();
    db.quick_add("b ~1h", today).unwrap();
    db.quick_add("c", today).unwrap();
    db.set_task_done(a.id, true).unwrap();
    let s = db.summarize_tasks(&TaskQuery::default()).unwrap();
    assert_eq!((s.total, s.done, s.estimate_minutes), (3, 1, 90));
}

// ---------- board columns (statuses) ----------

#[test]
fn columns_can_be_added_renamed_moved_and_must_be_unique() {
    let mut db = db();
    let p = project(&mut db, "Site", None);
    let cols = db.statuses(p.id).unwrap();
    let review = cols.iter().find(|s| s.name == "Review").unwrap().id;
    let qa = db
        .create_status(NewStatus {
            project_id: p.id,
            name: "QA".into(),
            category: Category::Active,
            color: None,
            after: Some(review),
        })
        .unwrap();
    let names = |db: &Db| {
        db.statuses(p.id)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&db),
        [
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "QA",
            "Done"
        ]
    );

    let dup = NewStatus {
        project_id: p.id,
        name: "qa".into(),
        category: Category::Todo,
        color: None,
        after: None,
    };
    assert_eq!(db.create_status(dup).unwrap_err().code(), "invalid_input");

    db.move_status(qa.id, None).unwrap();
    assert_eq!(names(&db)[0], "QA");
    db.update_status(
        qa.id,
        StatusPatch {
            name: Some("Testing".into()),
            wip_limit: Some(Some(3)),
            ..Default::default()
        },
    )
    .unwrap();
    let testing = db.status(qa.id).unwrap();
    assert_eq!(
        (testing.name.as_str(), testing.wip_limit),
        ("Testing", Some(3))
    );
    db.undo().unwrap();
    assert_eq!(db.status(qa.id).unwrap().name, "QA");
}

#[test]
fn changing_a_column_category_syncs_completion() {
    let mut db = db();
    let p = project(&mut db, "Site", None);
    let t = task(&mut db, "t", Some(p.id));
    let review = db
        .statuses(p.id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Review")
        .unwrap();
    db.update_task(
        t.id,
        TaskPatch {
            status_id: Some(review.id),
            ..Default::default()
        },
    )
    .unwrap();
    db.update_status(
        review.id,
        StatusPatch {
            category: Some(Category::Done),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(db.task(t.id).unwrap().completed_at.is_some());
    db.update_status(
        review.id,
        StatusPatch {
            category: Some(Category::Active),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(db.task(t.id).unwrap().completed_at.is_none());
}

#[test]
fn deleting_a_column_moves_its_tasks_and_keeps_required_columns() {
    let mut db = db();
    let p = project(&mut db, "Site", None);
    let cols = db.statuses(p.id).unwrap();
    let id = |name: &str| cols.iter().find(|s| s.name == name).unwrap().id;
    let t = task(&mut db, "t", Some(p.id));
    db.delete_status(id("To Do"), id("Done")).unwrap();
    let moved = db.task(t.id).unwrap();
    assert_eq!(moved.status_id, id("Done"));
    assert!(moved.completed_at.is_some(), "moved into a closed column");
    assert_eq!(
        db.delete_status(id("Done"), id("Backlog"))
            .unwrap_err()
            .code(),
        "invalid_input"
    );
    db.undo().unwrap();
    assert_eq!(db.task(t.id).unwrap().status_id, id("To Do"));
}

#[test]
fn status_name_filter_merges_columns_across_projects() {
    let mut db = db();
    let a = project(&mut db, "A", None);
    let b = project(&mut db, "B", None);
    let ta = task(&mut db, "a", Some(a.id));
    task(&mut db, "b", Some(b.id));
    let doing = db
        .statuses(a.id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "In Progress")
        .unwrap();
    db.update_task(
        ta.id,
        TaskPatch {
            status_id: Some(doing.id),
            ..Default::default()
        },
    )
    .unwrap();
    let q = |name: &str| TaskQuery {
        status_name: Some(name.into()),
        ..Default::default()
    };
    assert_eq!(db.list_tasks(&q("to do")).unwrap().total, 2 - 1);
    assert_eq!(titles(&db.list_tasks(&q("In Progress")).unwrap()), ["a"]);
    assert!(db.all_statuses().unwrap().len() >= 15);
}

#[test]
fn merged_columns_get_distinct_keys_and_survive_equal_neighbours() {
    let mut db = db();
    let a = project(&mut db, "A", None);
    let b = project(&mut db, "B", None);
    let ta = task(&mut db, "a", Some(a.id));
    let tb = task(&mut db, "b", Some(b.id));
    assert_ne!(
        ta.sort_key, tb.sort_key,
        "keys are distinct across a merged column"
    );

    // Force a tie (as older data could have) and drop a card between them.
    let tc = task(&mut db, "c", Some(a.id));
    db.move_task(
        tb.id,
        MoveTask {
            above: Some(ta.id),
            ..Default::default()
        },
    )
    .unwrap();
    let tie = db.task(ta.id).unwrap().sort_key;
    let q = TaskQuery {
        status_name: Some("To Do".into()),
        sort: TaskSort::Manual,
        ..Default::default()
    };
    db.move_task(
        tc.id,
        MoveTask {
            above: Some(ta.id),
            below: Some(tb.id),
            ..Default::default()
        },
    )
    .unwrap();
    let order = db.list_tasks(&q).unwrap();
    assert!(order.rows.iter().all(|r| !r.sort_key.is_empty()));
    let c_key = db.task(tc.id).unwrap().sort_key;
    assert!(c_key > tie);
}

// ---------- progress ----------

#[test]
fn progress_is_manual_without_subtasks_and_derived_with_them() {
    let mut db = db();
    let row = |db: &Db, id: Id| {
        db.list_tasks(&TaskQuery {
            include_archived: true,
            ..Default::default()
        })
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.id == id)
        .unwrap()
    };
    let solo = task(&mut db, "solo", None);
    assert_eq!(row(&db, solo.id).progress, 0);
    db.update_task(
        solo.id,
        TaskPatch {
            progress: Some(Some(40)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(row(&db, solo.id).progress, 40);
    assert!(
        db.update_task(
            solo.id,
            TaskPatch {
                progress: Some(Some(140)),
                ..Default::default()
            }
        )
        .is_err()
    );
    db.set_task_done(solo.id, true).unwrap();
    assert_eq!(row(&db, solo.id).progress, 100, "closed tasks are 100%");

    let parent = task(&mut db, "parent", None);
    db.update_task(
        parent.id,
        TaskPatch {
            progress: Some(Some(90)),
            ..Default::default()
        },
    )
    .unwrap();
    let sub = |db: &mut Db, title: &str| {
        db.create_task(NewTask {
            title: title.into(),
            parent_id: Some(parent.id),
            ..Default::default()
        })
        .unwrap()
    };
    let a = sub(&mut db, "a");
    let b = sub(&mut db, "b");
    sub(&mut db, "c");
    sub(&mut db, "d");
    db.set_task_done(a.id, true).unwrap();
    db.update_task(
        b.id,
        TaskPatch {
            progress: Some(Some(50)),
            ..Default::default()
        },
    )
    .unwrap();
    // (100 + 50 + 0 + 0) / 4, and the parent's own 90% is ignored once it has subtasks.
    assert_eq!(row(&db, parent.id).progress, 38);
}

// ---------- repeating tasks ----------

#[test]
fn completing_a_repeating_task_creates_the_next_one() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "Send invoices".into(),
            due_date: Some("2030-01-07".into()), // a Monday, safely in the future
            recurrence_rule: Some("FREQ=WEEKLY;BYDAY=MO".into()),
            tags: vec!["admin".into()],
            ..Default::default()
        })
        .unwrap();
    db.set_task_done(t.id, true).unwrap();
    let open = db
        .list_tasks(&TaskQuery {
            search: Some("invoices".into()),
            categories: vec![Category::Todo],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(open.total, 1);
    let next = db.task(open.rows[0].id).unwrap();
    assert_eq!(next.due_date.as_deref(), Some("2030-01-14"));
    assert_eq!(next.tags, ["admin"]);
    assert_eq!(next.series_id, db.task(t.id).unwrap().series_id);
    assert!(open.rows[0].recurring);

    // Reopen and complete again: no duplicate.
    db.set_task_done(t.id, false).unwrap();
    db.set_task_done(t.id, true).unwrap();
    let count = |db: &Db| {
        db.list_tasks(&TaskQuery {
            search: Some("invoices".into()),
            categories: vec![Category::Todo, Category::Active],
            ..Default::default()
        })
        .unwrap()
        .total
    };
    assert_eq!(count(&db), 1, "only the one open instance");

    // One undo removes the completion and the spawned instance together.
    let mut db2 = self::db();
    let t2 = db2
        .create_task(NewTask {
            title: "Backup".into(),
            recurrence_rule: Some("AFTER:3D".into()),
            ..Default::default()
        })
        .unwrap();
    db2.set_task_done(t2.id, true).unwrap();
    assert_eq!(
        db2.list_tasks(&TaskQuery {
            categories: vec![Category::Todo],
            ..Default::default()
        })
        .unwrap()
        .total,
        1
    );
    db2.undo().unwrap();
    let all = db2.list_tasks(&TaskQuery::default()).unwrap();
    assert_eq!((all.total, all.rows[0].completed_at), (1, None));
}

#[test]
fn moving_a_repeating_card_to_done_also_repeats() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "Weekly review".into(),
            due_date: Some("2030-01-11".into()),
            recurrence_rule: Some("FREQ=WEEKLY".into()),
            ..Default::default()
        })
        .unwrap();
    let done = db
        .statuses(t.project_id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Done")
        .unwrap();
    db.move_task(
        t.id,
        MoveTask {
            status_id: Some(done.id),
            ..Default::default()
        },
    )
    .unwrap();
    let open = db
        .list_tasks(&TaskQuery {
            categories: vec![Category::Todo],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(open.rows[0].due_date.as_deref(), Some("2030-01-18"));
}

#[test]
fn quick_add_every_sets_rule_and_first_date() {
    let mut db = db();
    let today = jiff::Zoned::now().date();
    let t = db.quick_add("Water plants every! 3 days", today).unwrap();
    assert_eq!(t.recurrence_rule.as_deref(), Some("AFTER:3D"));
    let t = db.quick_add("Team report every friday", today).unwrap();
    assert_eq!(t.recurrence_rule.as_deref(), Some("FREQ=WEEKLY;BYDAY=FR"));
    let due: jiff::civil::Date = t.due_date.unwrap().parse().unwrap();
    assert_eq!(due.weekday(), jiff::civil::Weekday::Friday);
    assert!(due >= today);
    assert!(
        db.create_task(NewTask {
            title: "x".into(),
            recurrence_rule: Some("FREQ=NEVER".into()),
            ..Default::default()
        })
        .is_err()
    );
}

// ---------- saved views ----------

#[test]
fn saved_views_round_trip_and_undo() {
    let mut db = db();
    let a = db
        .create_saved_view(
            "  Acme this week ",
            r#"{ "scope": "client", "search": "invoice" }"#,
        )
        .unwrap();
    assert_eq!(a.name, "Acme this week");
    assert_eq!(a.spec_json, r#"{"scope":"client","search":"invoice"}"#);
    let b = db
        .create_saved_view("Urgent", r#"{"sort":"priority"}"#)
        .unwrap();
    let names: Vec<String> = db
        .saved_views()
        .unwrap()
        .into_iter()
        .map(|v| v.name)
        .collect();
    assert_eq!(names, ["Acme this week", "Urgent"]);

    let renamed = db
        .update_saved_view(
            b.id,
            SavedViewPatch {
                name: Some("Urgent work".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(renamed.name, "Urgent work");
    assert_eq!(renamed.spec_json, b.spec_json);

    db.delete_saved_view(a.id).unwrap();
    assert_eq!(db.saved_views().unwrap().len(), 1);
    db.undo().unwrap();
    assert_eq!(db.saved_view(a.id).unwrap().name, "Acme this week");
    db.undo().unwrap();
    assert_eq!(db.saved_view(b.id).unwrap().name, "Urgent");
}

#[test]
fn saved_view_specs_must_be_json_objects() {
    let mut db = db();
    for bad in ["", "not json", "[1, 2]", "\"text\""] {
        assert!(db.create_saved_view("x", bad).is_err(), "{bad:?}");
    }
    assert!(db.create_saved_view("  ", "{}").is_err());
    let huge = format!(r#"{{"search":"{}"}}"#, "a".repeat(20_000));
    assert!(db.create_saved_view("big", &huge).is_err());
    let v = db.create_saved_view("ok", "{}").unwrap();
    let bad_patch = SavedViewPatch {
        spec_json: Some("[]".into()),
        ..Default::default()
    };
    assert!(db.update_saved_view(v.id, bad_patch).is_err());
}

// ---------- batches (bulk edits) ----------

fn live_titles(db: &Db) -> Vec<String> {
    let mut titles: Vec<String> = db
        .list_tasks(&TaskQuery::default())
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    titles.sort();
    titles
}

#[test]
fn a_batch_is_one_undo_step() {
    let mut db = db();
    let ids: Vec<Id> = ["a", "b", "c"]
        .iter()
        .map(|t| task(&mut db, t, None).id)
        .collect();
    db.batch("Delete 2 tasks", |db| {
        db.delete_task(ids[0])?;
        db.delete_task(ids[1])
    })
    .unwrap();
    assert_eq!(live_titles(&db), ["c"]);
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Delete 2 tasks")
    );

    assert_eq!(db.undo().unwrap(), "Delete 2 tasks");
    assert_eq!(live_titles(&db), ["a", "b", "c"]);
    db.redo().unwrap();
    assert_eq!(live_titles(&db), ["c"]);
    db.undo().unwrap();
    // The step before the batch is untouched.
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Create task “c”")
    );
}

#[test]
fn a_failing_batch_changes_nothing() {
    let mut db = db();
    let a = task(&mut db, "a", None);
    let before = db.history_state().unwrap();
    let missing = Id::new();
    let result = db.batch("Complete 2 tasks", |db| {
        db.set_task_done(a.id, true)?;
        db.set_task_done(missing, true)
    });
    assert!(result.is_err());
    assert!(db.task(a.id).unwrap().completed_at.is_none());
    assert_eq!(db.history_state().unwrap().undo, before.undo);
    // The connection is usable again afterwards.
    db.set_task_done(a.id, true).unwrap();
    assert!(db.task(a.id).unwrap().completed_at.is_some());
}

#[test]
fn repeating_tasks_repeat_inside_a_batch() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "Weekly report".into(),
            due_date: Some("2030-01-07".into()),
            recurrence_rule: Some("FREQ=WEEKLY".into()),
            ..Default::default()
        })
        .unwrap();
    let plain = task(&mut db, "plain", None);
    db.batch("Complete 2 tasks", |db| {
        db.set_task_done(t.id, true)?;
        db.set_task_done(plain.id, true)
    })
    .unwrap();
    let open = TaskQuery {
        categories: vec![Category::Todo, Category::Active],
        ..Default::default()
    };
    assert_eq!(db.list_tasks(&open).unwrap().total, 1);
    db.undo().unwrap();
    assert_eq!(db.list_tasks(&open).unwrap().total, 2);
}

// ---------- ordering clients and projects ----------

fn client_names(db: &Db) -> Vec<String> {
    db.clients(false)
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect()
}

fn project_names(db: &Db, client: Option<Id>) -> Vec<String> {
    db.projects(false)
        .unwrap()
        .into_iter()
        .filter(|p| !p.project.is_inbox && p.project.client_id == client)
        .map(|p| p.project.name)
        .collect()
}

#[test]
fn clients_move_up_and_down() {
    let mut db = db();
    let a = client(&mut db, "A");
    client(&mut db, "B");
    let c = client(&mut db, "C");
    assert!(db.move_client(c.id, Direction::Up).unwrap());
    assert_eq!(client_names(&db), ["A", "C", "B"]);
    assert!(db.move_client(c.id, Direction::Up).unwrap());
    assert_eq!(client_names(&db), ["C", "A", "B"]);
    // Already first: nothing changes and nothing goes on the undo stack.
    let before = db.history_state().unwrap();
    assert!(!db.move_client(c.id, Direction::Up).unwrap());
    assert_eq!(db.history_state().unwrap().undo, before.undo);
    assert!(db.move_client(a.id, Direction::Down).unwrap());
    assert_eq!(client_names(&db), ["C", "B", "A"]);
    assert!(!db.move_client(a.id, Direction::Down).unwrap());
    db.undo().unwrap();
    assert_eq!(client_names(&db), ["C", "A", "B"]);
}

#[test]
fn projects_move_within_their_client_only() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let other = client(&mut db, "Other");
    let web = project(&mut db, "Web", Some(acme.id));
    project(&mut db, "App", Some(acme.id));
    project(&mut db, "Elsewhere", Some(other.id));
    let mine = project(&mut db, "Mine", None);
    project(&mut db, "Hobby", None);

    assert!(db.move_project(web.id, Direction::Down).unwrap());
    assert_eq!(project_names(&db, Some(acme.id)), ["App", "Web"]);
    assert!(!db.move_project(web.id, Direction::Down).unwrap());
    assert_eq!(project_names(&db, Some(other.id)), ["Elsewhere"]);
    assert!(db.move_project(mine.id, Direction::Down).unwrap());
    assert_eq!(project_names(&db, None), ["Hobby", "Mine"]);

    let inbox = db
        .projects(false)
        .unwrap()
        .into_iter()
        .find(|p| p.project.is_inbox)
        .unwrap();
    assert!(!db.move_project(inbox.project.id, Direction::Down).unwrap());
}

#[test]
fn moving_skips_archived_rows() {
    let mut db = db();
    let a = client(&mut db, "A");
    let hidden = client(&mut db, "Hidden");
    let c = client(&mut db, "C");
    db.set_client_archived(hidden.id, true).unwrap();
    assert!(db.move_client(c.id, Direction::Up).unwrap());
    assert_eq!(client_names(&db), ["C", "A"]);
    assert!(!db.move_client(c.id, Direction::Up).unwrap());
    assert_eq!(db.client(a.id).unwrap().name, "A");
}

#[test]
fn moved_projects_go_last_and_move_up() {
    let mut db = db();
    let acme = client(&mut db, "Acme");
    let other = client(&mut db, "Other");
    // Each client's first project gets the same key; moving one over makes a tie.
    let first = project(&mut db, "First", Some(acme.id));
    let moved = project(&mut db, "Moved", Some(other.id));
    assert_eq!(first.sort_key, moved.sort_key);
    db.update_project(
        moved.id,
        ProjectPatch {
            client_id: Some(Some(acme.id)),
            ..Default::default()
        },
    )
    .unwrap();
    let names = project_names(&db, Some(acme.id));
    // A project moved to another client goes last there.
    assert_eq!(names, ["First", "Moved"]);
    let last = names.last().unwrap().clone();
    let last_id = if last == "First" { first.id } else { moved.id };
    assert!(db.move_project(last_id, Direction::Up).unwrap());
    let after = project_names(&db, Some(acme.id));
    assert_eq!(after.first().unwrap(), &last);
    assert_eq!(after.len(), 2);
    // One undo restores the original order and keys.
    db.undo().unwrap();
    assert_eq!(project_names(&db, Some(acme.id)), names);
}
