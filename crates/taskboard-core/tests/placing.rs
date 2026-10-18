//! Drag-and-drop placement in the sidebar: put a section, group or project
//! before another one (or at the end), possibly in another container, as
//! one undoable action.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

fn section(db: &mut Db, name: &str) -> Section {
    db.create_section(NewSection {
        name: name.into(),
        ..Default::default()
    })
    .unwrap()
}

fn group(db: &mut Db, name: &str, section: Id) -> Client {
    db.create_client(NewClient {
        name: name.into(),
        section_id: Some(section),
        ..Default::default()
    })
    .unwrap()
}

fn project(db: &mut Db, name: &str, group: Option<Id>, section: Option<Id>) -> Project {
    db.create_project(NewProject {
        name: name.into(),
        client_id: group,
        section_id: section,
        ..Default::default()
    })
    .unwrap()
}

fn section_names(db: &Db) -> Vec<String> {
    db.sections().unwrap().into_iter().map(|s| s.name).collect()
}

fn group_names(db: &Db, section: Id) -> Vec<String> {
    db.clients(false)
        .unwrap()
        .into_iter()
        .filter(|c| c.section_id == Some(section))
        .map(|c| c.name)
        .collect()
}

/// Projects directly in a section (no group), in sidebar order.
fn loose_names(db: &Db, section: Id) -> Vec<String> {
    db.projects(false)
        .unwrap()
        .into_iter()
        .map(|p| p.project)
        .filter(|p| !p.is_inbox && p.client_id.is_none() && p.section_id == Some(section))
        .map(|p| p.name)
        .collect()
}

fn group_project_names(db: &Db, group: Id) -> Vec<String> {
    db.projects(false)
        .unwrap()
        .into_iter()
        .map(|p| p.project)
        .filter(|p| p.client_id == Some(group))
        .map(|p| p.name)
        .collect()
}

#[test]
fn place_section_before_another_in_one_undo_step() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    section(&mut db, "B");
    let c = section(&mut db, "C");
    assert!(db.place_section(c.id, Some(a.id)).unwrap());
    assert_eq!(section_names(&db), ["C", "A", "B"]);
    db.undo().unwrap();
    assert_eq!(section_names(&db), ["A", "B", "C"]);
}

#[test]
fn place_section_at_the_end() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    section(&mut db, "B");
    section(&mut db, "C");
    assert!(db.place_section(a.id, None).unwrap());
    assert_eq!(section_names(&db), ["B", "C", "A"]);
}

#[test]
fn placing_an_item_where_it_already_is_changes_nothing() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let undo_before = db.history_state().unwrap().undo;
    // Before its own next sibling, or before itself: already there.
    assert!(!db.place_section(a.id, Some(b.id)).unwrap());
    assert!(!db.place_section(a.id, Some(a.id)).unwrap());
    assert!(!db.place_section(b.id, None).unwrap());
    assert_eq!(db.history_state().unwrap().undo, undo_before);
}

#[test]
fn place_group_reorders_within_its_section() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Clients");
    let acme = group(&mut db, "Acme", s.id);
    group(&mut db, "Bluebird", s.id);
    let north = group(&mut db, "Northwind", s.id);
    assert!(db.place_client(north.id, s.id, Some(acme.id)).unwrap());
    assert_eq!(group_names(&db, s.id), ["Northwind", "Acme", "Bluebird"]);
}

#[test]
fn place_group_into_another_section_takes_its_projects_along() {
    let mut db = Db::open_in_memory().unwrap();
    let clients = section(&mut db, "Clients");
    let teams = section(&mut db, "Teams");
    let acme = group(&mut db, "Acme", clients.id);
    let design = group(&mut db, "Design", teams.id);
    let web = project(&mut db, "Website", Some(acme.id), None);
    assert!(db.place_client(acme.id, teams.id, Some(design.id)).unwrap());
    assert_eq!(group_names(&db, teams.id), ["Acme", "Design"]);
    assert!(group_names(&db, clients.id).is_empty());
    assert_eq!(db.project(web.id).unwrap().section_id, Some(teams.id));
    // One undo puts the group and its project back.
    db.undo().unwrap();
    assert_eq!(db.client(acme.id).unwrap().section_id, Some(clients.id));
    assert_eq!(db.project(web.id).unwrap().section_id, Some(clients.id));
}

