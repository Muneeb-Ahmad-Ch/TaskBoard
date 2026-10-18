//! Progress for any list (Today, Inbox, All work…): what is done vs what is
//! left, from the task query.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

#[test]
fn completed_from_counts_only_tasks_finished_since_then() {
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    let today = jiff::Zoned::now().date().to_string();
    let mk = |db: &mut Db, title: &str| {
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(inbox),
            due_date: Some(today.clone()),
            ..Default::default()
        })
        .unwrap()
    };
    let a = mk(&mut db, "Done now");
    mk(&mut db, "Still open");
    db.set_task_done(a.id, true).unwrap();
    let start = taskboard_core::clock::now_ms() - 60_000;
    let done_today = db
        .summarize_tasks(&TaskQuery {
            due_to: Some(today.clone()),
            categories: vec![Category::Done],
            completed_from: Some(start),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(done_today.total, 1);
    let later = db
        .summarize_tasks(&TaskQuery {
            categories: vec![Category::Done],
            completed_from: Some(taskboard_core::clock::now_ms() + 60_000),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(later.total, 0);
}

#[test]
fn progress_excludes_cancelled_tasks() {
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    let mk = |db: &mut Db, title: &str| {
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(inbox),
            ..Default::default()
        })
        .unwrap()
    };
    let done = mk(&mut db, "Done");
    mk(&mut db, "Open");
    let dropped = mk(&mut db, "Dropped");
    db.set_task_done(done.id, true).unwrap();
    // The default workflow has no cancelled column; add one.
    let cancelled = db
        .create_status(NewStatus {
            project_id: inbox,
            name: "Won't do".into(),
            category: Category::Cancelled,
            color: None,
            after: None,
        })
        .unwrap();
    db.update_task(
        dropped.id,
        TaskPatch {
            status_id: Some(cancelled.id),
            ..Default::default()
        },
    )
    .unwrap();
    let s = db
        .summarize_tasks(&TaskQuery {
            project_id: Some(inbox),
            categories: vec![Category::Todo, Category::Active, Category::Done],
            top_level: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!((s.done, s.total), (1, 2));
}
