//! The Waiting column: in the default workflow, and added to existing
//! projects in one step. Time blocks remember the task they are for.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

fn names(db: &Db, project: Id) -> Vec<String> {
    db.statuses(project)
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect()
}

fn project(db: &mut Db, name: &str) -> Project {
    db.create_project(NewProject {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn new_projects_have_a_waiting_column_that_counts_as_open() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    let cols = db.statuses(p.id).unwrap();
    let waiting = cols.iter().find(|s| s.name == "Waiting").unwrap();
    assert_eq!(waiting.category, Category::Active);
    let at = cols.iter().position(|s| s.name == "Waiting").unwrap();
    assert_eq!(cols[at - 1].name, "In Progress");
}

#[test]
fn adds_a_column_to_every_project_missing_it_as_one_undo_step() {
    let mut db = Db::open_in_memory().unwrap();
    let a = project(&mut db, "A");
    let b = project(&mut db, "B");
    // A project from before Waiting was a default, and a custom workflow
    // without an in-progress column.
    for p in [a.id, b.id] {
        let waiting = db
            .statuses(p)
            .unwrap()
            .into_iter()
            .find(|s| s.name == "Waiting")
            .unwrap();
        let to_do = db
            .statuses(p)
            .unwrap()
            .into_iter()
            .find(|s| s.name == "To Do")
            .unwrap();
        db.delete_status(waiting.id, to_do.id).unwrap();
    }
    for name in ["In Progress", "Review"] {
        let col = db
            .statuses(b.id)
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap();
        let backlog = db.statuses(b.id).unwrap()[0].id;
        db.delete_status(col.id, backlog).unwrap();
    }
    let inbox = db.inbox_id().unwrap();
    let before_inbox = names(&db, inbox);

    let added = db
        .add_status_everywhere("waiting", Category::Active, Some("70"))
        .unwrap();
    // A and B; the Inbox already has one.
    assert_eq!(added, 2);
    assert_eq!(names(&db, inbox), before_inbox);
    // After the first in-progress column…
    assert_eq!(
        names(&db, a.id),
        [
            "Backlog",
            "To Do",
            "In Progress",
            "waiting",
            "Review",
            "Done"
        ]
    );
    // …else just before the finished ones.
    assert_eq!(names(&db, b.id), ["Backlog", "To Do", "waiting", "Done"]);
    let col = db
        .statuses(b.id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "waiting")
        .unwrap();
    assert_eq!((col.category, col.color.as_str()), (Category::Active, "70"));

    // Nothing left to add: no change and no undo step.
    let label = db.history_state().unwrap().undo;
    assert_eq!(
        db.add_status_everywhere("Waiting", Category::Active, None)
            .unwrap(),
        0
    );
    assert_eq!(db.history_state().unwrap().undo, label);

    db.undo().unwrap();
    assert!(!names(&db, a.id).iter().any(|n| n == "waiting"));
    assert!(!names(&db, b.id).iter().any(|n| n == "waiting"));
}

#[test]
fn rejects_a_blank_column_name() {
    let mut db = Db::open_in_memory().unwrap();
    assert!(
        db.add_status_everywhere("  ", Category::Active, None)
            .is_err()
    );
}

#[test]
fn occurrences_name_the_task_a_block_is_for() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Site");
    let t = db
        .create_task(NewTask {
            title: "Write copy".into(),
            project_id: Some(p.id),
            ..Default::default()
        })
        .unwrap();
    let start = 1_900_000_000_000;
    db.create_event(NewEvent {
        kind: EventKind::Block,
        title: "Write copy".into(),
        client_id: None,
        project_id: Some(p.id),
        task_id: Some(t.id),
        start_at: Some(start),
        end_at: Some(start + 3_600_000),
        all_day_start: None,
        all_day_end: None,
        time_zone: None,
        location: None,
        meeting_url: None,
        attendees: None,
        notes_md: None,
        status: None,
        counts_as_time: None,
        flexible: None,
        recurrence_rule: None,
    })
    .unwrap();
    let occ = db
        .occurrences(start - 3_600_000, start + 7_200_000)
        .unwrap();
    assert_eq!(occ.len(), 1);
    assert_eq!(occ[0].task_id, Some(t.id));
}
