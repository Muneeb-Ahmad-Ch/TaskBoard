//! The shape of the workspace: sections → groups → projects, their board
//! columns (statuses), order, and pins.

use serde_json::{Value, json};
use taskboard_core::{
    ClientPatch, Db, Id, NewClient, NewProject, NewSection, NewStatus, ProjectPatch, ProjectPlace,
    SectionDelete, SectionPatch, StatusPatch,
};

use super::{
    Args, BOOL, CATEGORY, COLOR, DATE, ID, NULLABLE_ID, NULLABLE_STR, STR, TIME, Tool, ToolResult,
    bool_arg, err, from_args, id_arg, opt_id, to_value, today, without,
};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "get_overview",
        description: "The map of the workspace: sections, groups, projects (with task counts and their board columns), pinned projects, today's date and the Today / Inbox / overdue counts. Call this first to learn the ids other tools need.",
        fields: &[(
            "include_archived",
            r#"{"type":"boolean","description":"Also list archived groups and projects (marked archived_at)"}"#,
            false,
        )],
    },
    Tool {
        name: "add_section",
        description: "Adds a sidebar section (e.g. \"Teams\"). item_name is what one group in it is called (\"team\").",
        fields: &[
            ("name", STR, true),
            ("item_name", STR, false),
            ("color", COLOR, false),
        ],
    },
    Tool {
        name: "update_section",
        description: "Renames or recolors a section, or changes what its groups are called.",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("item_name", STR, false),
            ("color", COLOR, false),
        ],
    },
    Tool {
        name: "delete_section",
        description: "Deletes a section. Its groups and projects move to move_to (another section id); without move_to everything in it is deleted. The last section cannot be deleted.",
        fields: &[("id", ID, true), ("move_to", ID, false)],
    },
    Tool {
        name: "place_section",
        description: "Moves a section in the sidebar: before another section, or last when before is omitted.",
        fields: &[("id", ID, true), ("before", ID, false)],
    },
    Tool {
        name: "add_group",
        description: "Adds a group (a client, team, person… whatever the section calls its items) to a section.",
        fields: &[
            ("name", STR, true),
            (
                "section_id",
                r#"{"type":"string","description":"Default: the first section"}"#,
                false,
            ),
            ("color", COLOR, false),
            ("contact", STR, false),
            ("notes_md", STR, false),
            (
                "time_zone",
                r#"{"type":"string","description":"IANA zone, e.g. Europe/London (used to find meeting slots)"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_group",
        description: "Changes a group: name, color, contact, notes, time zone and working hours, or moves it to another section (section_id, its projects go along).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("section_id", ID, false),
            ("color", COLOR, false),
            ("contact", STR, false),
            ("notes_md", STR, false),
            ("time_zone", NULLABLE_STR, false),
            ("work_start", TIME, false),
            ("work_end", TIME, false),
            ("meetings_count_as_time", BOOL, false),
        ],
    },
    Tool {
        name: "archive_group",
        description: "Archives a group with its projects (archived=false restores them).",
        fields: &[
            ("id", ID, true),
            (
                "archived",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_group",
        description: "Deletes an empty group. Move or delete its projects first.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "place_group",
        description: "Moves a group into a section (its projects go along), before another group there or last.",
        fields: &[
            ("id", ID, true),
            ("section_id", ID, true),
            ("before", ID, false),
        ],
    },
    Tool {
        name: "add_project",
        description: "Adds a project inside a group (client_id) or directly in a section (section_id). With neither it goes to the first section. New projects get the columns Backlog, To Do, In Progress, Review, Done.",
        fields: &[
            ("name", STR, true),
            (
                "client_id",
                r#"{"type":"string","description":"Group to put it in"}"#,
                false,
            ),
            ("section_id", ID, false),
            ("color", COLOR, false),
            (
                "code",
                r#"{"type":"string","description":"Job / PO code"}"#,
                false,
            ),
            (
                "deadline",
                r#"{"type":"string","description":"YYYY-MM-DD"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_project",
        description: "Changes a project: name, color, code, state (active / on_hold / completed), dates, or where it lives (client_id, null = no group; section_id for groupless projects).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("color", COLOR, false),
            ("client_id", NULLABLE_ID, false),
            ("section_id", ID, false),
            ("code", NULLABLE_STR, false),
            (
                "state",
                r#"{"type":"string","enum":["active","on_hold","completed"]}"#,
                false,
            ),
            ("start_date", DATE, false),
            ("deadline", DATE, false),
        ],
    },
    Tool {
        name: "archive_project",
        description: "Hides a project and its tasks from everyday views (archived=false brings it back).",
        fields: &[
            ("id", ID, true),
            (
                "archived",
                r#"{"type":"boolean","description":"Default true"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "delete_project",
        description: "Deletes a project and all its tasks (undoable). The Inbox cannot be deleted.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "place_project",
        description: "Moves a project into a group (group_id) or directly into a section (section_id), before another project there or last.",
        fields: &[
            ("id", ID, true),
            ("group_id", ID, false),
            ("section_id", ID, false),
            ("before", ID, false),
        ],
    },
    Tool {
        name: "pin_project",
        description: "Pins a project to the top of the sidebar, before another pinned project or last. Pinning a pinned project moves it.",
        fields: &[("id", ID, true), ("before", ID, false)],
    },
    Tool {
        name: "unpin_project",
        description: "Removes a project from the pinned list.",
        fields: &[("id", ID, true)],
    },
    Tool {
        name: "list_statuses",
        description: "A project's board columns (statuses) in order, with what each means (category) and its WIP limit.",
        fields: &[("project_id", ID, true)],
    },
    Tool {
        name: "add_status",
        description: "Adds a board column to a project, after another column (after) or last.",
        fields: &[
            ("project_id", ID, true),
            ("name", STR, true),
            ("category", CATEGORY, true),
            ("color", COLOR, false),
            (
                "after",
                r#"{"type":"string","description":"Column id to follow; omit to add it last"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "update_status",
        description: "Renames or recolors a board column, changes what it means (category) or its WIP limit (null = none).",
        fields: &[
            ("id", ID, true),
            ("name", STR, false),
            ("color", COLOR, false),
            ("category", CATEGORY, false),
            (
                "wip_limit",
                r#"{"type":["integer","null"],"description":"Max cards; null = no limit"}"#,
                false,
            ),
        ],
    },
    Tool {
        name: "move_status",
        description: "Moves a board column to just after another column; omit after (or null) to make it the first column.",
        fields: &[("id", ID, true), ("after", NULLABLE_ID, false)],
    },
    Tool {
        name: "delete_status",
        description: "Deletes a board column; its tasks move to move_to (another column of the same project).",
        fields: &[("id", ID, true), ("move_to", ID, true)],
    },
];

pub(super) fn call(db: &mut Db, name: &str, args: &Args) -> ToolResult {
    match name {
        "get_overview" => overview(db, bool_arg(args, "include_archived", false)),
        "add_section" => to_value(
            db.create_section(from_args::<NewSection>(args.clone())?)
                .map_err(err)?,
        ),
        "update_section" => {
            let id = id_arg(args, "id")?;
            let patch: SectionPatch = from_args(without(args, &["id"]))?;
            to_value(db.update_section(id, patch).map_err(err)?)
        }
        "delete_section" => {
            let how = match opt_id(args, "move_to")? {
                Some(section_id) => SectionDelete::MoveTo { section_id },
                None => SectionDelete::Everything,
            };
            db.delete_section(id_arg(args, "id")?, how).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "place_section" => {
            let moved = db
                .place_section(id_arg(args, "id")?, opt_id(args, "before")?)
                .map_err(err)?;
            Ok(json!({ "moved": moved }))
        }
        "add_group" => to_value(
            db.create_client(from_args::<NewClient>(args.clone())?)
                .map_err(err)?,
        ),
        "update_group" => {
            let id = id_arg(args, "id")?;
            let patch: ClientPatch = from_args(without(args, &["id"]))?;
            to_value(db.update_client(id, patch).map_err(err)?)
        }
        "archive_group" => {
            let archived = bool_arg(args, "archived", true);
            db.set_client_archived(id_arg(args, "id")?, archived)
                .map_err(err)?;
            Ok(json!({ "archived": archived }))
        }
        "delete_group" => {
            db.delete_client(id_arg(args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "place_group" => {
            let id = id_arg(args, "id")?;
            db.place_client(id, id_arg(args, "section_id")?, opt_id(args, "before")?)
                .map_err(err)?;
            to_value(db.client(id).map_err(err)?)
        }
        "add_project" => to_value(
            db.create_project(from_args::<NewProject>(args.clone())?)
                .map_err(err)?,
        ),
        "update_project" => {
            let id = id_arg(args, "id")?;
            let patch: ProjectPatch = from_args(without(args, &["id"]))?;
            to_value(db.update_project(id, patch).map_err(err)?)
        }
        "archive_project" => {
            let archived = bool_arg(args, "archived", true);
            db.set_project_archived(id_arg(args, "id")?, archived)
                .map_err(err)?;
            Ok(json!({ "archived": archived }))
        }
        "delete_project" => {
            db.delete_project(id_arg(args, "id")?).map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        "place_project" => {
            let id = id_arg(args, "id")?;
            let to = match (opt_id(args, "group_id")?, opt_id(args, "section_id")?) {
                (Some(g), None) => ProjectPlace::Group { id: g },
                (None, Some(s)) => ProjectPlace::Section { id: s },
                (Some(_), Some(_)) => {
                    return Err("give group_id or section_id, not both".to_owned());
                }
                (None, None) => {
                    return Err("give group_id (to put it in a group) or section_id (directly in a section)".to_owned());
                }
            };
            let project = db.project(id).map_err(err)?;
            if project.is_inbox {
                return Err("the Inbox has a fixed place and cannot move".to_owned());
            }
            db.place_project(id, to, opt_id(args, "before")?)
                .map_err(err)?;
            to_value(db.project(id).map_err(err)?)
        }
        "pin_project" => {
            let pinned = db
                .pin_project(id_arg(args, "id")?, opt_id(args, "before")?)
                .map_err(err)?;
            Ok(json!({ "pinned": pinned }))
        }
        "unpin_project" => {
            Ok(json!({ "pinned": db.unpin_project(id_arg(args, "id")?).map_err(err)? }))
        }
        "list_statuses" => {
            let project = id_arg(args, "project_id")?;
            db.project(project).map_err(err)?;
            let list = db.statuses(project).map_err(err)?;
            Ok(json!({
                "project_id": project,
                "statuses": list.iter().map(status_json).collect::<Vec<_>>(),
            }))
        }
        "add_status" => to_value(
            db.create_status(from_args::<NewStatus>(args.clone())?)
                .map_err(err)?,
        ),
        "update_status" => {
            let id = id_arg(args, "id")?;
            let patch: StatusPatch = from_args(without(args, &["id"]))?;
            to_value(db.update_status(id, patch).map_err(err)?)
        }
        "move_status" => to_value(
            db.move_status(id_arg(args, "id")?, opt_id(args, "after")?)
                .map_err(err)?,
        ),
        "delete_status" => {
            db.delete_status(id_arg(args, "id")?, id_arg(args, "move_to")?)
                .map_err(err)?;
            Ok(json!({ "deleted": true }))
        }
        _ => Err(format!("tool \"{name}\" has no handler")),
    }
}

fn status_json(s: &taskboard_core::Status) -> Value {
    json!({ "id": s.id, "name": s.name, "category": s.category, "color": s.color, "wip_limit": s.wip_limit })
}

fn overview(db: &mut Db, include_archived: bool) -> ToolResult {
    let sections = db.sections().map_err(err)?;
    let groups = db.clients(include_archived).map_err(err)?;
    let projects = db.projects(include_archived).map_err(err)?;
    let counts = db.sidebar_counts(today()).map_err(err)?;
    let pinned = db.pinned_projects().map_err(err)?;
    let mut statuses = db.all_statuses().map_err(err)?;
    statuses.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    let columns = |project: Id| -> Vec<&str> {
        statuses
            .iter()
            .filter(|s| s.project_id == project)
            .map(|s| s.name.as_str())
            .collect()
    };
    let item = |section: Option<Id>| {
        sections.iter().find(|s| Some(s.id) == section).map(|s| {
            if s.item_name.trim().is_empty() {
                "group".to_owned()
            } else {
                s.item_name.clone()
            }
        })
    };
    Ok(json!({
        "today": today().to_string(),
        "sections": sections.iter().map(|s| json!({ "id": s.id, "name": s.name, "item_name": s.item_name, "color": s.color })).collect::<Vec<_>>(),
        "groups": groups.iter().map(|c| json!({
            "id": c.id, "name": c.name, "section_id": c.section_id, "kind": item(c.section_id),
            "color": c.color, "time_zone": c.time_zone, "contact": c.contact, "archived_at": c.archived_at,
        })).collect::<Vec<_>>(),
        "projects": projects.iter().map(|p| json!({
            "id": p.project.id, "name": p.project.name, "client_id": p.project.client_id, "group_name": p.client_name,
            "section_id": p.project.section_id, "is_inbox": p.project.is_inbox, "state": p.project.state,
            "deadline": p.project.deadline, "open_tasks": p.open_tasks, "total_tasks": p.total_tasks, "done_tasks": p.done_tasks,
            "statuses": columns(p.project.id), "pinned": pinned.contains(&p.project.id), "archived_at": p.project.archived_at,
        })).collect::<Vec<_>>(),
        "pinned": pinned,
        "counts": counts,
    }))
}
