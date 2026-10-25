//! Fixes from the 0.6.0 whole-app audit: counts, columns, repeats, restore,
//! duplicates, quick add.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

fn project(db: &mut Db, name: &str) -> Id {
    db.create_project(NewProject {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
    .id
}

fn task(db: &mut Db, title: &str, project: Id) -> Task {
    db.create_task(NewTask {
        title: title.into(),
        project_id: Some(project),
        ..Default::default()
    })
    .unwrap()
}

fn column(db: &Db, project: Id, name: &str) -> Status {
    db.statuses(project)
        .unwrap()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap()
}

#[test]
fn project_counts_leave_out_archived_tasks() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    task(&mut db, "Open", p);
    let hidden = task(&mut db, "Parked", p);
    db.set_task_archived(hidden.id, true).unwrap();
    let summary = db
        .projects(false)
        .unwrap()
        .into_iter()
        .find(|s| s.project.id == p)
        .unwrap();
    assert_eq!(summary.open_tasks, 1);
    assert_eq!(summary.total_tasks, 1);
}

#[test]
fn the_last_open_column_cannot_become_a_finished_one() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    // Close every open column but one.
    let open: Vec<Status> = db
        .statuses(p)
        .unwrap()
        .into_iter()
        .filter(|s| !s.category.is_closed())
        .collect();
    for s in &open[1..] {
        db.update_status(
            s.id,
            StatusPatch {
                category: Some(Category::Done),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let last = db.update_status(
        open[0].id,
        StatusPatch {
            category: Some(Category::Cancelled),
            ..Default::default()
        },
    );
    assert!(
        last.is_err(),
        "a project needs somewhere for new tasks to go"
    );
    // New tasks still start open.
    assert!(task(&mut db, "New", p).completed_at.is_none());
}

#[test]
fn moving_and_completing_a_repeating_task_makes_the_next_one_where_it_went() {
    let mut db = Db::open_in_memory().unwrap();
    let a = project(&mut db, "A");
    let b = project(&mut db, "B");
    let t = db
        .create_task(NewTask {
            title: "Invoice".into(),
            project_id: Some(a),
            due_date: Some("2031-01-06".into()),
            recurrence_rule: Some("FREQ=WEEKLY".into()),
            ..Default::default()
        })
        .unwrap();
    db.update_task(
        t.id,
        TaskPatch {
            project_id: Some(b),
            status_id: Some(column(&db, b, "Done").id),
            ..Default::default()
        },
    )
    .unwrap();
    let open = db
        .list_tasks(&TaskQuery {
            search: Some("Invoice".into()),
            categories: vec![Category::Todo, Category::Active],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(open.total, 1);
    assert_eq!(open.rows[0].project_id, b);
}

#[test]
fn restoring_a_task_in_an_archived_project_says_so() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Old");
    let t = task(&mut db, "Kept", p);
    db.set_task_archived(t.id, true).unwrap();
    db.set_project_archived(p, true).unwrap();
    let r = db.set_task_archived(t.id, false);
    assert!(
        r.as_ref()
            .is_err_and(|e| e.to_string().contains("archived")),
        "restoring into a hidden project must not make it vanish: {r:?}"
    );
}

#[test]
fn duplicating_a_task_with_a_long_title_works() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    let long = "x".repeat(300);
    let t = task(&mut db, &long, p);
    let copy = db.duplicate_task(t.id).unwrap();
    assert!(copy.title.chars().count() <= 300);
}

#[test]
fn quick_add_tags_drop_trailing_punctuation() {
    let db = Db::open_in_memory().unwrap();
    let p = db
        .preview_quick_add(
            "Call Ann about #design, then #billing.",
            jiff::civil::date(2031, 1, 6),
        )
        .unwrap();
    assert_eq!(p.parsed.tags, ["design", "billing"]);
}

#[test]
fn quick_add_huge_estimates_are_refused_not_overflowed() {
    let db = Db::open_in_memory().unwrap();
    let p = db.preview_quick_add("Big ~99999999999999999h30m", jiff::civil::date(2031, 1, 6));
    // Either rejected or kept as text, never a wrapped number.
    if let Ok(p) = p {
        assert!(p.parsed.estimate_minutes.is_none_or(|m| m > 0));
    }
}

#[test]
fn the_dashboard_counts_cancelled_tasks_as_not_done() {
    let mut db = Db::open_in_memory().unwrap();
    let acme = db
        .create_client(NewClient {
            name: "Acme".into(),
            ..Default::default()
        })
        .unwrap();
    let p = db
        .create_project(NewProject {
            name: "Site".into(),
            client_id: Some(acme.id),
            ..Default::default()
        })
        .unwrap()
        .id;
    let done = task(&mut db, "Shipped", p);
    db.set_task_done(done.id, true).unwrap();
    let dropped = task(&mut db, "Dropped", p);
    let cancelled = db
        .statuses(p)
        .unwrap()
        .into_iter()
        .find(|s| s.category == Category::Cancelled)
        .map(|s| s.id);
    let Some(cancelled) = cancelled else {
        // No cancelled column by default: add one.
        let c = db
            .create_status(NewStatus {
                project_id: p,
                name: "Dropped".into(),
                category: Category::Cancelled,
                color: None,
                after: None,
            })
            .unwrap();
        db.update_task(
            dropped.id,
            TaskPatch {
                status_id: Some(c.id),
                ..Default::default()
            },
        )
        .unwrap();
        return check_cancel(&db, acme.id);
    };
    db.update_task(
        dropped.id,
        TaskPatch {
            status_id: Some(cancelled),
            ..Default::default()
        },
    )
    .unwrap();
    check_cancel(&db, acme.id);
}

fn check_cancel(db: &Db, acme: Id) {
    let d = db.dashboard(jiff::Zoned::now().date()).unwrap();
    assert_eq!(d.completed_this_week, 1);
    let row = d
        .clients
        .iter()
        .find(|c| c.client_id == Some(acme))
        .unwrap();
    assert_eq!(row.done_this_week, 1);
}

#[test]
fn old_history_is_compacted_and_recent_undo_still_works() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    task(&mut db, "Old work", p);
    // Everything so far counts as old.
    let removed = db.compact_history(clock::now_ms() + 1).unwrap();
    assert!(removed > 0);
    assert_eq!(
        db.history_state().unwrap().undo,
        None,
        "old steps can't be undone any more"
    );
    let fresh = task(&mut db, "New work", p);
    assert!(db.history_state().unwrap().undo.is_some());
    db.undo().unwrap();
    assert!(db.task(fresh.id).is_err(), "the new step undoes");
    assert!(
        db.list_tasks(&TaskQuery::default())
            .unwrap()
            .rows
            .iter()
            .any(|r| r.title == "Old work")
    );
    // Nothing older than the cutoff: nothing to do.
    assert_eq!(db.compact_history(0).unwrap(), 0);
}
