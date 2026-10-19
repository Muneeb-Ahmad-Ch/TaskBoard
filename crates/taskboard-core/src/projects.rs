//! Projects (jobs) and their per-project status workflows.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Row};

use crate::error::{Error, Result, invalid};
use crate::id::Id;
use crate::model::{
    Category, NewProject, Project, ProjectPatch, ProjectState, ProjectSummary, Status, boolean,
    clean_color, clean_date, clean_name, int, opt_id, opt_text, palette_color, text,
};
use crate::order::key_between;
use crate::sections::{default_section, require_section};
use crate::storage::Db;
use crate::tx::{Table, Tx};

const COLS: &str = "p.id, p.client_id, p.name, p.code, p.color, p.icon, p.state, p.start_date,
                    p.deadline, p.is_inbox, p.sort_key, p.archived_at, p.created_at, p.updated_at,
                    p.section_id";

fn map(r: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        client_id: r.get(1)?,
        name: r.get(2)?,
        code: r.get(3)?,
        color: r.get(4)?,
        icon: r.get(5)?,
        state: ProjectState::parse(&r.get::<_, String>(6)?),
        start_date: r.get(7)?,
        deadline: r.get(8)?,
        is_inbox: r.get(9)?,
        sort_key: r.get(10)?,
        archived_at: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        section_id: r.get(14)?,
    })
}

pub(crate) fn map_status(r: &Row<'_>) -> rusqlite::Result<Status> {
    let category: String = r.get(4)?;
    Ok(Status {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        color: r.get(3)?,
        category: Category::parse(&category).unwrap_or(Category::Todo),
        wip_limit: r.get(5)?,
        sort_key: r.get(6)?,
    })
}

pub(crate) const STATUS_COLS: &str = "id, project_id, name, color, category, wip_limit, sort_key";

/// A new project's columns: the starting workflow (Settings → Board columns).
fn insert_default_statuses(tx: &mut Tx<'_>, project: Id) -> Result<()> {
    let mut last: Option<String> = None;
    for col in crate::columns::default_columns_in(tx.conn()) {
        let key = key_between(last.as_deref(), None)?;
        tx.insert(
            Table::Status,
            &[
                ("id", Id::new().into()),
                ("project_id", project.into()),
                ("name", text(col.name)),
                ("color", text(col.color)),
                ("category", text(col.category.as_str())),
                ("sort_key", text(key.clone())),
                ("created_at", int(tx.now())),
            ],
        )?;
        last = Some(key);
    }
    Ok(())
}

fn require_client(tx: &Tx<'_>, client: Id) -> Result<()> {
    let ok: bool = tx.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM client WHERE id = ?1 AND deleted_at IS NULL)",
        [client],
        |r| r.get(0),
    )?;
    if ok {
        Ok(())
    } else {
        Err(Error::NotFound("client"))
    }
}

