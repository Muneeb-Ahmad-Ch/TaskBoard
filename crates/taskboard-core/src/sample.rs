//! Onboarding sample workspace (SPEC §6.8): realistic, dated relative to
//! today, created and removed in one action each.

use jiff::ToSpan;
use jiff::civil::{Date, Time};
use rusqlite::OptionalExtension;
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};

use crate::error::{Result, invalid};
use crate::events::{EventKind, NewEvent, insert_event, local_zone};
use crate::id::Id;
use crate::model::{NewClient, NewProject, NewTask, Priority, clean_tz, int, text};
use crate::order::key_between;
use crate::projects::insert_project;
use crate::storage::Db;
use crate::tasks::insert_task;
use crate::tx::{Table, Tx};

const SAMPLE_KEY: &str = "sample_workspace";

#[derive(Debug, Default, Serialize, Deserialize)]
struct SampleIds {
    client: Option<Id>,
    projects: Vec<Id>,
    events: Vec<Id>,
    /// Sections the sample created (removed again if they end up empty).
    #[serde(default)]
    sections: Vec<Id>,
}

/// A live section with this name (case-insensitive), or a new one recorded
/// in `created` so removing the sample can take it away again.
fn sample_section(
    tx: &mut Tx<'_>,
    name: &str,
    item: &str,
    color: &str,
    created: &mut Vec<Id>,
) -> Result<Id> {
    let existing: Option<Id> = tx
        .conn()
        .query_row(
            "SELECT id FROM section WHERE deleted_at IS NULL AND name = ?1 COLLATE NOCASE
             ORDER BY sort_key LIMIT 1",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let last: Option<String> = tx.conn().query_row(
        "SELECT max(sort_key) FROM section WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let id = Id::new();
    tx.insert(
        Table::Section,
        &[
            ("id", id.into()),
            ("name", text(name)),
            ("item_name", text(item)),
            ("color", text(color)),
            ("sort_key", text(key_between(last.as_deref(), None)?)),
            ("created_at", int(tx.now())),
        ],
    )?;
    created.push(id);
    Ok(id)
}

fn ms(day: Date, h: i8, m: i8) -> Option<i64> {
    let (tz, _) = local_zone();
    tz.to_zoned(day.to_datetime(Time::new(h, m, 0, 0).ok()?))
        .ok()
        .map(|z| z.timestamp().as_millisecond())
}

fn insert_client(tx: &mut Tx<'_>, input: &NewClient) -> Result<Id> {
    let id = Id::new();
    let last: Option<String> = tx.conn().query_row(
        "SELECT max(sort_key) FROM client WHERE deleted_at IS NULL AND section_id IS ?1",
        [input.section_id.map_or(Value::Null, Value::from)],
        |r| r.get(0),
    )?;
    tx.insert(
        Table::Client,
        &[
            ("id", id.into()),
            (
                "section_id",
                input.section_id.map_or(Value::Null, Value::from),
            ),
            ("name", text(input.name.clone())),
            (
                "color",
                text(input.color.clone().unwrap_or_else(|| "190".into())),
            ),
            ("contact", text(input.contact.clone().unwrap_or_default())),
            ("notes_md", text(input.notes_md.clone().unwrap_or_default())),
            (
                "time_zone",
                input
                    .time_zone
                    .as_deref()
                    .map(clean_tz)
                    .transpose()?
                    .map_or(Value::Null, Value::Text),
            ),
            ("sort_key", text(key_between(last.as_deref(), None)?)),
            ("created_at", int(tx.now())),
        ],
    )?;
    Ok(id)
}

impl Db {
    pub fn has_sample_workspace(&self) -> Result<bool> {
        Ok(self.setting::<Option<SampleIds>>(SAMPLE_KEY)?.is_some())
    }

    /// Creates a sample client with projects, tasks and meetings.
    pub fn create_sample_workspace(&mut self, today: Date) -> Result<()> {
        if self.has_sample_workspace()? {
            return Err(invalid("the sample workspace is already there"));
        }
        let inbox = self.inbox_id()?;
        let day = |n: i64| today.checked_add(n.days()).unwrap_or(today).to_string();
        let monday = today
            .checked_add(((7 - i64::from(today.weekday().to_monday_zero_offset())) % 7).days())
            .unwrap_or(today);
        let tomorrow = today.tomorrow().unwrap_or(today);
        let mut ids = SampleIds::default();
        self.transact("Add sample workspace", |tx| {
            let clients_section =
                sample_section(tx, "Clients", "client", "230", &mut ids.sections)?;
            let personal_section = sample_section(tx, "Personal", "", "45", &mut ids.sections)?;
            let client = insert_client(
                tx,
                &NewClient {
                    name: "Sample Co. (example)".into(),
                    section_id: Some(clients_section),
                    color: Some("190".into()),
                    contact: Some("Alex Rivera · alex@example.com".into()),
                    notes_md: Some(
                        "A sample client. Remove the whole sample from Settings → Data.".into(),
                    ),
                    time_zone: Some("Europe/London".into()),
                },
            )?;
            ids.client = Some(client);
            let website = insert_project(
                tx,
                &NewProject {
                    name: "Website refresh".into(),
                    client_id: Some(client),
                    color: Some("190".into()),
                    ..Default::default()
                },
            )?;
            let retainer = insert_project(
                tx,
                &NewProject {
                    name: "Monthly retainer".into(),
                    client_id: Some(client),
                    color: Some("230".into()),
                    ..Default::default()
                },
            )?;
            let learning = insert_project(
                tx,
                &NewProject {
                    name: "Learning (sample)".into(),
                    section_id: Some(personal_section),
                    color: Some("45".into()),
                    ..Default::default()
                },
            )?;
            ids.projects = vec![website, retainer, learning];

            let task = |title: &str, project: Id| NewTask {
                title: title.into(),
                project_id: Some(project),
                ..Default::default()
            };
            let launch = insert_task(
                tx,
                &NewTask {
                    due_date: Some(day(5)),
                    priority: Some(Priority::High),
                    estimate_minutes: Some(240),
                    tags: vec!["design".into()],
                    ..task("Ship the new homepage", website)
                },
                inbox,
            )?;
            for sub in ["Hero copy", "Pricing section", "Mobile layout check"] {
                insert_task(
                    tx,
                    &NewTask {
                        parent_id: Some(launch),
                        ..task(sub, website)
                    },
                    inbox,
                )?;
            }
            insert_task(
                tx,
                &NewTask {
                    due_date: Some(day(0)),
                    due_time: Some("15:00".into()),
                    priority: Some(Priority::Urgent),
                    estimate_minutes: Some(30),
                    ..task("Send wireframes for review", website)
                },
                inbox,
            )?;
            insert_task(
                tx,
                &NewTask {
                    due_date: Some(day(-1)),
                    estimate_minutes: Some(60),
                    tags: vec!["bug".into()],
                    ..task("Fix contact form on Safari", website)
                },
                inbox,
            )?;
            insert_task(
                tx,
                &NewTask {
                    due_date: Some(day(2)),
                    progress: Some(50),
                    estimate_minutes: Some(90),
                    ..task("Draft October report", retainer)
                },
                inbox,
            )?;
            insert_task(
                tx,
                &NewTask {
                    recurrence_rule: Some("FREQ=WEEKLY;BYDAY=FR".into()),
                    estimate_minutes: Some(20),
                    tags: vec!["admin".into()],
                    ..task("Send weekly update", retainer)
                },
                inbox,
            )?;
            insert_task(
                tx,
                &NewTask {
                    due_date: Some(day(7)),
                    ..task("Read: the Rust book, chapter 10", learning)
                },
                inbox,
            )?;
            insert_task(
                tx,
                &task("Try the Board view: drag me to In Progress", learning),
                inbox,
            )?;

            let meeting = |title: &str, start: Option<i64>, minutes: i64| NewEvent {
                kind: EventKind::Meeting,
                title: title.into(),
                client_id: None,
                project_id: Some(retainer),
                task_id: None,
                start_at: start,
                end_at: start.map(|s| s + minutes * 60_000),
                all_day_start: None,
                all_day_end: None,
                time_zone: None,
                location: None,
                meeting_url: Some("https://meet.example.com/sample".into()),
                attendees: Some("Alex Rivera".into()),
                notes_md: Some("- [ ] Review last week\n- [ ] Plan next steps".into()),
                status: None,
                counts_as_time: None,
                flexible: None,
                recurrence_rule: None,
            };
            ids.events.push(insert_event(
                tx,
                &NewEvent {
                    recurrence_rule: Some("FREQ=WEEKLY;BYDAY=MO".into()),
                    ..meeting("Sample Co. check-in", ms(monday, 10, 0), 30)
                },
            )?);
            ids.events.push(insert_event(
                tx,
                &NewEvent {
                    kind: EventKind::Block,
                    project_id: Some(website),
                    meeting_url: None,
                    attendees: None,
                    notes_md: None,
                    ..meeting("Focus: homepage", ms(tomorrow, 14, 0), 120)
                },
            )?);
            let json = serde_json::to_string(&ids)?;
            tx.insert(
                Table::Setting,
                &[("key", text(SAMPLE_KEY)), ("value", text(json))],
            )
        })
    }

    /// Removes everything the sample created (as one undoable action).
    pub fn remove_sample_workspace(&mut self) -> Result<()> {
        let Some(ids) = self.setting::<Option<SampleIds>>(SAMPLE_KEY)? else {
            return Err(invalid("there is no sample workspace"));
        };
        self.transact("Remove sample workspace", |tx| {
            let now = tx.now();
            for e in &ids.events {
                let live: bool = tx.conn().query_row(
                    "SELECT EXISTS(SELECT 1 FROM event WHERE id = ?1 AND deleted_at IS NULL)",
                    [e],
                    |r| r.get(0),
                )?;
                if live {
                    tx.update(Table::Event, &[(*e).into()], &[("deleted_at", int(now))])?;
                }
            }
            for p in &ids.projects {
                let tasks: Vec<Id> = {
                    let mut stmt = tx.conn().prepare(
                        "SELECT id FROM task WHERE project_id = ?1 AND deleted_at IS NULL",
                    )?;
                    stmt.query_map([p], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for t in tasks {
                    tx.update(Table::Task, &[t.into()], &[("deleted_at", int(now))])?;
                }
                let live: bool = tx.conn().query_row(
                    "SELECT EXISTS(SELECT 1 FROM project WHERE id = ?1 AND deleted_at IS NULL)",
                    [p],
                    |r| r.get(0),
                )?;
                if live {
                    tx.update(Table::Project, &[(*p).into()], &[("deleted_at", int(now))])?;
                }
            }
            if let Some(c) = ids.client {
                let others: i64 = tx.conn().query_row(
                    "SELECT count(*) FROM project WHERE client_id = ?1 AND deleted_at IS NULL",
                    [c],
                    |r| r.get(0),
                )?;
                if others == 0 {
                    tx.update(Table::Client, &[c.into()], &[("deleted_at", int(now))])?;
                }
            }
            for s in &ids.sections {
                let used: bool = tx.conn().query_row(
                    "SELECT EXISTS(SELECT 1 FROM client WHERE section_id = ?1 AND deleted_at IS NULL)
                         OR EXISTS(SELECT 1 FROM project WHERE section_id = ?1 AND deleted_at IS NULL)",
                    [s],
                    |r| r.get(0),
                )?;
                if !used {
                    tx.update(Table::Section, &[(*s).into()], &[("deleted_at", int(now))])?;
                }
            }
            tx.delete(Table::Setting, &[text(SAMPLE_KEY)])
        })
    }
}
