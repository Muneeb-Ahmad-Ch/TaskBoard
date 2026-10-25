//! Meeting notes: `- [ ]` action items become tasks (in the meeting's project,
//! else the Inbox), as one undoable step. Each item gets " (added as a task)"
//! appended so running it again never adds it twice.

use serde::Serialize;
use ts_rs::TS;

use crate::error::Result;
use crate::id::Id;
use crate::model::{NewTask, int, text};
use crate::storage::Db;
use crate::tasks::insert_task;
use crate::tx::{Table, Tx};

const MARK: &str = " (added as a task)";
const MARKERS: [&str; 3] = ["- [ ] ", "* [ ] ", "+ [ ] "];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct NotesToTasks {
    /// Tasks made.
    pub created: usize,
    /// The notes with each item marked as added.
    pub notes: String,
}

/// The title of an unchecked item not added yet, if `line` is one.
fn open_item(line: &str) -> Option<&str> {
    let rest = MARKERS
        .iter()
        .find_map(|m| line.trim_start().strip_prefix(m))?;
    let title = rest.trim();
    (!title.is_empty() && !title.ends_with(MARK.trim_start())).then_some(title)
}

/// Makes the tasks inside `tx`; returns the marked notes and how many.
fn make_tasks(
    tx: &mut Tx<'_>,
    notes: &str,
    project: Option<Id>,
    inbox: Id,
) -> Result<NotesToTasks> {
    let mut created = 0;
    let mut lines = Vec::new();
    for line in notes.split('\n') {
        match open_item(line.trim_end_matches('\r')) {
            Some(title) => {
                let title: String = title.chars().take(300).collect();
                insert_task(
                    tx,
                    &NewTask {
                        title,
                        project_id: project,
                        ..Default::default()
                    },
                    inbox,
                )?;
                created += 1;
                let line = line.trim_end_matches('\r').trim_end();
                lines.push(format!("{line}{MARK}"));
            }
            None => lines.push(line.to_owned()),
        }
    }
    Ok(NotesToTasks {
        created,
        notes: lines.join("\n"),
    })
}

fn count_open(notes: &str) -> usize {
    notes
        .split('\n')
        .filter(|l| open_item(l.trim_end_matches('\r')).is_some())
        .count()
}

fn label(n: usize) -> String {
    if n == 1 {
        "Make a task from notes".to_owned()
    } else {
        format!("Make {n} tasks from notes")
    }
}

impl Db {
    /// Makes a task of each unchecked `- [ ]` item in `notes` (in `project`,
    /// else the Inbox) and returns the notes with those items marked.
    pub fn tasks_from_notes(&mut self, notes: &str, project: Option<Id>) -> Result<NotesToTasks> {
        let n = count_open(notes);
        if n == 0 {
            return Ok(NotesToTasks {
                created: 0,
                notes: notes.to_owned(),
            });
        }
        if let Some(p) = project {
            self.project(p)?;
        }
        let inbox = self.inbox_id()?;
        self.transact(&label(n), |tx| make_tasks(tx, notes, project, inbox))
    }

    /// The same for a meeting's stored notes (one occurrence's own notes when
    /// `original_start` is given and it has some), saving the marked notes in
    /// the same undoable step. Returns how many tasks were made.
    pub fn tasks_from_event_notes(
        &mut self,
        event: Id,
        original_start: Option<i64>,
    ) -> Result<usize> {
        let e = self.event(event)?;
        let own = match original_start {
            Some(at) => self.occurrence_notes(event, at)?,
            None => None,
        };
        let notes = own.clone().unwrap_or_else(|| e.notes_md.clone());
        let n = count_open(&notes);
        if n == 0 {
            return Ok(0);
        }
        let inbox = self.inbox_id()?;
        self.transact(&label(n), |tx| {
            let made = make_tasks(tx, &notes, e.project_id, inbox)?;
            let _changed = match (original_start, own) {
                (Some(at), Some(_)) => tx.update(
                    Table::EventException,
                    &[event.into(), int(at)],
                    &[("notes_md", text(made.notes.clone()))],
                )?,
                _ => tx.update(
                    Table::Event,
                    &[event.into()],
                    &[("notes_md", text(made.notes.clone()))],
                )?,
            };
            Ok(made.created)
        })
    }
}
