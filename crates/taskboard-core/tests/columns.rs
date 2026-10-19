//! Board columns as settings: the columns new projects start with (editable),
//! adding missing ones to every project, and the one-time Waiting backfill
//! for projects made before Waiting was a default.
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

fn col(name: &str, category: Category, color: &str) -> DefaultColumn {
    DefaultColumn {
        name: name.into(),
        category,
        color: color.into(),
    }
}

#[test]
fn the_built_in_columns_are_the_default() {
    let db = Db::open_in_memory().unwrap();
    let cols = db.default_columns().unwrap();
    let got: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        got,
        [
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ]
    );
    assert_eq!(cols[3].category, Category::Active);
}

#[test]
fn new_projects_start_with_the_chosen_columns() {
    let mut db = Db::open_in_memory().unwrap();
    db.set_default_columns(vec![
        col("Ideas", Category::Todo, "gray"),
        col("Doing", Category::Active, "262"),
        col("Waiting on client", Category::Active, "70"),
        col("Shipped", Category::Done, "150"),
    ])
    .unwrap();
    let p = project(&mut db, "Site");
    assert_eq!(
        names(&db, p.id),
        ["Ideas", "Doing", "Waiting on client", "Shipped"]
    );
    // Settings changes are not undo steps (Ctrl+Z is for your work).
    let undo = db.history_state().unwrap().undo;
    assert_eq!(undo.as_deref(), Some("Create project “Site”"));
    // Back to the built-in set.
    db.reset_default_columns().unwrap();
    assert_eq!(db.default_columns().unwrap().len(), 6);
}

#[test]
fn the_column_list_must_be_usable() {
    let mut db = Db::open_in_memory().unwrap();
    let err = |db: &mut Db, cols: Vec<DefaultColumn>| db.set_default_columns(cols).is_err();
    // Needs somewhere open and somewhere done.
    assert!(err(&mut db, vec![col("Done", Category::Done, "150")]));
    assert!(err(&mut db, vec![col("To Do", Category::Todo, "230")]));
    // Names: not blank, not twice (any case).
    assert!(err(
        &mut db,
        vec![
            col(" ", Category::Todo, "230"),
            col("Done", Category::Done, "150")
        ]
    ));
    assert!(err(
        &mut db,
        vec![
            col("To Do", Category::Todo, "230"),
            col("to do", Category::Active, "262"),
            col("Done", Category::Done, "150"),
        ]
    ));
    assert!(err(&mut db, vec![]));
    // A bad color is refused too.
    assert!(err(
        &mut db,
        vec![
            col("To Do", Category::Todo, "#zzz"),
            col("Done", Category::Done, "150")
        ]
    ));
    // Unchanged after the refusals.
    assert_eq!(db.default_columns().unwrap().len(), 6);
}

#[test]
fn missing_columns_are_added_to_every_project_in_one_step() {
    let mut db = Db::open_in_memory().unwrap();
    let a = project(&mut db, "A");
    let b = project(&mut db, "B");
    // A lost Waiting and Review; B has its own extra column.
    for name in ["Waiting", "Review"] {
        let s = db
            .statuses(a.id)
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap();
        let backlog = db.statuses(a.id).unwrap()[0].id;
        db.delete_status(s.id, backlog).unwrap();
    }
    db.create_status(NewStatus {
        project_id: b.id,
        name: "QA".into(),
        category: Category::Active,
        color: None,
        after: None,
    })
    .unwrap();
    assert_eq!(db.projects_missing_columns().unwrap(), 1);

    let added = db.add_missing_columns().unwrap();
    assert_eq!(added, 1);
    assert_eq!(
        names(&db, a.id),
        [
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ]
    );
    // Nothing removed or renamed elsewhere.
    assert!(names(&db, b.id).contains(&"QA".to_owned()));
    assert_eq!(db.projects_missing_columns().unwrap(), 0);
    // Nothing to do: no change, no undo step.
    let label = db.history_state().unwrap().undo;
    assert_eq!(db.add_missing_columns().unwrap(), 0);
    assert_eq!(db.history_state().unwrap().undo, label);
    // One undo step takes it all back.
    db.undo().unwrap();
    assert_eq!(
        names(&db, a.id),
        ["Backlog", "To Do", "In Progress", "Done"]
    );
}

#[test]
fn projects_from_before_waiting_get_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let p = {
        let mut db = Db::open(&path).unwrap();
        project(&mut db, "Old project")
    };
    // Make it look like a database from 0.5.0: no Waiting, no record of the backfill.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE status SET deleted_at = 1 WHERE lower(name) = 'waiting'",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM app_meta WHERE key LIKE 'backfill:%'", [])
            .unwrap();
    }
    let mut db = Db::open(&path).unwrap();
    let cols = names(&db, p.id);
    assert_eq!(
        cols,
        [
            "Backlog",
            "To Do",
            "In Progress",
            "Waiting",
            "Review",
            "Done"
        ]
    );
    // Not an undo step: the user didn't do it.
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Create project “Old project”")
    );
    // Removed on purpose afterwards: it stays removed.
    let waiting = db
        .statuses(p.id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Waiting")
        .unwrap();
    let todo = db
        .statuses(p.id)
        .unwrap()
        .into_iter()
        .find(|s| s.name == "To Do")
        .unwrap();
    db.delete_status(waiting.id, todo.id).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert!(!names(&db, p.id).iter().any(|n| n == "Waiting"));
}