/// Creates a project (with the default workflow) inside an existing transaction.
pub(crate) fn insert_project(tx: &mut Tx<'_>, input: &NewProject) -> Result<Id> {
    let name = clean_name(&input.name, "project name")?;
    let deadline = input.deadline.as_deref().map(clean_date).transpose()?;
    let id = Id::new();
    if let Some(client) = input.client_id {
        require_client(tx, client)?;
    }
    // A grouped project lives in its group's section.
    let section_id: Option<Id> = match (input.client_id, input.section_id) {
        (Some(client), _) => tx.conn().query_row(
            "SELECT section_id FROM client WHERE id = ?1",
            [client],
            |r| r.get(0),
        )?,
        (None, Some(section)) => {
            require_section(tx, section)?;
            Some(section)
        }
        (None, None) => Some(default_section(tx)?),
    };
    let (count, last): (i64, Option<String>) = tx.conn().query_row(
        "SELECT count(*), max(sort_key) FROM project
         WHERE deleted_at IS NULL AND is_inbox = 0 AND client_id IS ?1 AND section_id IS ?2",
        [opt_id(input.client_id), opt_id(section_id)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let color = match &input.color {
        Some(c) => clean_color(c)?,
        None => palette_color(count + 1),
    };
    tx.insert(
        Table::Project,
        &[
            ("id", id.into()),
            ("client_id", opt_id(input.client_id)),
            ("section_id", opt_id(section_id)),
            ("name", text(name)),
            (
                "code",
                opt_text(input.code.clone().filter(|c| !c.trim().is_empty())),
            ),
            ("color", text(color)),
            ("deadline", opt_text(deadline)),
            ("sort_key", text(key_between(last.as_deref(), None)?)),
            ("created_at", int(tx.now())),
        ],
    )?;
    insert_default_statuses(tx, id)?;
    Ok(id)
}

impl Db {
    pub(crate) fn ensure_inbox(&mut self) -> Result<()> {
        let exists: bool = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM project WHERE is_inbox = 1)",
            [],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(());
        }
        self.transact_system(|tx| {
            let id = Id::new();
            tx.insert(
                Table::Project,
                &[
                    ("id", id.into()),
                    ("name", text("Inbox")),
                    ("color", text("gray")),
                    ("is_inbox", boolean(true)),
                    ("sort_key", text("a0")),
                    ("created_at", int(tx.now())),
                ],
            )?;
            insert_default_statuses(tx, id)
        })
    }

    pub fn inbox_id(&self) -> Result<Id> {
        Ok(self
            .conn()
            .query_row("SELECT id FROM project WHERE is_inbox = 1", [], |r| {
                r.get(0)
            })?)
    }

    pub fn create_project(&mut self, input: NewProject) -> Result<Project> {
        let name = clean_name(&input.name, "project name")?;
        let id = self.transact(&format!("Create project “{name}”"), |tx| {
            insert_project(tx, &input)
        })?;
        self.project(id)
    }

    pub fn project(&self, id: Id) -> Result<Project> {
        self.conn()
            .query_row(
                &format!("SELECT {COLS} FROM project p WHERE p.id = ?1 AND p.deleted_at IS NULL"),
                [id],
                map,
            )
            .optional()?
            .ok_or(Error::NotFound("project"))
    }

    /// All live projects with client names and task counts, ordered for the
    /// sidebar: Inbox, then by client order, then project order.
    pub fn projects(&self, include_archived: bool) -> Result<Vec<ProjectSummary>> {
        // One grouped pass over tasks, not three correlated scans per project
        // (325 ms → see ARCHITECTURE.md on a 50k-task database).
        let sql = format!(
            "WITH per_status AS (
                SELECT t.project_id AS project_id, t.status_id AS status_id, count(*) AS n
                FROM task t
                WHERE t.deleted_at IS NULL AND t.parent_id IS NULL
                GROUP BY t.project_id, t.status_id
             ), counts AS (
                SELECT ps.project_id AS project_id,
                       sum(CASE WHEN s.category IN ('todo','active') THEN ps.n ELSE 0 END) AS open,
                       sum(CASE WHEN s.category <> 'cancelled' THEN ps.n ELSE 0 END) AS total,
                       sum(CASE WHEN s.category = 'done' THEN ps.n ELSE 0 END) AS done
                FROM per_status ps JOIN status s ON s.id = ps.status_id
                GROUP BY ps.project_id
             )
             SELECT {COLS}, c.name,
                coalesce(k.open, 0), coalesce(k.total, 0), coalesce(k.done, 0)
             FROM project p
             LEFT JOIN client c ON c.id = p.client_id
             LEFT JOIN counts k ON k.project_id = p.id
             WHERE p.deleted_at IS NULL {}
             ORDER BY p.is_inbox DESC, c.sort_key IS NULL, c.sort_key, p.sort_key",
            if include_archived {
                ""
            } else {
                "AND p.archived_at IS NULL"
            }
        );
        let mut stmt = self.conn().prepare(&sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProjectSummary {
                    project: map(r)?,
                    client_name: r.get(15)?,
                    open_tasks: r.get(16)?,
                    total_tasks: r.get(17)?,
                    done_tasks: r.get(18)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn statuses(&self, project: Id) -> Result<Vec<Status>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {STATUS_COLS} FROM status WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY sort_key"
        ))?;
        let rows = stmt
            .query_map([project], map_status)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn update_project(&mut self, id: Id, patch: ProjectPatch) -> Result<Project> {
        let current = self.project(id)?;
        let mut changes: Vec<(&str, Value)> = Vec::new();
        if let Some(name) = &patch.name {
            changes.push(("name", text(clean_name(name, "project name")?)));
        }
        if let Some(color) = &patch.color {
            changes.push(("color", text(clean_color(color)?)));
        }
        if let Some(client) = patch.client_id {
            if current.is_inbox && client.is_some() {
                return Err(invalid("the Inbox cannot belong to a client"));
            }
            changes.push(("client_id", opt_id(client)));
        }
        if let Some(code) = patch.code {
            changes.push(("code", opt_text(code.filter(|c| !c.trim().is_empty()))));
        }
        if let Some(state) = patch.state {
            changes.push(("state", text(state.as_str())));
        }
        if let Some(d) = patch.start_date {
            changes.push((
                "start_date",
                opt_text(d.as_deref().map(clean_date).transpose()?),
            ));
        }
        if let Some(d) = patch.deadline {
            changes.push((
                "deadline",
                opt_text(d.as_deref().map(clean_date).transpose()?),
            ));
        }
        let new_client = patch.client_id.unwrap_or(current.client_id);
        if patch.section_id.is_some() && (current.is_inbox || new_client.is_some()) {
            return Err(invalid(
                "a project in a group moves with its group; move the group instead",
            ));
        }
        self.transact(&format!("Edit project “{}”", current.name), |tx| {
            if let Some(Some(client)) = patch.client_id {
                require_client(tx, client)?;
            }
            // Grouped projects live in their group's section; leaving a group
            // keeps the section the project was in.
            let new_section: Option<Id> = match (new_client, patch.section_id) {
                (Some(client), _) => tx.conn().query_row(
                    "SELECT section_id FROM client WHERE id = ?1",
                    [client],
                    |r| r.get(0),
                )?,
                (None, Some(section)) => {
                    require_section(tx, section)?;
                    Some(section)
                }
                (None, None) => current.section_id,
            };
            if new_section != current.section_id {
                changes.push(("section_id", opt_id(new_section)));
            }
            // A project that changes group or section goes last there,
            // instead of keeping a key that may tie with the new siblings.
            if (new_client, new_section) != (current.client_id, current.section_id) {
                let last: Option<String> = tx.conn().query_row(
                    "SELECT max(sort_key) FROM project
                     WHERE deleted_at IS NULL AND is_inbox = 0 AND client_id IS ?1
                       AND section_id IS ?2 AND id <> ?3",
                    rusqlite::params![new_client, new_section, id],
                    |r| r.get(0),
                )?;
                changes.push(("sort_key", text(key_between(last.as_deref(), None)?)));
            }
            tx.update(Table::Project, &[id.into()], &changes)
        })?;
        self.project(id)
    }

    pub fn set_project_archived(&mut self, id: Id, archived: bool) -> Result<Project> {
        let project = self.project(id)?;
        if project.is_inbox {
            return Err(invalid("the Inbox cannot be archived"));
        }
        let label = if archived { "Archive" } else { "Unarchive" };
        self.transact(&format!("{label} project “{}”", project.name), |tx| {
            let stamp = if archived { int(tx.now()) } else { Value::Null };
            tx.update(Table::Project, &[id.into()], &[("archived_at", stamp)])
        })?;
        self.project(id)
    }

    /// Soft-deletes a project and all its live tasks as one undoable action.
    pub fn delete_project(&mut self, id: Id) -> Result<()> {
        let project = self.project(id)?;
        if project.is_inbox {
            return Err(invalid("the Inbox cannot be deleted"));
        }
        self.transact(&format!("Delete project “{}”", project.name), |tx| {
            delete_project_rows(tx, id)
        })?;
        Ok(())
    }
}

/// Soft-deletes a project and its tasks inside the caller's transaction.
pub(crate) fn delete_project_rows(tx: &mut Tx<'_>, id: Id) -> Result<()> {
    let now = tx.now();
    let tasks: Vec<Id> = {
        let mut stmt = tx
            .conn()
            .prepare("SELECT id FROM task WHERE project_id = ?1 AND deleted_at IS NULL")?;
        stmt.query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for task in tasks {
        tx.update(Table::Task, &[task.into()], &[("deleted_at", int(now))])?;
    }
    tx.update(Table::Project, &[id.into()], &[("deleted_at", int(now))])?;
    Ok(())
}
