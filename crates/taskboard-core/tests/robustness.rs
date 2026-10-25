//! Hostile or odd text must give an error, never a panic: a panic while the
//! app holds the database lock breaks every later command until a restart.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

#[test]
fn repeat_rules_with_non_ascii_text_are_refused() {
    for rule in [
        "FREQ=WEEKLY;BYDAY=éM",
        "FREQ=WEEKLY;BYDAY=1é",
        "FREQ=DAILY;UNTIL=2026é021T20000Z",
        "FREQ=DAILY;UNTIL=é",
        "FREQ=MONTHLY;BYMONTHDAY=é",
    ] {
        assert!(rrule::Rule::parse(rule).is_err(), "{rule}");
    }
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    let r = db.create_task(NewTask {
        title: "x".into(),
        project_id: Some(inbox),
        recurrence_rule: Some("FREQ=WEEKLY;BYDAY=éM".into()),
        ..Default::default()
    });
    assert!(r.is_err());
    // The database still works afterwards.
    assert!(db.list_tasks(&TaskQuery::default()).is_ok());
}

#[test]
fn sort_keys_with_non_ascii_text_are_refused() {
    for key in ["aé", "é", "b0é", "Zé0"] {
        assert!(order::validate(key).is_err(), "{key}");
    }
}
