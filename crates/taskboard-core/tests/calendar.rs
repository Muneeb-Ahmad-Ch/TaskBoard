#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jiff::civil::date;
use jiff::tz::TimeZone;
use taskboard_core::*;

const HOUR: i64 = 3_600_000;
const DAY: i64 = 86_400_000;

fn db() -> Db {
    Db::open_in_memory().unwrap()
}

/// Local wall time → UTC ms in the system zone (what the app does).
fn at(y: i16, m: i8, d: i8, h: i8, min: i8) -> i64 {
    TimeZone::system()
        .to_zoned(date(y, m, d).at(h, min, 0, 0))
        .unwrap()
        .timestamp()
        .as_millisecond()
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
        location: None,
        meeting_url: None,
        attendees: None,
        notes_md: None,
        status: None,
        counts_as_time: None,
        flexible: None,
        recurrence_rule: None,
    }
}

#[test]
fn single_and_all_day_events_appear_in_range() {
    let mut db = db();
    let s = at(2026, 10, 20, 15, 0);
    db.create_event(meeting("Acme sync", s, s + HOUR)).unwrap();
    db.create_event(NewEvent {
        start_at: None,
        end_at: None,
        all_day_start: Some("2026-10-22".into()),
        all_day_end: Some("2026-10-23".into()),
        ..meeting("Offsite", 0, 0)
    })
    .unwrap();
    let week = db
        .occurrences(at(2026, 10, 19, 0, 0), at(2026, 10, 26, 0, 0))
        .unwrap();
    let titles: Vec<&str> = week.iter().map(|o| o.title.as_str()).collect();
    assert_eq!(titles, ["Acme sync", "Offsite"]);
    assert!(week[1].all_day);
    assert_eq!(week[1].end_at - week[1].start_at, 2 * DAY);
    assert!(
        db.occurrences(at(2026, 10, 27, 0, 0), at(2026, 11, 2, 0, 0))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn recurring_meetings_expand_and_exceptions_apply() {
    let mut db = db();
    let s = at(2026, 10, 19, 9, 30); // Monday
    let standup = db
        .create_event(NewEvent {
            recurrence_rule: Some("FREQ=WEEKLY;BYDAY=MO,WE,FR".into()),
            ..meeting("Stand-up", s, s + 15 * 60_000)
        })
        .unwrap();
    assert_eq!(
        standup.recurrence_text.as_deref(),
        Some("Every week on Mon, Wed, Fri")
    );
    let range = (at(2026, 10, 19, 0, 0), at(2026, 11, 2, 0, 0));
    let all = db.occurrences(range.0, range.1).unwrap();
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|o| o.recurring));

    // Move Wednesday's to 11:00 and cancel Friday's.
    let wed = all[1].original_start;
    let fri = all[2].original_start;
    db.edit_occurrence(
        standup.id,
        wed,
        OccurrencePatch {
            start_at: Some(at(2026, 10, 21, 11, 0)),
            end_at: Some(at(2026, 10, 21, 11, 15)),
            ..Default::default()
        },
    )
    .unwrap();
    db.edit_occurrence(
        standup.id,
        fri,
        OccurrencePatch {
            cancelled: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let after = db.occurrences(range.0, range.1).unwrap();
    assert_eq!(after.len(), 5);
    let moved = after.iter().find(|o| o.original_start == wed).unwrap();
    assert_eq!(moved.start_at, at(2026, 10, 21, 11, 0));
    assert!(moved.modified);

    db.undo().unwrap();
    assert_eq!(
        db.occurrences(range.0, range.1).unwrap().len(),
        6,
        "undo restores the cancelled one"
    );
}

#[test]
fn this_and_following_splits_the_series() {
    let mut db = db();
    let s = at(2026, 10, 19, 10, 0);
    let sync = db
        .create_event(NewEvent {
            recurrence_rule: Some("FREQ=WEEKLY".into()),
            ..meeting("Client sync", s, s + HOUR)
        })
        .unwrap();
    let third = at(2026, 11, 2, 10, 0);
    let later = db
        .split_series(
            sync.id,
            third,
            EventPatch {
                title: Some("Client sync (new time)".into()),
                start_at: Some(at(2026, 11, 2, 14, 0)),
                end_at: Some(at(2026, 11, 2, 15, 0)),
                ..Default::default()
            },
        )
        .unwrap();
    let occ = db
        .occurrences(at(2026, 10, 19, 0, 0), at(2026, 11, 17, 0, 0))
        .unwrap();
    let titles: Vec<&str> = occ.iter().map(|o| o.title.as_str()).collect();
    // Oct 19, Oct 26 from the old series; Nov 2, 9, 16 from the new one.
    assert_eq!(
        titles,
        [
            "Client sync",
            "Client sync",
            "Client sync (new time)",
            "Client sync (new time)",
            "Client sync (new time)"
        ]
    );
    assert_eq!(occ[2].event_id, later.id);
    assert!(
        db.event(sync.id)
            .unwrap()
            .recurrence_rule
            .unwrap()
            .contains("UNTIL=20261101")
    );
}

#[test]
fn meetings_inherit_the_clients_work_time_default_and_project_implies_client() {
    let mut db = db();
    let acme = db
        .create_client(NewClient {
            name: "Acme".into(),
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
    let s = at(2026, 10, 20, 9, 0);
    let m = db
        .create_event(NewEvent {
            project_id: Some(site.id),
            ..meeting("Kickoff", s, s + HOUR)
        })
        .unwrap();
    assert_eq!(m.client_id, Some(acme.id));
    assert!(m.counts_as_time);
    let occ = &db.occurrences(s - HOUR, s + 2 * HOUR).unwrap()[0];
    assert_eq!(
        (occ.client_name.as_deref(), occ.project_name.as_deref()),
        (Some("Acme"), Some("Site"))
    );
    let block = db
        .create_event(NewEvent {
            kind: EventKind::Block,
            ..meeting("Deep work", s, s + HOUR)
        })
        .unwrap();
    assert!(!block.counts_as_time && block.flexible);
}

#[test]
fn validation() {
    let mut db = db();
    let s = at(2026, 10, 20, 9, 0);
    assert!(db.create_event(meeting("Backwards", s, s - HOUR)).is_err());
    assert!(
        db.create_event(meeting("Too long", s, s + 20 * DAY))
            .is_err()
    );
    assert!(
        db.create_event(NewEvent {
            recurrence_rule: Some("FREQ=SOMETIMES".into()),
            ..meeting("x", s, s + HOUR)
        })
        .is_err()
    );
    let one = db.create_event(meeting("One-off", s, s + HOUR)).unwrap();
    assert!(
        db.edit_occurrence(one.id, s, OccurrencePatch::default())
            .is_err()
    );
    assert!(db.occurrences(s, s - 1).is_err());
}

#[test]
fn edit_and_delete_events_are_undoable() {
    let mut db = db();
    let s = at(2026, 10, 20, 9, 0);
    let m = db.create_event(meeting("Call", s, s + HOUR)).unwrap();
    db.update_event(
        m.id,
        EventPatch {
            start_at: Some(s + HOUR),
            end_at: Some(s + 2 * HOUR),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.event(m.id).unwrap().start_at, Some(s + HOUR));
    db.delete_event(m.id).unwrap();
    assert!(db.event(m.id).is_err());
    db.undo().unwrap();
    db.undo().unwrap();
    assert_eq!(db.event(m.id).unwrap().start_at, Some(s));
}
