//! Table and Board filters (priority, column, tags, project, has a date,
//! added since) and sorting by every table column.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use taskboard_core::*;

/// Title, project, column, priority, tags, due, estimate, progress.
type Spec<'a> = (
    &'a str,
    Id,
    &'a str,
    Priority,
    &'a [&'a str],
    Option<&'a str>,
    Option<i64>,
    Option<i64>,
);

struct Fixture {
    db: Db,
    web: Id,
    app: Id,
}

/// Two projects; tasks with a spread of priorities, columns, tags, dates.
fn fixture() -> Fixture {
    let mut db = Db::open_in_memory().unwrap();
    let acme = db
        .create_client(NewClient {
            name: "Acme".into(),
            ..Default::default()
        })
        .unwrap();
    let web = db
        .create_project(NewProject {
            name: "Website".into(),
            client_id: Some(acme.id),
            ..Default::default()
        })
        .unwrap()
        .id;
    let app = db
        .create_project(NewProject {
            name: "App".into(),
            ..Default::default()
        })
        .unwrap()
        .id;
    let col = |db: &Db, project: Id, name: &str| {
        db.statuses(project)
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap()
            .id
    };
    let specs: [Spec; 5] = [
        (
            "Alpha",
            web,
            "To Do",
            Priority::High,
            &["design"],
            Some("2026-01-05"),
            Some(30),
            None,
        ),
        (
            "Bravo",
            web,
            "Waiting",
            Priority::Low,
            &["client", "design"],
            None,
            Some(120),
            Some(40),
        ),
        (
            "Charlie",
            app,
            "In Progress",
            Priority::Urgent,
            &[],
            Some("2026-01-02"),
            None,
            Some(70),
        ),
        (
            "Delta",
            app,
            "Done",
            Priority::None,
            &["admin"],
            None,
            Some(15),
            None,
        ),
        (
            "Echo",
            web,
            "Backlog",
            Priority::Med,
            &["client"],
            Some("2026-01-09"),
            None,
            None,
        ),
    ];
    for (title, project, status, priority, tags, due, estimate, progress) in specs {
        let status_id = Some(col(&db, project, status));
        db.create_task(NewTask {
            title: title.into(),
            project_id: Some(project),
            status_id,
            priority: Some(priority),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            due_date: due.map(str::to_owned),
            estimate_minutes: estimate,
            progress,
            ..Default::default()
        })
        .unwrap();
    }
    Fixture { db, web, app }
}

fn titles(db: &Db, q: TaskQuery) -> Vec<String> {
    db.list_tasks(&q)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect()
}

fn sorted(db: &Db, sort: TaskSort, descending: bool) -> Vec<String> {
    titles(
        db,
        TaskQuery {
            sort,
            descending,
            ..Default::default()
        },
    )
}

fn set(v: Vec<String>) -> Vec<String> {
    let mut v = v;
    v.sort();
    v
}

#[test]
fn filter_by_several_priorities() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            priorities: vec![Priority::High, Priority::Urgent],
            ..Default::default()
        },
    );
    assert_eq!(set(got), ["Alpha", "Charlie"]);
}

#[test]
fn filter_by_several_columns_ignoring_case() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            status_names: vec!["waiting".into(), "In Progress".into()],
            ..Default::default()
        },
    );
    assert_eq!(set(got), ["Bravo", "Charlie"]);
}

#[test]
fn filter_by_tags_matches_any_of_them() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            tags: vec!["#Admin".into(), "client".into()],
            ..Default::default()
        },
    );
    assert_eq!(set(got), ["Bravo", "Delta", "Echo"]);
}

#[test]
fn filter_by_several_projects() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            project_ids: vec![f.app],
            ..Default::default()
        },
    );
    assert_eq!(set(got), ["Charlie", "Delta"]);
    let both = titles(
        &f.db,
        TaskQuery {
            project_ids: vec![f.app, f.web],
            ..Default::default()
        },
    );
    assert_eq!(both.len(), 5);
}

#[test]
fn filter_to_tasks_with_a_due_date() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            dated: true,
            ..Default::default()
        },
    );
    assert_eq!(set(got), ["Alpha", "Charlie", "Echo"]);
}

#[test]
fn filters_combine() {
    let f = fixture();
    let got = titles(
        &f.db,
        TaskQuery {
            tags: vec!["design".into()],
            priorities: vec![Priority::Low, Priority::Urgent],
            ..Default::default()
        },
    );
    assert_eq!(got, ["Bravo"]);
}

