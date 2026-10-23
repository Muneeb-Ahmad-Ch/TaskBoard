#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jiff::civil::{Time, date};
use taskboard_core::planning::local_ms;
use taskboard_core::*;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;

fn db() -> Db {
    Db::open_in_memory().unwrap()
}

fn at(y: i16, m: i8, d: i8, h: i8, min: i8) -> i64 {
    local_ms(date(y, m, d), Time::new(h, min, 0, 0).unwrap()).unwrap()
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

fn iv(a: &str, b: &str) -> Interval {
    Interval {
        start: a.into(),
        end: b.into(),
    }
}

/// A far-future week so "minimum notice" never interferes.
const MON: (i16, i8, i8) = (2030, 1, 7);

#[test]
fn working_hours_default_edit_and_undo() {
    let mut db = db();
    let week = db.working_hours().unwrap();
    assert_eq!(week[0].intervals, [iv("09:00", "17:00")]);
    assert!(week[5].intervals.is_empty(), "Saturday off by default");

    let mut new = week.clone();
    new[0].intervals = vec![iv("09:00", "13:00"), iv("14:00", "18:00")];
    new[4].intervals = vec![];
    db.set_working_hours(new).unwrap();
    let saved = db.working_hours().unwrap();
    assert_eq!(saved[0].intervals.len(), 2);
    assert!(saved[4].intervals.is_empty());

    let overlapping = vec![WorkingDay {
        weekday: 1,
        intervals: vec![iv("09:00", "12:00"), iv("11:00", "13:00")],
    }];
    assert!(db.set_working_hours(overlapping).is_err());

    db.undo().unwrap();
    assert_eq!(
        db.working_hours().unwrap()[0].intervals,
        [iv("09:00", "17:00")]
    );
}

#[test]
fn days_off_and_custom_days_change_capacity() {
    let mut db = db();
    let (y, m, d) = MON;
    db.set_day_override(DayOverride {
        date: "2030-01-08".into(),
        kind: OverrideKind::Off,
        intervals: vec![],
        note: "Eid".into(),
    })
    .unwrap();
    db.set_day_override(DayOverride {
        date: "2030-01-09".into(),
        kind: OverrideKind::Custom,
        intervals: vec![iv("10:00", "12:00")],
        note: String::new(),
    })
    .unwrap();
    db.create_event(meeting("Sync", at(y, m, d, 10, 0), at(y, m, d, 11, 30)))
        .unwrap();
    db.create_task(NewTask {
        title: "Report".into(),
        due_date: Some("2030-01-07".into()),
        estimate_minutes: Some(120),
        ..Default::default()
    })
    .unwrap();

    let plans = db.day_plans("2030-01-07", "2030-01-09").unwrap();
    assert_eq!(
        (
            plans[0].available_min,
            plans[0].meetings_min,
            plans[0].due_estimate_min
        ),
        (480, 90, 120)
    );
    assert!(plans[1].day_off && plans[1].available_min == 0 && plans[1].note == "Eid");
    assert_eq!(plans[2].available_min, 120);
    assert_eq!(plans[0].working.len(), 1);

    db.clear_day_override("2030-01-08").unwrap();
    assert_eq!(
        db.day_plans("2030-01-08", "2030-01-08").unwrap()[0].available_min,
        480
    );
}

#[test]
fn timer_starts_stops_and_only_one_runs() {
    let mut db = db();
    let t = db
        .create_task(NewTask {
            title: "Build".into(),
            ..Default::default()
        })
        .unwrap();
    let now = taskboard_core::clock::now_ms();
    db.log_time(NewTimeEntry {
        task_id: Some(t.id),
        started_at: now - HOUR,
        ..Default::default()
    })
    .unwrap();
    assert!(db.running_timer().unwrap().is_some());
    // Starting another stops the first.
    db.log_time(NewTimeEntry {
        task_id: Some(t.id),
        started_at: now,
        ..Default::default()
    })
    .unwrap();
    let running = db.running_timer().unwrap().unwrap();
    assert_eq!(running.started_at, now);
    assert!(db.stop_timer().unwrap());
    assert!(db.running_timer().unwrap().is_none());
    assert!(!db.stop_timer().unwrap());
    assert!(
        db.log_time(NewTimeEntry {
            started_at: now,
            ended_at: Some(now + HOUR),
            ..Default::default()
        })
        .is_err(),
        "needs an owner"
    );
}

#[test]
fn logged_time_includes_meetings_that_count_and_rolls_up_per_client() {
    let mut db = db();
    let (y, m, d) = MON;
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
    let task = db
        .create_task(NewTask {
            title: "Hero".into(),
            project_id: Some(site.id),
            ..Default::default()
        })
        .unwrap();
    db.log_time(NewTimeEntry {
        task_id: Some(task.id),
        started_at: at(y, m, d, 9, 0),
        ended_at: Some(at(y, m, d, 11, 0)),
        ..Default::default()
    })
    .unwrap();
    db.create_event(NewEvent {
        client_id: Some(acme.id),
        ..meeting("Acme call", at(y, m, d, 14, 0), at(y, m, d, 15, 0))
    })
    .unwrap();
    db.create_event(NewEvent {
        kind: EventKind::Block,
        client_id: Some(acme.id),
        ..meeting("Focus", at(y, m, d, 15, 0), at(y, m, d, 17, 0))
    })
    .unwrap();

    let day = (at(y, m, d, 0, 0), at(y, m, d + 1, 0, 0));
    let logged = db.logged_time(day.0, day.1).unwrap();
    assert_eq!(logged.len(), 2, "entry + counted meeting, not the block");
    assert_eq!(
        logged[0].client_name.as_deref(),
        Some("Acme"),
        "task → project → client"
    );

    let hours = db.hours_by_client(day.0, day.1).unwrap();
    assert_eq!(hours.len(), 1);
    assert_eq!((hours[0].planned_min, hours[0].logged_min), (180, 180));
    assert_eq!(
        db.day_plans("2030-01-07", "2030-01-07").unwrap()[0].logged_min,
        180
    );
}

#[test]
fn slot_finder_respects_meetings_buffers_and_hours() {
    let mut db = db();
    let (y, m, d) = MON;
    db.create_event(meeting("Busy", at(y, m, d, 10, 0), at(y, m, d, 12, 0)))
        .unwrap();
    let q = FindSlots {
        from_date: "2030-01-07".into(),
        to_date: "2030-01-07".into(),
        duration_min: 60,
        buffer_min: Some(15),
        earliest: None,
        latest: None,
        blocks_flexible: None,
        max_meetings_per_day: None,
        min_notice_min: Some(0),
        client_id: None,
    };
    let r = db.find_slots(&q).unwrap();
    let windows: Vec<(i64, i64)> = r.days[0]
        .windows
        .iter()
        .map(|s| (s.start_at, s.end_at))
        .collect();
    assert_eq!(
        windows,
        [(at(y, m, d, 12, 15), at(y, m, d, 17, 0))],
        "09:00–09:45 is too short"
    );
    let first = &r.days[0].slots[0];
    assert_eq!(
        (first.start_at, first.end_at),
        (at(y, m, d, 12, 15), at(y, m, d, 13, 15))
    );

    // Mornings only → nothing left that day.
    let mornings = FindSlots {
        latest: Some("12:00".into()),
        ..q.clone()
    };
    assert!(db.find_slots(&mornings).unwrap().days[0].slots.is_empty());

    // Meeting cap of 1 → the day is full.
    let capped = FindSlots {
        max_meetings_per_day: Some(1),
        ..q.clone()
    };
    assert!(db.find_slots(&capped).unwrap().days[0].windows.is_empty());

    // Weekend has no working hours.
    let sat = FindSlots {
        from_date: "2030-01-12".into(),
        to_date: "2030-01-12".into(),
        ..q.clone()
    };
    assert!(db.find_slots(&sat).unwrap().days[0].windows.is_empty());
}

#[test]
fn slot_finder_intersects_with_client_hours_in_their_zone() {
    let mut db = db();
    // A client in this machine's zone who only works 13:00–15:00: every slot
    // must land inside that window (deterministic wherever the test runs).
    let (_, zone) = taskboard_core::events::local_zone();
    let c = db
        .create_client(NewClient {
            name: "Afternoons Ltd".into(),
            time_zone: Some(zone.clone()),
            ..Default::default()
        })
        .unwrap();
    db.update_client(
        c.id,
        ClientPatch {
            work_start: Some(Some("13:00".into())),
            work_end: Some(Some("15:00".into())),
            ..Default::default()
        },
    )
    .unwrap();
    let q = FindSlots {
        from_date: "2030-01-07".into(),
        to_date: "2030-01-11".into(),
        duration_min: 30,
        buffer_min: None,
        earliest: None,
        latest: None,
        blocks_flexible: None,
        max_meetings_per_day: None,
        min_notice_min: Some(0),
        client_id: Some(c.id),
    };
    let r = db.find_slots(&q).unwrap();
    assert_eq!(r.client_time_zone.as_deref(), Some(zone.as_str()));
    let (y, m, d) = MON;
    let monday = &r.days[0];
    assert_eq!(
        monday
            .windows
            .iter()
            .map(|w| (w.start_at, w.end_at))
            .collect::<Vec<_>>(),
        [(at(y, m, d, 13, 0), at(y, m, d, 15, 0))]
    );
    assert_eq!(r.days.iter().filter(|d| !d.slots.is_empty()).count(), 5);
    assert!(
        db.find_slots(&FindSlots {
            duration_min: 1,
            ..q.clone()
        })
        .is_err()
    );
}

#[test]
fn day_plan_estimates_skip_archived_work_and_count_subtasks_once() {
    let mut db = Db::open_in_memory().unwrap();
    let inbox = db.inbox_id().unwrap();
    let day = "2031-03-04";
    let mk = |db: &mut Db, title: &str, est: i64, parent: Option<Id>| {
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(inbox),
            parent_id: parent,
            due_date: Some(day.into()),
            estimate_minutes: Some(est),
            ..Default::default()
        })
        .unwrap()
    };
    mk(&mut db, "Plain", 30, None);
    let old = mk(&mut db, "Archived", 60, None);
    db.set_task_archived(old.id, true).unwrap();
    let parent = mk(&mut db, "Parent", 120, None);
    mk(&mut db, "Child", 45, Some(parent.id));
    let plans = db.day_plans(day, day).unwrap();
    assert_eq!(plans[0].due_estimate_min, 30 + 45);
}
