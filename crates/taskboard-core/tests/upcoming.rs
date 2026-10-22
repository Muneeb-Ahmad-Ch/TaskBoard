//! The Reminders page: what will remind you (timed tasks, meetings) and what
//! repeats, including future repeats of a task that don't exist yet.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jiff::ToSpan;
use jiff::civil::{Date, Time};
use taskboard_core::*;

const MIN: i64 = 60_000;

fn today() -> Date {
    jiff::Zoned::now().date()
}

fn day(offset: i64) -> Date {
    today().checked_add(offset.days()).unwrap()
}

/// Local wall-clock moment in ms.
fn at(date: Date, h: i8, m: i8) -> i64 {
    jiff::tz::TimeZone::system()
        .to_zoned(date.to_datetime(Time::new(h, m, 0, 0).unwrap()))
        .unwrap()
        .timestamp()
        .as_millisecond()
}

fn task(
    db: &mut Db,
    title: &str,
    due: Option<Date>,
    time: Option<&str>,
    rule: Option<&str>,
) -> Task {
    let inbox = db.inbox_id().unwrap();
    db.create_task(NewTask {
        title: title.into(),
        project_id: Some(inbox),
        due_date: due.map(|d| d.to_string()),
        due_time: time.map(str::to_owned),
        recurrence_rule: rule.map(str::to_owned),
        ..Default::default()
    })
    .unwrap()
}

fn event(kind: EventKind, title: &str, start: i64, rule: Option<&str>) -> NewEvent {
    NewEvent {
        kind,
        title: title.into(),
        client_id: None,
        project_id: None,
        task_id: None,
        start_at: Some(start),
        end_at: Some(start + 30 * MIN),
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
        recurrence_rule: rule.map(str::to_owned),
    }
}

fn upcoming(db: &Db, days: i64, kinds: Vec<ReminderKind>) -> UpcomingReminders {
    db.upcoming_reminders(
        &ReminderQuery {
            from: None,
            days: Some(days),
            kinds,
        },
        clock::now_ms(),
    )
    .unwrap()
}

fn titles(u: &UpcomingReminders) -> Vec<String> {
    u.items.iter().map(|i| i.title.clone()).collect()
}

#[test]
fn a_timed_task_is_a_reminder_and_says_when_it_fires() {
    let mut db = Db::open_in_memory().unwrap();
    let t = task(&mut db, "Call the bank", Some(day(1)), Some("09:00"), None);
    task(&mut db, "Plain task", Some(day(1)), None, None);
    let done = task(&mut db, "Finished", Some(day(2)), Some("10:00"), None);
    db.set_task_done(done.id, true).unwrap();
    let u = upcoming(&db, 30, vec![]);
    assert!(u.reminders_on);
    assert_eq!(titles(&u), ["Call the bank"]);
    let i = &u.items[0];
    assert_eq!(i.kind, ReminderKind::Task);
    assert_eq!(i.task_id, Some(t.id));
    assert_eq!(i.date, day(1).to_string());
    assert_eq!(i.time.as_deref(), Some("09:00"));
    assert_eq!(i.at, at(day(1), 9, 0));
    // Default: tasks remind at their due time.
    assert_eq!(i.remind_at, Some(at(day(1), 9, 0) - u.task_lead_min * MIN));
    assert!(!i.projected && !i.overdue && i.repeat.is_none());
}

#[test]
fn a_monthly_task_shows_its_next_repeats_ahead_of_time() {
    let mut db = Db::open_in_memory().unwrap();
    let first = day(3);
    let rule = format!("FREQ=MONTHLY;BYMONTHDAY={}", first.day());
    let t = task(&mut db, "Pay rent", Some(first), None, Some(&rule));
    // 200 days: a repeat on the 29th–31st skips short months.
    let u = upcoming(&db, 200, vec![]);
    let rent: Vec<_> = u.items.iter().filter(|i| i.title == "Pay rent").collect();
    assert!(rent.len() >= 3, "{rent:?}");
    assert!(!rent[0].projected && rent[0].date == first.to_string());
    assert!(
        rent[1..]
            .iter()
            .all(|i| i.projected && i.task_id == Some(t.id))
    );
    assert!(
        rent[0]
            .repeat
            .as_deref()
            .unwrap()
            .starts_with("Every month")
    );
    // Untimed: it repeats, but no notification fires for it.
    assert!(rent.iter().all(|i| i.remind_at.is_none()));
    let dates: Vec<&str> = rent.iter().map(|i| i.date.as_str()).collect();
    let mut sorted = dates.clone();
    sorted.sort_unstable();
    assert_eq!(dates, sorted);
}

