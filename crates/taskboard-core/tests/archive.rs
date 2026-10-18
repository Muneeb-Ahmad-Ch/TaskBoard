//! Archiving tasks (one by one, or every completed task in a list) and
//! pinned projects.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

fn project(db: &mut Db, name: &str) -> Project {
    db.create_project(NewProject {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
}

fn task(db: &mut Db, title: &str, project: Id, parent: Option<Id>) -> Task {
    db.create_task(NewTask {
        title: title.into(),
        project_id: Some(project),
        parent_id: parent,
        ..Default::default()
    })
    .unwrap()
}

fn titles(db: &Db, q: TaskQuery) -> Vec<String> {
    let mut v: Vec<String> = db
        .list_tasks(&TaskQuery { limit: 500, ..q })
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    v.sort();
    v
}

#[test]
fn archiving_hides_a_task_and_its_subtasks_in_one_undo_step() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Website");
    let parent = task(&mut db, "Launch", p.id, None);
    task(&mut db, "Copy", p.id, Some(parent.id));
    task(&mut db, "Other", p.id, None);
    let archived = db.set_task_archived(parent.id, true).unwrap();
    assert!(archived.archived_at.is_some());
    assert_eq!(titles(&db, TaskQuery::default()), ["Other"]);
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Archive “Launch”")
    );
    db.undo().unwrap();
    assert_eq!(
        titles(&db, TaskQuery::default()),
        ["Copy", "Launch", "Other"]
    );
}

#[test]
fn archived_only_lists_archived_tasks_with_their_archive_time() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Website");
    let a = task(&mut db, "Old", p.id, None);
    task(&mut db, "Current", p.id, None);
    db.set_task_archived(a.id, true).unwrap();
    let page = db
        .list_tasks(&TaskQuery {
            archived_only: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].title, "Old");
    assert!(page.rows[0].archived_at.is_some());
    let all = db
        .list_tasks(&TaskQuery {
            include_archived: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(all.total, 2);
    assert!(
        all.rows
            .iter()
            .any(|r| r.title == "Current" && r.archived_at.is_none())
    );
}

#[test]
fn restoring_brings_back_subtasks_archived_with_it_but_not_earlier_ones() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Website");
    let parent = task(&mut db, "Launch", p.id, None);
    let early = task(&mut db, "Archived first", p.id, Some(parent.id));
    task(&mut db, "Copy", p.id, Some(parent.id));
    db.set_task_archived(early.id, true).unwrap();
    db.set_task_archived(parent.id, true).unwrap();
    db.set_task_archived(parent.id, false).unwrap();
    assert_eq!(titles(&db, TaskQuery::default()), ["Copy", "Launch"]);
    assert!(db.task(early.id).unwrap().archived_at.is_some());
}

#[test]
fn restoring_a_subtask_brings_back_its_archived_parent() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Website");
    let parent = task(&mut db, "Launch", p.id, None);
    let child = task(&mut db, "Copy", p.id, Some(parent.id));
    db.set_task_archived(parent.id, true).unwrap();
    db.set_task_archived(child.id, false).unwrap();
    assert!(db.task(parent.id).unwrap().archived_at.is_none());
    assert!(db.task(child.id).unwrap().archived_at.is_none());
}

#[test]
fn archiving_twice_or_restoring_an_active_task_changes_nothing() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "Website");
    let t = task(&mut db, "Launch", p.id, None);
    let before = db.history_state().unwrap().undo;
    db.set_task_archived(t.id, false).unwrap();
    assert_eq!(db.history_state().unwrap().undo, before);
    let first = db.set_task_archived(t.id, true).unwrap().archived_at;
    let label = db.history_state().unwrap().undo;
    let second = db.set_task_archived(t.id, true).unwrap().archived_at;
    assert_eq!(first, second);
    assert_eq!(db.history_state().unwrap().undo, label);
}

#[test]
fn archived_tasks_leave_the_sidebar_counts() {
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    let today = jiff::Zoned::now().date();
    let t = db
        .create_task(NewTask {
            title: "Due today".into(),
            project_id: Some(inbox),
            due_date: Some(today.to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(db.sidebar_counts(today).unwrap().today, 1);
    db.set_task_archived(t.id, true).unwrap();
    let counts = db.sidebar_counts(today).unwrap();
    assert_eq!((counts.today, counts.inbox), (0, 0));
}

#[test]
fn archive_completed_takes_only_closed_top_level_tasks_in_scope() {
    let mut db = Db::open_in_memory().unwrap();
    let web = project(&mut db, "Website");
    let other = project(&mut db, "Other");
    let done = task(&mut db, "Shipped", web.id, None);
    task(&mut db, "Its subtask", web.id, Some(done.id));
    task(&mut db, "Still open", web.id, None);
    let elsewhere = task(&mut db, "Done elsewhere", other.id, None);
    db.set_task_done(done.id, true).unwrap();
    db.set_task_done(elsewhere.id, true).unwrap();
    let n = db
        .archive_completed(&TaskQuery {
            project_id: Some(web.id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        titles(
            &db,
            TaskQuery {
                categories: vec![],
                ..Default::default()
            }
        ),
        ["Done elsewhere", "Still open"]
    );
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Archive 1 completed task")
    );
    db.undo().unwrap();
    assert!(db.task(done.id).unwrap().archived_at.is_none());
}

#[test]
fn archive_completed_with_nothing_to_archive_adds_no_undo_step() {
    let mut db = Db::open_in_memory().unwrap();
    let web = project(&mut db, "Website");
    task(&mut db, "Open", web.id, None);
    let before = db.history_state().unwrap().undo;
    assert_eq!(db.archive_completed(&TaskQuery::default()).unwrap(), 0);
    assert_eq!(db.history_state().unwrap().undo, before);
}

#[test]
fn pins_keep_their_order_and_drop_projects_that_are_gone() {
    let mut db = Db::open_in_memory().unwrap();
    let a = project(&mut db, "A");
    let b = project(&mut db, "B");
    let c = project(&mut db, "C");
    db.pin_project(a.id, None).unwrap();
    db.pin_project(b.id, None).unwrap();
    db.pin_project(c.id, Some(a.id)).unwrap();
    assert_eq!(db.pinned_projects().unwrap(), [c.id, a.id, b.id]);
    // Pinning again moves it rather than duplicating it.
    db.pin_project(c.id, None).unwrap();
    assert_eq!(db.pinned_projects().unwrap(), [a.id, b.id, c.id]);
    db.unpin_project(a.id).unwrap();
    db.set_project_archived(b.id, true).unwrap();
    db.delete_project(c.id).unwrap();
    assert!(db.pinned_projects().unwrap().is_empty());
    // An archived project comes back pinned when it is restored.
    db.set_project_archived(b.id, false).unwrap();
    assert_eq!(db.pinned_projects().unwrap(), [b.id]);
}

#[test]
fn the_inbox_and_unknown_projects_cannot_be_pinned() {
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    assert!(db.pin_project(inbox, None).is_err());
    assert!(db.pin_project(Id::new(), None).is_err());
    let p = project(&mut db, "A");
    assert!(db.set_pinned_projects(vec![p.id, p.id, Id::new()]).is_ok());
    assert_eq!(db.pinned_projects().unwrap(), [p.id]);
}
