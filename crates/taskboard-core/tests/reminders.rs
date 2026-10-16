#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::clock::now_ms;
use taskboard_core::*;

const MIN: i64 = 60_000;

fn db() -> Db {
    Db::open_in_memory().unwrap()
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
        meeting_url: Some("https://meet.example.com/x".into()),
        attendees: None,
        notes_md: None,
        status: None,
        counts_as_time: None,
        flexible: None,
        recurrence_rule: None,
    }
}

fn no_digest(db: &mut Db) {
    db.set_notify_settings(NotifySettings {
        digest: false,
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn meeting_reminders_fire_once_within_the_lead_window() {
    let mut db = db();
    no_digest(&mut db);
    let now = now_ms();
    db.create_event(meeting("Soon", now + 5 * MIN, now + 35 * MIN))
        .unwrap();
    db.create_event(meeting("Later", now + 30 * MIN, now + 60 * MIN))
        .unwrap();
    let cancelled = db
        .create_event(meeting("Off", now + 3 * MIN, now + 20 * MIN))
        .unwrap();
    db.update_event(
        cancelled.id,
        EventPatch {
            status: Some(EventStatus::Cancelled),
            ..Default::default()
        },
    )
    .unwrap();

    let pending = db.pending_notifications(now).unwrap();
    assert_eq!(pending.len(), 1, "{pending:?}");
    let n = &pending[0];
    assert_eq!(n.kind, NotificationKind::Meeting);
    assert!(n.title.starts_with("In 5 min: Soon"), "{}", n.title);
    assert_eq!(n.url.as_deref(), Some("https://meet.example.com/x"));

    db.mark_notified(std::slice::from_ref(&n.key), now).unwrap();
    assert!(db.pending_notifications(now).unwrap().is_empty());
    // 25 minutes later "Later" is within its 10-minute lead.
    let later = db.pending_notifications(now + 25 * MIN).unwrap();
    assert_eq!(
        later.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
        ["In 5 min: Later"]
    );
}

#[test]
fn timed_tasks_remind_at_their_due_time() {
    let mut db = db();
    no_digest(&mut db);
    let now = jiff::Zoned::now();
    if now.hour() >= 23 {
        return; // the due time would roll into tomorrow
    }
    let due = now.checked_add(jiff::Span::new().minutes(1)).unwrap();
    db.create_task(NewTask {
        title: "Call bank".into(),
        due_date: Some(due.date().to_string()),
        due_time: Some(format!("{:02}:{:02}", due.hour(), due.minute())),
        ..Default::default()
    })
    .unwrap();
    let at = now_ms();
    assert!(
        db.pending_notifications(at - 10 * MIN).unwrap().is_empty(),
        "not before the due time"
    );
    let fired = db.pending_notifications(at + 2 * MIN).unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, NotificationKind::Task);
}

#[test]
fn digest_and_master_switch() {
    let mut db = db();
    let now = jiff::Zoned::now();
    if now.hour() == 0 && now.minute() < 2 {
        return;
    }
    let earlier = now.checked_sub(jiff::Span::new().minutes(1)).unwrap();
    db.set_notify_settings(NotifySettings {
        digest_time: format!("{:02}:{:02}", earlier.hour(), earlier.minute()),
        ..Default::default()
    })
    .unwrap();
    db.create_task(NewTask {
        title: "Pay rent".into(),
        due_date: Some(now.date().to_string()),
        ..Default::default()
    })
    .unwrap();
    let pending = db.pending_notifications(now_ms()).unwrap();
    let digest = pending
        .iter()
        .find(|n| n.kind == NotificationKind::Digest)
        .expect("digest");
    assert!(digest.body.contains("1 due today"), "{}", digest.body);

    db.set_notify_settings(NotifySettings {
        enabled: false,
        ..Default::default()
    })
    .unwrap();
    assert!(db.pending_notifications(now_ms()).unwrap().is_empty());
    assert!(
        db.set_notify_settings(NotifySettings {
            digest_time: "25:00".into(),
            ..Default::default()
        })
        .is_err()
    );
    // Settings changes are not undoable work.
    assert!(db.history_state().unwrap().undo.as_deref() == Some("Create task “Pay rent”"));
}

#[test]
fn dashboard_counts_streaks_and_burndown() {
    let mut db = db();
    let today = jiff::Zoned::now().date();
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
    let mut ids = Vec::new();
    for i in 0..4 {
        ids.push(
            db.create_task(NewTask {
                title: format!("t{i}"),
                project_id: Some(site.id),
                ..Default::default()
            })
            .unwrap()
            .id,
        );
    }
    db.create_task(NewTask {
        title: "late".into(),
        project_id: Some(site.id),
        due_date: Some(today.yesterday().unwrap().to_string()),
        ..Default::default()
    })
    .unwrap();
    db.set_task_done(ids[0], true).unwrap();
    db.set_task_done(ids[1], true).unwrap();

    let d = db.dashboard(today).unwrap();
    assert_eq!(d.completed_this_week, 2);
    assert_eq!(d.overdue, 1);
    assert_eq!((d.streak_days, d.best_streak_days), (1, 1));
    assert_eq!(d.completions.len(), 14);
    assert_eq!(d.completions.last().unwrap().count, 2);
    let acme_row = d.clients.iter().find(|c| c.name == "Acme").unwrap();
    assert_eq!(
        (acme_row.open, acme_row.overdue, acme_row.done_this_week),
        (3, 1, 2)
    );
    // A client with only open work (no completions) must not break the summary.
    let quiet = db
        .create_client(NewClient {
            name: "Quiet".into(),
            ..Default::default()
        })
        .unwrap();
    let q = db
        .create_project(NewProject {
            name: "Q".into(),
            client_id: Some(quiet.id),
            ..Default::default()
        })
        .unwrap();
    db.create_task(NewTask {
        title: "open only".into(),
        project_id: Some(q.id),
        ..Default::default()
    })
    .unwrap();
    let d2 = db.dashboard(today).unwrap();
    let quiet_row = d2.clients.iter().find(|c| c.name == "Quiet").unwrap();
    assert_eq!((quiet_row.open, quiet_row.done_this_week), (1, 0));

    let p = &d.projects[0];
    assert_eq!((p.done, p.total, p.open), (2, 5, 3));
    assert_eq!(*p.burndown.last().unwrap(), 3);
}