#[test]
fn a_repeat_counted_from_completion_shows_only_the_current_one() {
    let mut db = Db::open_in_memory().unwrap();
    task(
        &mut db,
        "Water plants",
        Some(day(1)),
        None,
        Some("AFTER:3D"),
    );
    let u = upcoming(&db, 60, vec![]);
    assert_eq!(titles(&u), ["Water plants"]);
    assert_eq!(
        u.items[0].repeat.as_deref(),
        Some("3 days after completion")
    );
}

#[test]
fn meetings_remind_ahead_and_time_blocks_only_when_asked() {
    let mut db = Db::open_in_memory().unwrap();
    let start = at(day(2), 14, 0);
    db.create_event(event(
        EventKind::Meeting,
        "Weekly sync",
        start,
        Some("FREQ=WEEKLY"),
    ))
    .unwrap();
    db.create_event(event(EventKind::Block, "Deep work", start, None))
        .unwrap();
    let u = upcoming(&db, 20, vec![]);
    let syncs: Vec<_> = u
        .items
        .iter()
        .filter(|i| i.title == "Weekly sync")
        .collect();
    assert!(syncs.len() >= 2);
    assert_eq!(syncs[0].kind, ReminderKind::Meeting);
    assert_eq!(syncs[0].remind_at, Some(start - u.meeting_lead_min * MIN));
    assert!(
        syncs[0]
            .repeat
            .as_deref()
            .unwrap()
            .starts_with("Every week")
    );
    assert!(!titles(&u).contains(&"Deep work".to_owned()));
    let with_blocks = upcoming(&db, 20, vec![ReminderKind::Block]);
    assert_eq!(titles(&with_blocks), ["Deep work"]);
}

#[test]
fn overdue_reminders_come_first() {
    let mut db = Db::open_in_memory().unwrap();
    task(&mut db, "Missed call", Some(day(-2)), Some("11:00"), None);
    task(&mut db, "Soon", Some(day(1)), Some("08:00"), None);
    let u = upcoming(&db, 7, vec![]);
    assert_eq!(titles(&u), ["Missed call", "Soon"]);
    assert!(u.items[0].overdue && !u.items[1].overdue);
}

#[test]
fn with_notifications_off_nothing_says_it_will_fire() {
    let mut db = Db::open_in_memory().unwrap();
    db.set_notify_settings(NotifySettings {
        enabled: false,
        ..Default::default()
    })
    .unwrap();
    task(&mut db, "Call", Some(day(1)), Some("09:00"), None);
    let u = upcoming(&db, 7, vec![]);
    assert!(!u.reminders_on);
    assert_eq!(u.items.len(), 1);
    assert_eq!(u.items[0].remind_at, None);
}

#[test]
fn the_window_is_bounded() {
    let db = Db::open_in_memory().unwrap();
    let r = db.upcoming_reminders(
        &ReminderQuery {
            from: None,
            days: Some(5000),
            kinds: vec![],
        },
        clock::now_ms(),
    );
    assert!(r.is_err());
}

#[test]
fn repeating_lists_every_series_with_its_next_date() {
    let mut db = Db::open_in_memory().unwrap();
    let rent = task(
        &mut db,
        "Pay rent",
        Some(day(3)),
        Some("09:00"),
        Some("FREQ=MONTHLY"),
    );
    task(&mut db, "Once", Some(day(3)), Some("09:00"), None);
    let closed = task(&mut db, "Old habit", Some(day(1)), None, Some("FREQ=DAILY"));
    db.set_task_archived(closed.id, true).unwrap();
    let start = at(day(1), 10, 0);
    let ev = db
        .create_event(event(
            EventKind::Meeting,
            "Standup",
            start,
            Some("FREQ=DAILY;COUNT=5"),
        ))
        .unwrap();
    let list = db.repeating(clock::now_ms()).unwrap();
    let names: Vec<&str> = list.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(names, ["Standup", "Pay rent"], "soonest first");
    let standup = &list[0];
    assert_eq!(standup.event_id, Some(ev.id));
    assert_eq!(standup.next_at, Some(start));
    assert!(standup.describe.starts_with("Every day"));
    let r = &list[1];
    assert_eq!(r.task_id, Some(rent.id));
    assert_eq!(r.kind, ReminderKind::Task);
    assert_eq!(r.next_date.as_deref(), Some(day(3).to_string().as_str()));
    assert_eq!(r.next_time.as_deref(), Some("09:00"));
    assert!(r.describe.starts_with("Every month"));
}

#[test]
fn a_finished_series_is_not_listed() {
    let mut db = Db::open_in_memory().unwrap();
    let start = at(day(-10), 10, 0);
    db.create_event(event(
        EventKind::Meeting,
        "Ended",
        start,
        Some("FREQ=DAILY;COUNT=2"),
    ))
    .unwrap();
    assert!(db.repeating(clock::now_ms()).unwrap().is_empty());
}
