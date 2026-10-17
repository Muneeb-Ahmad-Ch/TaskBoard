#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jiff::civil::{Time, date};
use taskboard_core::planning::local_ms;
use taskboard_core::*;

fn at(y: i16, m: i8, d: i8, h: i8) -> i64 {
    local_ms(date(y, m, d), Time::new(h, 0, 0, 0).unwrap()).unwrap()
}

fn meeting(title: &str, start: i64, end: i64) -> NewEvent {
    NewEvent {
        kind: EventKind::Meeting,
        title: title.into(),
        client_id: None,
        project_id: None,
        task_id: None,
        start_at: Some(start),
        end_at: Some(end),
        all_day_start: None,
        all_day_end: None,
        time_zone: None,
        location: Some("Office, room 2".into()),
        meeting_url: None,
        attendees: None,
        notes_md: Some("Agenda; first line\nsecond line".into()),
        status: None,
        counts_as_time: None,
        flexible: None,
        recurrence_rule: None,
    }
}

/// A small but varied workspace.
fn workspace() -> Db {
    let mut db = Db::open_in_memory().unwrap();
    let acme = db
        .create_client(NewClient {
            name: "Acme".into(),
            time_zone: Some("Europe/London".into()),
            ..Default::default()
        })
        .unwrap();
    let site = db
        .create_project(NewProject {
            name: "Site".into(),
            client_id: Some(acme.id),
            ..Default::default()
        })
        .unwrap();
    let parent = db
        .create_task(NewTask {
            title: "Launch".into(),
            project_id: Some(site.id),
            due_date: Some("2030-01-10".into()),
            tags: vec!["web".into()],
            ..Default::default()
        })
        .unwrap();
    db.create_task(NewTask {
        title: "QA pass".into(),
        parent_id: Some(parent.id),
        ..Default::default()
    })
    .unwrap();
    db.create_task(NewTask {
        title: "Inbox thought".into(),
        ..Default::default()
    })
    .unwrap();
    let standup = db
        .create_event(NewEvent {
            recurrence_rule: Some("FREQ=WEEKLY;BYDAY=MO".into()),
            project_id: Some(site.id),
            ..meeting("Standup", at(2030, 1, 7, 9), at(2030, 1, 7, 10))
        })
        .unwrap();
    db.edit_occurrence(
        standup.id,
        at(2030, 1, 14, 9),
        OccurrencePatch {
            cancelled: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    db.set_day_override(DayOverride {
        date: "2030-01-01".into(),
        kind: OverrideKind::Off,
        intervals: vec![],
        note: "New year".into(),
    })
    .unwrap();
    db.log_time(NewTimeEntry {
        task_id: Some(parent.id),
        started_at: at(2030, 1, 7, 11),
        ended_at: Some(at(2030, 1, 7, 12)),
        ..Default::default()
    })
    .unwrap();
    db
}

fn titles(db: &Db) -> Vec<String> {
    let mut t: Vec<String> = db
        .list_tasks(&TaskQuery {
            include_archived: true,
            limit: 1000,
            ..Default::default()
        })
        .unwrap()
        .rows
        .into_iter()
        .map(|r| {
            format!(
                "{} | {} | {:?} | {:?} | {:?}",
                r.title,
                r.project_name,
                r.due_date,
                r.tags,
                r.parent_id.is_some()
            )
        })
        .collect();
    t.sort();
    t
}

#[test]
fn json_round_trip_into_a_fresh_database() {
    let src = workspace();
    let json = src.export_json().unwrap();
    assert!(json.contains("\"app\": \"taskboard\""));

    let mut dst = Db::open_in_memory().unwrap();
    let report = dst.import_json(&json).unwrap();
    assert!(report.created > 10, "{report:?}");
    assert_eq!(titles(&dst), titles(&src));
    let range = (at(2030, 1, 1, 0), at(2030, 2, 1, 0));
    assert_eq!(
        dst.occurrences(range.0, range.1).unwrap().len(),
        src.occurrences(range.0, range.1).unwrap().len()
    );
    assert_eq!(
        dst.day_overrides("2030-01-01", "2030-01-01").unwrap().len(),
        1
    );
    assert_eq!(
        dst.logged_time(range.0, range.1).unwrap().len(),
        src.logged_time(range.0, range.1).unwrap().len()
    );
    assert_eq!(
        dst.clients(false).unwrap()[0].time_zone.as_deref(),
        Some("Europe/London")
    );
    // The imported Inbox task landed in this database's Inbox.
    let inbox = dst.inbox_id().unwrap();
    assert_eq!(
        dst.list_tasks(&TaskQuery {
            project_id: Some(inbox),
            ..Default::default()
        })
        .unwrap()
        .total,
        1
    );

    // Importing again adds nothing.
    let again = dst.import_json(&json).unwrap();
    assert_eq!(again.created, 0);
    // One undo removes the whole first import... (second import made no changes)
    dst.undo().unwrap();
    assert!(dst.list_tasks(&TaskQuery::default()).unwrap().total == 0);
    assert!(dst.import_json("{\"app\":\"other\"}").is_err());
}

#[test]
fn csv_import_todoist_and_trello_shapes() {
    let mut db = Db::open_in_memory().unwrap();
    let todoist = "TYPE,CONTENT,DESCRIPTION,PRIORITY,DATE\n\
                   section,Planning,,,\n\
                   task,Write brief,Two pages,4,2030-01-08\n\
                   task,Email Sam,,1,\n";
    let r = db.import_tasks_csv(todoist, None).unwrap();
    assert_eq!((r.created, r.skipped), (2, 1));
    let brief = db
        .list_tasks(&TaskQuery {
            search: Some("brief".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(brief.rows[0].priority, Priority::Urgent);
    assert_eq!(brief.rows[0].due_date.as_deref(), Some("2030-01-08"));

    let trello = "Card Name,List Name,Labels,Due Date,Closed\n\
                  Homepage copy,Website,\"copy, review\",2030-01-09T17:00:00.000Z,false\n\
                  Old card,Website,,,true\n\
                  Logo,Branding,,someday,false\n";
    let r = db.import_tasks_csv(trello, None).unwrap();
    assert_eq!((r.created, r.projects_created), (3, 2));
    assert_eq!(r.warnings.len(), 1, "the unreadable date is reported");
    let done = db
        .list_tasks(&TaskQuery {
            categories: vec![Category::Done],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(done.rows[0].title, "Old card");
    let copy = db
        .list_tasks(&TaskQuery {
            tag: Some("review".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(copy.rows[0].project_name, "Website");
    db.undo().unwrap();
    assert_eq!(
        db.list_tasks(&TaskQuery::default()).unwrap().total,
        2,
        "one undo per import"
    );
    assert!(
        db.import_tasks_csv("Foo,Bar\n1,2\n", None).is_err(),
        "needs a title column"
    );

    let csv = db.export_tasks_csv(&TaskQuery::default()).unwrap();
    assert!(csv.starts_with("Title,Client,Project"));
    assert_eq!(csv.lines().count(), 3);
}

#[test]
fn ics_round_trip() {
    let src = workspace();
    let ics = src.export_ics().unwrap();
    assert!(
        ics.contains("BEGIN:VEVENT")
            && ics.contains("RRULE:FREQ=WEEKLY;BYDAY=MO")
            && ics.contains("EXDATE:")
    );
    assert!(ics.contains("BEGIN:VTODO") && ics.contains("SUMMARY:Launch"));
    assert!(ics.contains("LOCATION:Office\\, room 2"));

    let mut dst = Db::open_in_memory().unwrap();
    let r = dst.import_ics(&ics).unwrap();
    assert_eq!(r.created, 1, "{r:?}");
    let range = (at(2030, 1, 1, 0), at(2030, 2, 1, 0));
    let occ = dst.occurrences(range.0, range.1).unwrap();
    assert_eq!(
        occ.len(),
        3,
        "Mondays 7, 21, 28; the cancelled 14th stays cancelled (EXDATE)"
    );
    assert_eq!(occ[0].start_at, at(2030, 1, 7, 9));
    assert_eq!(occ[0].location, "Office, room 2");
    assert_eq!(
        dst.import_ics(&ics).unwrap().created,
        0,
        "same UID is skipped"
    );
    assert!(dst.import_ics("hello").is_err());
}

#[test]
fn backups_make_list_verify_and_restore() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("taskboard.db");
    let mut db = Db::open(&path).unwrap();
    db.create_task(NewTask {
        title: "Keep me".into(),
        ..Default::default()
    })
    .unwrap();
    let b = db.backup_now("manual").unwrap();
    assert_eq!(b.kind, "manual");
    assert!(db.ensure_daily_backup().unwrap().is_some());
    assert!(
        db.ensure_daily_backup().unwrap().is_none(),
        "one daily backup per day"
    );
    assert_eq!(db.list_backups().unwrap().len(), 2);

    db.create_task(NewTask {
        title: "Added later".into(),
        ..Default::default()
    })
    .unwrap();
    drop(db);
    taskboard_core::backup::restore_file(std::path::Path::new(&b.path), &path).unwrap();
    let db = Db::open(&path).unwrap();
    let all: Vec<String> = db
        .list_tasks(&TaskQuery::default())
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert_eq!(all, ["Keep me"]);
    assert!(Db::open_in_memory().unwrap().backup_now("manual").is_err());
}

#[test]
fn sample_workspace_adds_and_removes_cleanly() {
    let mut db = Db::open_in_memory().unwrap();
    let today = jiff::Zoned::now().date();
    let mine = db
        .create_task(NewTask {
            title: "My own task".into(),
            ..Default::default()
        })
        .unwrap();
    db.create_sample_workspace(today).unwrap();
    assert!(db.has_sample_workspace().unwrap());
    assert!(db.create_sample_workspace(today).is_err(), "only once");
    assert_eq!(db.clients(false).unwrap()[0].name, "Sample Co. (example)");
    assert!(db.list_tasks(&TaskQuery::default()).unwrap().total >= 10);
    let week = (
        taskboard_core::clock::now_ms(),
        taskboard_core::clock::now_ms() + 8 * 86_400_000,
    );
    assert!(db.occurrences(week.0, week.1).unwrap().len() >= 2);

    db.remove_sample_workspace().unwrap();
    assert!(!db.has_sample_workspace().unwrap());
    assert!(db.clients(false).unwrap().is_empty());
    let left: Vec<String> = db
        .list_tasks(&TaskQuery::default())
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert_eq!(left, ["My own task"], "only sample data was removed");
    assert!(db.task(mine.id).is_ok());
    db.undo().unwrap();
    assert!(db.has_sample_workspace().unwrap(), "removal is undoable");
}

#[test]
fn ics_import_skips_events_that_already_exist_here() {
    // Re-importing your own calendar (into the same database, or after a JSON
    // restore) must not duplicate meetings.
    let mut src = workspace();
    let ics = src.export_ics().unwrap();
    let r = src.import_ics(&ics).unwrap();
    assert_eq!(r.created, 0, "{r:?}");

    let json = src.export_json().unwrap();
    let mut restored = Db::open_in_memory().unwrap();
    restored.import_json(&json).unwrap();
    let range = (at(2030, 1, 1, 0), at(2030, 2, 1, 0));
    let before = restored.occurrences(range.0, range.1).unwrap().len();
    let r = restored.import_ics(&ics).unwrap();
    assert_eq!(r.created, 0, "{r:?}");
    assert_eq!(
        restored.occurrences(range.0, range.1).unwrap().len(),
        before
    );
}
