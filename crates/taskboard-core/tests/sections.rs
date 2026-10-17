#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use rusqlite::Connection;
use taskboard_core::*;

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

/// Turns a current database into the schema-6 shape (no sections), so
/// reopening it runs migration 7 on realistic data.
fn downgrade_to_v6(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "DROP INDEX IF EXISTS client_section;
         DROP INDEX IF EXISTS project_section;
         ALTER TABLE client DROP COLUMN section_id;
         ALTER TABLE project DROP COLUMN section_id;
         DROP TABLE section;
         PRAGMA user_version = 6;",
    )
    .unwrap();
}

#[test]
fn migration_maps_existing_clients_and_personal_projects() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v6.db");
    let (acme, old_client, web, old_web, mine, old_mine, keys) = {
        let mut db = Db::open(&path).unwrap();
        let acme = client(&mut db, "Acme");
        let old_client = client(&mut db, "Old client");
        let web = project(&mut db, "Website", Some(acme.id));
        let old_web = project(&mut db, "Old site", Some(old_client.id));
        let mine = project(&mut db, "Mine", None);
        let old_mine = project(&mut db, "Old hobby", None);
        db.set_client_archived(old_client.id, true).unwrap();
        db.set_project_archived(old_mine.id, true).unwrap();
        let keys: Vec<String> = db
            .projects(true)
            .unwrap()
            .into_iter()
            .map(|p| p.project.sort_key)
            .collect();
        (
            acme.id,
            old_client.id,
            web.id,
            old_web.id,
            mine.id,
            old_mine.id,
            keys,
        )
    };
    downgrade_to_v6(&path);

    let db = Db::open(&path).unwrap();
    let sections = db.sections().unwrap();
    let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Clients", "Personal"]);
    let (clients, personal) = (&sections[0], &sections[1]);
    assert_eq!(clients.item_name, "client");
    assert_eq!(personal.item_name, "");

    for c in db.clients(true).unwrap() {
        assert_eq!(c.section_id, Some(clients.id), "client {}", c.name);
    }
    assert!([acme, old_client].iter().all(|id| db.client(*id).is_ok()));
    for id in [web, old_web] {
        assert_eq!(db.project(id).unwrap().section_id, Some(clients.id));
    }
    for id in [mine, old_mine] {
        assert_eq!(db.project(id).unwrap().section_id, Some(personal.id));
    }
    let inbox = db.inbox_id().unwrap();
    assert_eq!(db.project(inbox).unwrap().section_id, None);
    let after: Vec<String> = db
        .projects(true)
        .unwrap()
        .into_iter()
        .map(|p| p.project.sort_key)
        .collect();
    assert_eq!(after, keys, "project order unchanged");
    assert_eq!(
        db.history_state().unwrap().undo.as_deref(),
        Some("Archive project “Old hobby”"),
        "the migration adds no undo step"
    );
}

#[test]
fn fresh_database_has_no_sections() {
    let db = Db::open_in_memory().unwrap();
    assert!(db.sections().unwrap().is_empty());
}

#[test]
fn migration_without_data_creates_no_sections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.db");
    drop(Db::open(&path).unwrap());
    downgrade_to_v6(&path);
    let db = Db::open(&path).unwrap();
    assert!(db.sections().unwrap().is_empty());
}

// ---------- Task 2: create / edit / reorder / delete ----------

