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

// ---------- Task 3: groups and projects belong to sections ----------

fn in_section(db: &mut Db, name: &str, section: Id) -> Project {
    db.create_project(NewProject {
        name: name.into(),
        section_id: Some(section),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn new_group_and_project_default_to_first_section() {
    let mut db = Db::open_in_memory().unwrap();
    let first = section(&mut db, "First");
    section(&mut db, "Second");
    let g = client(&mut db, "G");
    let p = project(&mut db, "P", None);
    assert_eq!(g.section_id, Some(first.id));
    assert_eq!(p.section_id, Some(first.id));
}

#[test]
fn fresh_db_creates_my_work_on_first_project() {
    let mut db = Db::open_in_memory().unwrap();
    let p = project(&mut db, "P", None);
    let sections = db.sections().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, "My work");
    assert_eq!(p.section_id, Some(sections[0].id));
}

#[test]
fn moving_group_moves_its_projects_in_one_undo() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let g = client(&mut db, "G");
    let p = project(&mut db, "P", Some(g.id));
    assert_eq!(p.section_id, Some(a.id));
    db.update_client(
        g.id,
        ClientPatch {
            section_id: Some(b.id),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.client(g.id).unwrap().section_id, Some(b.id));
    assert_eq!(db.project(p.id).unwrap().section_id, Some(b.id));
    db.undo().unwrap();
    assert_eq!(db.client(g.id).unwrap().section_id, Some(a.id));
    assert_eq!(db.project(p.id).unwrap().section_id, Some(a.id));
}

#[test]
fn project_follows_group_into_other_section() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let loose = in_section(&mut db, "Loose", a.id);
    let g = db
        .create_client(NewClient {
            name: "G".into(),
            section_id: Some(b.id),
            ..Default::default()
        })
        .unwrap();
    db.update_project(
        loose.id,
        ProjectPatch {
            client_id: Some(Some(g.id)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.project(loose.id).unwrap().section_id, Some(b.id));
}

#[test]
fn groupless_project_moves_between_sections() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let p = in_section(&mut db, "P", a.id);
    db.update_project(
        p.id,
        ProjectPatch {
            section_id: Some(b.id),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.project(p.id).unwrap().section_id, Some(b.id));
    // A grouped project moves with its group, not on its own.
    let g = client(&mut db, "G");
    let gp = project(&mut db, "GP", Some(g.id));
    let err = db.update_project(
        gp.id,
        ProjectPatch {
            section_id: Some(b.id),
            ..Default::default()
        },
    );
    assert!(err.is_err());
}

#[test]
fn section_query_lists_all_work_in_section() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let g = client(&mut db, "G");
    let grouped = project(&mut db, "Grouped", Some(g.id));
    let direct = in_section(&mut db, "Direct", a.id);
    let other = in_section(&mut db, "Other", b.id);
    for (title, p) in [("one", grouped.id), ("two", direct.id), ("three", other.id)] {
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(p),
            ..Default::default()
        })
        .unwrap();
    }
    db.create_task(NewTask {
        title: "inbox".into(),
        ..Default::default()
    })
    .unwrap();
    let page = db
        .list_tasks(&TaskQuery {
            section_id: Some(a.id),
            ..Default::default()
        })
        .unwrap();
    let mut titles: Vec<String> = page.rows.into_iter().map(|r| r.title).collect();
    titles.sort();
    assert_eq!(titles, ["one", "two"]);
}

#[test]
fn reorder_projects_within_section_and_group_only() {
    let mut db = Db::open_in_memory().unwrap();
    let a = section(&mut db, "A");
    let b = section(&mut db, "B");
    let first = in_section(&mut db, "First", a.id);
    in_section(&mut db, "Other section", b.id);
    let second = in_section(&mut db, "Second", a.id);
    assert!(db.move_project(second.id, Direction::Up).unwrap());
    let order: Vec<String> = db
        .projects(false)
        .unwrap()
        .into_iter()
        .filter(|p| p.project.section_id == Some(a.id))
        .map(|p| p.project.name)
        .collect();
    assert_eq!(order, ["Second", "First"]);
    assert!(!db.move_project(second.id, Direction::Up).unwrap());
    assert!(db.project(first.id).is_ok());
}

