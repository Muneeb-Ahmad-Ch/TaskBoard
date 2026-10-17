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