#[test]
fn place_project_reorders_within_its_group() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Clients");
    let acme = group(&mut db, "Acme", s.id);
    let web = project(&mut db, "Website", Some(acme.id), None);
    project(&mut db, "Mobile", Some(acme.id), None);
    let ads = project(&mut db, "Ads", Some(acme.id), None);
    assert!(
        db.place_project(ads.id, ProjectPlace::Group { id: acme.id }, Some(web.id))
            .unwrap()
    );
    assert_eq!(
        group_project_names(&db, acme.id),
        ["Ads", "Website", "Mobile"]
    );
}

#[test]
fn place_project_into_a_group_in_another_section_follows_the_group() {
    let mut db = Db::open_in_memory().unwrap();
    let personal = section(&mut db, "Personal");
    let teams = section(&mut db, "Teams");
    let design = group(&mut db, "Design", teams.id);
    let kit = project(&mut db, "Brand kit", Some(design.id), None);
    let hobby = project(&mut db, "Hobby", None, Some(personal.id));
    assert!(
        db.place_project(hobby.id, ProjectPlace::Group { id: design.id }, None)
            .unwrap()
    );
    let moved = db.project(hobby.id).unwrap();
    assert_eq!(moved.client_id, Some(design.id));
    assert_eq!(moved.section_id, Some(teams.id));
    assert_eq!(group_project_names(&db, design.id), ["Brand kit", "Hobby"]);
    db.undo().unwrap();
    let back = db.project(hobby.id).unwrap();
    assert_eq!((back.client_id, back.section_id), (None, Some(personal.id)));
    let _ = kit;
}

#[test]
fn place_project_out_of_its_group_into_a_section() {
    let mut db = Db::open_in_memory().unwrap();
    let clients = section(&mut db, "Clients");
    let personal = section(&mut db, "Personal");
    let acme = group(&mut db, "Acme", clients.id);
    let web = project(&mut db, "Website", Some(acme.id), None);
    let read = project(&mut db, "Reading", None, Some(personal.id));
    assert!(
        db.place_project(
            web.id,
            ProjectPlace::Section { id: personal.id },
            Some(read.id)
        )
        .unwrap()
    );
    let moved = db.project(web.id).unwrap();
    assert_eq!(
        (moved.client_id, moved.section_id),
        (None, Some(personal.id))
    );
    assert_eq!(loose_names(&db, personal.id), ["Website", "Reading"]);
}

#[test]
fn the_inbox_cannot_be_placed() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Clients");
    let inbox = db
        .projects(false)
        .unwrap()
        .into_iter()
        .map(|p| p.project)
        .find(|p| p.is_inbox)
        .unwrap();
    assert!(
        !db.place_project(inbox.id, ProjectPlace::Section { id: s.id }, None)
            .unwrap()
    );
    assert!(db.project(inbox.id).unwrap().section_id.is_none());
}

#[test]
fn placing_into_a_missing_container_is_refused() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Clients");
    let acme = group(&mut db, "Acme", s.id);
    let web = project(&mut db, "Website", Some(acme.id), None);
    assert!(db.place_client(acme.id, Id::new(), None).is_err());
    assert!(
        db.place_project(web.id, ProjectPlace::Group { id: Id::new() }, None)
            .is_err()
    );
    assert!(
        db.place_project(web.id, ProjectPlace::Section { id: Id::new() }, None)
            .is_err()
    );
}

#[test]
fn placing_works_when_siblings_share_a_sort_key() {
    let mut db = Db::open_in_memory().unwrap();
    let s = section(&mut db, "Clients");
    let acme = group(&mut db, "Acme", s.id);
    let other = group(&mut db, "Other", s.id);
    // Projects created in different groups can share keys once moved together.
    let a = project(&mut db, "A", Some(acme.id), None);
    let b = project(&mut db, "B", Some(other.id), None);
    db.update_project(
        b.id,
        ProjectPatch {
            client_id: Some(Some(acme.id)),
            ..Default::default()
        },
    )
    .unwrap();
    let c = project(&mut db, "C", Some(acme.id), None);
    assert!(
        db.place_project(c.id, ProjectPlace::Group { id: acme.id }, Some(a.id))
            .unwrap()
    );
    assert_eq!(group_project_names(&db, acme.id), ["C", "A", "B"]);
}