#[test]
fn sample_workspace_uses_or_creates_clients_and_personal() {
    let today = jiff::civil::date(2026, 10, 17);
    let mut db = Db::open_in_memory().unwrap();
    let mine = section(&mut db, "Family");
    db.create_sample_workspace(today).unwrap();
    let names: Vec<String> = db.sections().unwrap().into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["Family", "Clients", "Personal"]);
    db.remove_sample_workspace().unwrap();
    let names: Vec<String> = db.sections().unwrap().into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["Family"], "only the sample's empty sections go");
    assert!(db.section(mine.id).is_ok());
}

// ---------- Task 4: roll-ups and first-run setups ----------

#[test]
fn dashboard_rolls_up_groupless_projects_per_section() {
    let mut db = Db::open_in_memory().unwrap();
    let work = section(&mut db, "Work");
    let family = section(&mut db, "Family");
    let g = client(&mut db, "Acme");
    let gp = project(&mut db, "Site", Some(g.id));
    let home = in_section(&mut db, "Home", family.id);
    let side = in_section(&mut db, "Side", work.id);
    for (title, p) in [("a", gp.id), ("b", home.id), ("c", side.id)] {
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(p),
            ..Default::default()
        })
        .unwrap();
    }
    let today = jiff::civil::date(2026, 10, 17);
    let rows = db.dashboard(today).unwrap().clients;
    let find = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    let acme = find("Acme");
    assert_eq!(
        (acme.client_id, acme.section_id),
        (Some(g.id), Some(work.id))
    );
    assert_eq!(acme.section_name, "Work");
    let fam = find("Family");
    assert_eq!(
        (fam.client_id, fam.section_id, fam.open),
        (None, Some(family.id), 1)
    );
    let loose = rows
        .iter()
        .find(|r| r.client_id.is_none() && r.section_id == Some(work.id))
        .unwrap();
    assert_eq!((loose.name.as_str(), loose.open), ("Work", 1));
    assert!(
        !rows.iter().any(|r| r.name == "Personal"),
        "no fixed Personal row"
    );
}

#[test]
fn hours_by_group_include_section_rows() {
    let mut db = Db::open_in_memory().unwrap();
    let family = section(&mut db, "Family");
    let home = in_section(&mut db, "Home", family.id);
    let start = 1_800_000_000_000_i64;
    db.log_time(NewTimeEntry {
        project_id: Some(home.id),
        started_at: start,
        ended_at: Some(start + 30 * 60_000),
        ..Default::default()
    })
    .unwrap();
    let hours = db.hours_by_client(start - 1, start + 3_600_000).unwrap();
    let row = hours
        .iter()
        .find(|h| h.section_id == Some(family.id))
        .unwrap();
    assert_eq!(
        (row.client_id, row.client_name.as_str(), row.logged_min),
        (None, "Family", 30)
    );
}

#[test]
fn apply_setup_creates_exact_sections() {
    let cases = [
        (
            Setup::Freelancer,
            vec![("Clients", "client"), ("Personal", "")],
        ),
        (
            Setup::Company,
            vec![
                ("Departments", "department"),
                ("Clients", "client"),
                ("Internal", ""),
            ],
        ),
        (
            Setup::PersonalLife,
            vec![
                ("Home", ""),
                ("Family", ""),
                ("Health", ""),
                ("Personal", ""),
            ],
        ),
        (Setup::Blank, vec![("My work", "")]),
    ];
    for (setup, want) in cases {
        let mut db = Db::open_in_memory().unwrap();
        let made = db.apply_setup(setup).unwrap();
        let got: Vec<(String, String)> = made.into_iter().map(|s| (s.name, s.item_name)).collect();
        let want: Vec<(String, String)> = want
            .iter()
            .map(|(n, i)| ((*n).to_owned(), (*i).to_owned()))
            .collect();
        assert_eq!(got, want, "{setup:?}");
        assert_eq!(db.sections().unwrap().len(), got.len());
        db.undo().unwrap();
        assert!(db.sections().unwrap().is_empty(), "one undo step");
    }
}