#[test]
fn rows_say_when_the_task_was_added_and_filter_by_it() {
    let f = fixture();
    let rows = f.db.list_tasks(&TaskQuery::default()).unwrap().rows;
    let now = clock::now_ms();
    assert!(
        rows.iter()
            .all(|r| r.created_at > now - 60_000 && r.created_at <= now)
    );
    let later = titles(
        &f.db,
        TaskQuery {
            created_from: Some(now + 60_000),
            ..Default::default()
        },
    );
    assert!(later.is_empty());
    let before = titles(
        &f.db,
        TaskQuery {
            created_to: Some(now - 60_000),
            ..Default::default()
        },
    );
    assert!(before.is_empty());
    let all = titles(
        &f.db,
        TaskQuery {
            created_from: Some(now - 60_000),
            created_to: Some(now + 60_000),
            ..Default::default()
        },
    );
    assert_eq!(all.len(), 5);
}

#[test]
fn sort_by_column_follows_the_board_order() {
    let f = fixture();
    // Backlog and To Do (to do), In Progress (active), Waiting… by board order.
    assert_eq!(
        sorted(&f.db, TaskSort::Status, false),
        ["Echo", "Alpha", "Charlie", "Bravo", "Delta"]
    );
    assert_eq!(
        sorted(&f.db, TaskSort::Status, true),
        ["Delta", "Bravo", "Charlie", "Alpha", "Echo"]
    );
}

#[test]
fn sort_by_project_puts_group_then_project_names_in_order() {
    let f = fixture();
    // "App" has no group (empty), "Acme / Website" comes after.
    let got = sorted(&f.db, TaskSort::Project, false);
    assert_eq!(&got[..2], ["Charlie", "Delta"]);
    assert_eq!(set(got[2..].to_vec()), ["Alpha", "Bravo", "Echo"]);
}

#[test]
fn sort_by_progress() {
    let f = fixture();
    // Delta is done (100), Charlie 70, Bravo 40, Alpha and Echo 0.
    let got = sorted(&f.db, TaskSort::Progress, true);
    assert_eq!(&got[..3], ["Delta", "Charlie", "Bravo"]);
    let up = sorted(&f.db, TaskSort::Progress, false);
    assert_eq!(&up[3..], ["Charlie", "Delta"]);
}

#[test]
fn sort_by_estimate_keeps_unestimated_last_both_ways() {
    let f = fixture();
    let up = sorted(&f.db, TaskSort::Estimate, false);
    assert_eq!(&up[..3], ["Delta", "Alpha", "Bravo"]);
    let down = sorted(&f.db, TaskSort::Estimate, true);
    assert_eq!(&down[..3], ["Bravo", "Alpha", "Delta"]);
    assert_eq!(set(down[3..].to_vec()), ["Charlie", "Echo"]);
}

#[test]
fn sort_by_tags_keeps_untagged_last_both_ways() {
    let f = fixture();
    // First tag alphabetically: Delta admin, Bravo client, Echo client, Alpha design.
    let up = sorted(&f.db, TaskSort::Tags, false);
    assert_eq!(up[0], "Delta");
    assert_eq!(set(up[1..3].to_vec()), ["Bravo", "Echo"]);
    assert_eq!(&up[3..], ["Alpha", "Charlie"]);
    let down = sorted(&f.db, TaskSort::Tags, true);
    assert_eq!(down[0], "Alpha");
    assert_eq!(down[4], "Charlie");
}

#[test]
fn every_sort_pages_without_gaps_or_repeats() {
    let f = fixture();
    for sort in [
        TaskSort::Manual,
        TaskSort::Due,
        TaskSort::Priority,
        TaskSort::Created,
        TaskSort::Updated,
        TaskSort::Title,
        TaskSort::Status,
        TaskSort::Project,
        TaskSort::Progress,
        TaskSort::Estimate,
        TaskSort::Tags,
    ] {
        for descending in [false, true] {
            let mut seen = Vec::new();
            for offset in 0..5 {
                seen.extend(titles(
                    &f.db,
                    TaskQuery {
                        sort,
                        descending,
                        offset,
                        limit: 1,
                        ..Default::default()
                    },
                ));
            }
            assert_eq!(
                set(seen),
                ["Alpha", "Bravo", "Charlie", "Delta", "Echo"],
                "{sort:?} {descending}"
            );
        }
    }
}

#[test]
fn tag_names_lists_tags_on_live_tasks_most_used_first() {
    let mut f = fixture();
    // client ×2, design ×2, admin ×1 (Delta). Deleting Delta drops admin.
    let names = f.db.tag_names().unwrap();
    assert_eq!(names, ["client", "design", "admin"]);
    let delta =
        f.db.list_tasks(&TaskQuery {
            search: Some("Delta".into()),
            ..Default::default()
        })
        .unwrap()
        .rows[0]
            .id;
    f.db.delete_task(delta).unwrap();
    assert_eq!(f.db.tag_names().unwrap(), ["client", "design"]);
}
