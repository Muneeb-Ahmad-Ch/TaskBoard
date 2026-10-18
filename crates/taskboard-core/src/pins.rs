//! Pinned projects: a short list at the top of the sidebar, in the user's
//! order. Stored as a setting (not an undo step, like other preferences), so
//! the app and connected assistants share it.

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::storage::Db;

const KEY: &str = "pinned_projects";

impl Db {
    /// Pinned projects in order: live, unarchived ones only. An archived
    /// project stays in the stored list and is pinned again when restored.
    pub fn pinned_projects(&self) -> Result<Vec<Id>> {
        let stored: Vec<Id> = self.setting(KEY)?;
        let mut stmt = self.conn().prepare(
            "SELECT EXISTS(SELECT 1 FROM project WHERE id = ?1 AND deleted_at IS NULL
                             AND archived_at IS NULL AND is_inbox = 0)",
        )?;
        let mut out = Vec::with_capacity(stored.len());
        for id in stored {
            if stmt.query_row([id], |r| r.get::<_, bool>(0))? {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// Replaces the pinned list (duplicates, deleted projects and the Inbox
    /// are dropped). Returns the list as `pinned_projects` shows it.
    pub fn set_pinned_projects(&mut self, ids: Vec<Id>) -> Result<Vec<Id>> {
        let mut clean: Vec<Id> = Vec::with_capacity(ids.len());
        {
            let mut stmt = self.conn().prepare(
                "SELECT EXISTS(SELECT 1 FROM project WHERE id = ?1 AND deleted_at IS NULL
                                 AND is_inbox = 0)",
            )?;
            for id in ids {
                if !clean.contains(&id) && stmt.query_row([id], |r| r.get::<_, bool>(0))? {
                    clean.push(id);
                }
            }
        }
        self.set_setting(KEY, &clean)?;
        self.pinned_projects()
    }

    /// Pins a project before `before` (None = at the end); pinning a pinned
    /// project moves it.
    pub fn pin_project(&mut self, id: Id, before: Option<Id>) -> Result<Vec<Id>> {
        let project = self.project(id)?;
        if project.is_inbox {
            return Err(invalid(
                "the Inbox is always at the top; it can't be pinned",
            ));
        }
        if project.archived_at.is_some() {
            return Err(Error::Conflict(
                "restore the project before pinning it".into(),
            ));
        }
        let mut list: Vec<Id> = self.setting(KEY)?;
        list.retain(|p| *p != id);
        let at = before
            .and_then(|b| list.iter().position(|p| *p == b))
            .unwrap_or(list.len());
        list.insert(at, id);
        self.set_pinned_projects(list)
    }

    pub fn unpin_project(&mut self, id: Id) -> Result<Vec<Id>> {
        let mut list: Vec<Id> = self.setting(KEY)?;
        list.retain(|p| *p != id);
        self.set_pinned_projects(list)
    }
}
