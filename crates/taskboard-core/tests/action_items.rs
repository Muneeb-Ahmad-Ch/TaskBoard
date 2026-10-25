//! Meeting notes: `- [ ]` action items become tasks.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

const NOTES: &str = "Agenda\n- [ ] Send quote\n- [x] Already done\n* [ ] Book venue (added as a task)\n  - [ ] Call Ann\nNot an item [ ] here\n";

fn titles(db: &Db) -> Vec<String> {
    let mut t: Vec<String> = db
        .list_tasks(&TaskQuery::default())
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    t.sort();
    t
}

#[test]
fn unchecked_items_become_tasks_once() {
    let mut db = Db::open_in_memory().unwrap();
    let project = db
        .create_project(NewProject {
            name: "Site".into(),
            ..Default::default()
        })
        .unwrap()
        .id;
    let made = db.tasks_from_notes(NOTES, Some(project)).unwrap();
    assert_eq!(made.created, 2);
    assert_eq!(titles(&db), ["Call Ann", "Send quote"]);
    let rows = db.list_tasks(&TaskQuery::default()).unwrap().rows;
    assert!(rows.iter().all(|r| r.project_id == project));
    assert!(made.notes.contains("- [ ] Send quote (added as a task)"));
    assert!(made.notes.contains("  - [ ] Call Ann (added as a task)"));
    assert!(made.notes.contains("- [x] Already done\n"));
    // Running it again on the new notes adds nothing.
    let again = db.tasks_from_notes(&made.notes, Some(project)).unwrap();
    assert_eq!(again.created, 0);
    assert_eq!(again.notes, made.notes);
    // One undo step takes both tasks back.
    db.undo().unwrap();
    assert!(titles(&db).is_empty());
}

#[test]
fn a_meeting_s_action_items_become_tasks_in_its_project() {
    let mut db = Db::open_in_memory().unwrap();
    let project = db
        .create_project(NewProject {
            name: "Site".into(),
            ..Default::default()
        })
        .unwrap()
        .id;
    let ev = db
        .create_event(NewEvent {
            kind: EventKind::Meeting,
            title: "Kickoff".into(),
            client_id: None,
            project_id: Some(project),
            task_id: None,
            start_at: Some(1_900_000_000_000),
            end_at: Some(1_900_000_000_000 + 3_600_000),
            all_day_start: None,
            all_day_end: None,
            time_zone: None,
            location: None,
            meeting_url: None,
            attendees: None,
            notes_md: Some(NOTES.into()),
            status: None,
            counts_as_time: None,
            flexible: None,
            recurrence_rule: None,
        })
        .unwrap();
    assert_eq!(db.tasks_from_event_notes(ev.id, None).unwrap(), 2);
    assert!(
        db.event(ev.id)
            .unwrap()
            .notes_md
            .contains("Send quote (added as a task)")
    );
    assert_eq!(db.tasks_from_event_notes(ev.id, None).unwrap(), 0);
    // Without a project, tasks go to the Inbox.
    let inbox = db.inbox_id().unwrap();
    let made = db.tasks_from_notes("- [ ] Loose end", None).unwrap();
    assert_eq!(made.created, 1);
    assert!(
        db.list_tasks(&TaskQuery {
            project_id: Some(inbox),
            ..Default::default()
        })
        .unwrap()
        .rows
        .iter()
        .any(|r| r.title == "Loose end")
    );
}