fn section(db: &mut Db, name: &str) -> Section {
    db.create_section(NewSection {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
}

fn names(db: &Db) -> Vec<String> {
    db.sections().unwrap().into_iter().map(|s| s.name).collect()
}

#[test]
fn create_rename_recolor_and_undo() {
    let mut db = Db::open_in_memory().unwrap();
    let teams = section(&mut db, "  Teams ");
    assert_eq!(teams.name, "Teams");
    assert_eq!(teams.item_name, "");
    let edited = db
        .update_section(
            teams.id,
            SectionPatch {
                name: Some("Departments".into()),
                item_name: Some("department".into()),
                color: Some("150".into()),
            },
        )
        .unwrap();
    assert_eq!(
        (
            edited.name.as_str(),
            edited.item_name.as_str(),
            edited.color.as_str()
        ),
        ("Departments", "department", "150")
    );
    db.undo().unwrap();
    assert_eq!(db.section(teams.id).unwrap().name, "Teams");
}

#[test]
fn blank_name_refused_duplicates_allowed() {
    let mut db = Db::open_in_memory().unwrap();
    assert!(
        db.create_section(NewSection {
            name: "   ".into(),
            ..Default::default()
        })
        .is_err()
    );
    section(&mut db, "Teams");
    section(&mut db, "Teams");
    assert_eq!(names(&db), ["Teams", "Teams"]);
}

#[test]
fn move_section_up_down_and_ends() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    section(&mut db, "B");
    let c = section(&mut db, "C");
    assert!(db.move_section(c.id, Direction::Up).unwrap());
    assert!(db.move_section(c.id, Direction::Up).unwrap());
    assert_eq!(names(&db), ["C", "A", "B"]);
    let before = db.history_state().unwrap();
    assert!(!db.move_section(c.id, Direction::Up).unwrap());
    assert_eq!(db.history_state().unwrap().undo, before.undo);
    assert!(db.move_section(a.id, Direction::Down).unwrap());
    assert_eq!(names(&db), ["C", "B", "A"]);
}

#[test]
fn delete_empty_section_is_undoable() {
    let mut db = Db::open_in_memory().unwrap();
    section(&mut db, "Keep");
    let gone = section(&mut db, "Gone");
    db.delete_section(gone.id, SectionDelete::Everything)
        .unwrap();
    assert_eq!(names(&db), ["Keep"]);
    db.undo().unwrap();
    assert_eq!(names(&db), ["Keep", "Gone"]);
}

#[test]
fn delete_moving_contents_is_one_undo_step() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Old");
    let t = section(&mut db, "New");
    let g = db
        .create_client(NewClient {
            name: "G".into(),
            section_id: Some(s.id),
            ..Default::default()
        })
        .unwrap();
    let p = project(&mut db, "P", Some(g.id));
    let q = db
        .create_project(NewProject {
            name: "Q".into(),
            section_id: Some(s.id),
            ..Default::default()
        })
        .unwrap();
    db.delete_section(s.id, SectionDelete::MoveTo { section_id: t.id })
        .unwrap();
    assert_eq!(names(&db), ["New"]);
    assert_eq!(db.client(g.id).unwrap().section_id, Some(t.id));
    assert_eq!(db.project(p.id).unwrap().section_id, Some(t.id));
    assert_eq!(db.project(q.id).unwrap().section_id, Some(t.id));
    db.undo().unwrap();
    assert_eq!(names(&db), ["Old", "New"]);
    assert_eq!(db.client(g.id).unwrap().section_id, Some(s.id));
    assert_eq!(db.project(p.id).unwrap().section_id, Some(s.id));
    assert_eq!(db.project(q.id).unwrap().section_id, Some(s.id));
}

#[test]
fn delete_everything_soft_deletes_contents() {
    let mut db = Db::open_in_memory().unwrap();
    section(&mut db, "Keep");
    let s = section(&mut db, "Gone");
    let g = db
        .create_client(NewClient {
            name: "G".into(),
            section_id: Some(s.id),
            ..Default::default()
        })
        .unwrap();
    let p = project(&mut db, "P", Some(g.id));
    let t = db
        .create_task(NewTask {
            title: "t".into(),
            project_id: Some(p.id),
            ..Default::default()
        })
        .unwrap();
    db.delete_section(s.id, SectionDelete::Everything).unwrap();
    assert!(db.client(g.id).is_err());
    assert!(db.project(p.id).is_err());
    assert!(db.task(t.id).is_err(), "task deleted with its project");
    db.undo().unwrap();
    assert_eq!(db.client(g.id).unwrap().name, "G");
    assert_eq!(db.project(p.id).unwrap().name, "P");
    assert_eq!(db.task(t.id).unwrap().title, "t");
}

#[test]
fn cannot_delete_last_section() {
    let mut db = Db::open_in_memory().unwrap();
    let only = section(&mut db, "Only");
    let err = db
        .delete_section(only.id, SectionDelete::Everything)
        .unwrap_err();
    assert!(
        err.to_string().contains("Keep at least one section"),
        "{err}"
    );
}
