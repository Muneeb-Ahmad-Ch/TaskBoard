//! `taskboard` CLI: scripting and testing the core against a real database.
// Printing results is this binary's job.
#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use taskboard_core::{
    Category, Db, Id, NewClient, NewProject, NewTask, Priority, TaskQuery, TaskRow, TaskSort,
};

#[derive(Debug, Parser)]
#[command(name = "taskboard", version, about = "Taskboard command-line tools")]
struct Cli {
    /// Database file. Defaults to the desktop app's database.
    #[arg(long, global = true, env = "TASKBOARD_DB")]
    db: Option<PathBuf>,

    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Database diagnostics.
    Info,
    /// Add a task with Quick Add syntax: `taskboard add "Report tomorrow 3pm #work !high @Acme"`.
    Add { line: Vec<String> },
    /// List open tasks (or all with --all).
    Ls {
        /// Full-text search.
        #[arg(short, long)]
        search: Option<String>,
        /// Only this client (name prefix).
        #[arg(short, long)]
        client: Option<String>,
        /// Only this project (name prefix).
        #[arg(short, long)]
        project: Option<String>,
        /// Include done and cancelled tasks.
        #[arg(short, long)]
        all: bool,
        /// Sort: manual, due, priority, created, updated, title.
        #[arg(long, default_value = "due")]
        sort: String,
        #[arg(short = 'n', long, default_value_t = 50)]
        limit: i64,
    },
    /// Mark a task done (id or unique id prefix).
    Done { id: String },
    /// Reopen a task.
    Reopen { id: String },
    /// Delete a task (undoable).
    Rm { id: String },
    /// Undo the last action.
    Undo,
    /// Redo the last undone action.
    Redo,
    /// Manage clients.
    #[command(subcommand)]
    Client(ClientCmd),
    /// Manage projects.
    #[command(subcommand)]
    Project(ProjectCmd),
    /// Fill the database with realistic sample data (for benchmarks).
    Seed {
        #[arg(long, default_value_t = 50_000)]
        tasks: usize,
        #[arg(long, default_value_t = 12)]
        clients: usize,
    },
}

#[derive(Debug, Subcommand)]
enum ClientCmd {
    Add {
        name: String,
        /// IANA time zone, e.g. Europe/London.
        #[arg(long)]
        tz: Option<String>,
    },
    Ls,
}

#[derive(Debug, Subcommand)]
enum ProjectCmd {
    Add {
        name: String,
        /// Client name (prefix match). Omit for a personal project.
        #[arg(long)]
        client: Option<String>,
    },
    Ls,
}

/// Same location the app uses: `%APPDATA%\app.taskboard.desktop\taskboard.db`.
fn default_db_path() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA").context("APPDATA is not set; pass --db")?;
    Ok(PathBuf::from(base)
        .join("app.taskboard.desktop")
        .join("taskboard.db"))
}

fn today() -> jiff::civil::Date {
    jiff::Zoned::now().date()
}

fn find_client(db: &Db, name: &str) -> Result<Id> {
    let name = name.to_lowercase();
    db.clients(false)?
        .into_iter()
        .find(|c| c.name.to_lowercase().starts_with(&name))
        .map(|c| c.id)
        .with_context(|| format!("no client matches {name:?}"))
}

/// Accepts a full id or a unique prefix of one (as printed by `ls`).
fn find_task(db: &Db, prefix: &str) -> Result<Id> {
    if let Ok(id) = prefix.parse::<Id>() {
        return Ok(id);
    }
    let page = db.list_tasks(&TaskQuery {
        include_archived: true,
        limit: 1000,
        ..Default::default()
    })?;
    let matches: Vec<&TaskRow> = page
        .rows
        .iter()
        .filter(|r| r.id.to_string().replace('-', "").ends_with(prefix))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.id),
        [] => bail!("no task id ends with {prefix:?}"),
        _ => bail!("{prefix:?} is ambiguous"),
    }
}

/// Last 6 hex chars: v7 ids share their time-based prefix, so the end is distinctive.
fn short(id: Id) -> String {
    let s = id.to_string().replace('-', "");
    s.get(s.len().saturating_sub(6)..).unwrap_or(&s).to_owned()
}

fn print_rows(rows: &[TaskRow]) {
    for r in rows {
        let check = match r.category {
            Category::Done => "✓",
            Category::Cancelled => "✗",
            Category::Active => "◐",
            Category::Todo => "○",
        };
        let prio = match r.priority {
            Priority::Urgent => " !!!",
            Priority::High => " !!",
            Priority::Med => " !",
            _ => "",
        };
        let place = match &r.client_name {
            Some(c) => format!("{c} / {}", r.project_name),
            None => r.project_name.clone(),
        };
        let due = match (&r.due_date, &r.due_time) {
            (Some(d), Some(t)) => format!("  due {d} {t}"),
            (Some(d), None) => format!("  due {d}"),
            _ => String::new(),
        };
        let tags: String = r.tags.iter().map(|t| format!(" #{t}")).collect();
        println!(
            "{} {check} {}{prio}  [{place}]{due}{tags}",
            short(r.id),
            r.title
        );
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let path = match cli.db {
        Some(path) => path,
        None => default_db_path()?,
    };
    let mut db = Db::open(&path).with_context(|| format!("opening {}", path.display()))?;

    match cli.command {
        Command::Info => println!("{}", serde_json::to_string_pretty(&db.info()?)?),
        Command::Add { line } => {
            let task = db.quick_add(&line.join(" "), today())?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&task)?);
            } else {
                println!("added {} {}", short(task.id), task.title);
            }
        }
        Command::Ls {
            search,
            client,
            project,
            all,
            sort,
            limit,
        } => {
            let sort = match sort.as_str() {
                "manual" => TaskSort::Manual,
                "priority" => TaskSort::Priority,
                "created" => TaskSort::Created,
                "updated" => TaskSort::Updated,
                "title" => TaskSort::Title,
                _ => TaskSort::Due,
            };
            let query = TaskQuery {
                search,
                client_id: client.map(|c| find_client(&db, &c)).transpose()?,
                project_id: match project {
                    Some(p) => Some(
                        db.resolve_project(&p)?
                            .with_context(|| format!("no project matches {p:?}"))?,
                    ),
                    None => None,
                },
                categories: if all {
                    vec![]
                } else {
                    vec![Category::Todo, Category::Active]
                },
                sort,
                limit,
                ..Default::default()
            };
            let started = Instant::now();
            let page = db.list_tasks(&query)?;
            let elapsed = started.elapsed();
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&page)?);
            } else {
                print_rows(&page.rows);
                println!(
                    "— {} of {} tasks ({:.1} ms)",
                    page.rows.len(),
                    page.total,
                    elapsed.as_secs_f64() * 1000.0
                );
            }
        }
        Command::Done { id } => {
            let t = db.set_task_done(find_task(&db, &id)?, true)?;
            println!("done: {}", t.title);
        }
        Command::Reopen { id } => {
            let t = db.set_task_done(find_task(&db, &id)?, false)?;
            println!("reopened: {}", t.title);
        }
        Command::Rm { id } => {
            db.delete_task(find_task(&db, &id)?)?;
            println!("deleted (undo with `taskboard undo`)");
        }
        Command::Undo => println!("undid: {}", db.undo()?),
        Command::Redo => println!("redid: {}", db.redo()?),
        Command::Client(ClientCmd::Add { name, tz }) => {
            let c = db.create_client(NewClient {
                name,
                time_zone: tz,
                ..Default::default()
            })?;
            println!("added client {}", c.name);
        }
        Command::Client(ClientCmd::Ls) => {
            for c in db.clients(false)? {
                let tz = c.time_zone.map(|t| format!("  ({t})")).unwrap_or_default();
                println!("{}{tz}", c.name);
            }
        }
        Command::Project(ProjectCmd::Add { name, client }) => {
            let client_id = client.map(|c| find_client(&db, &c)).transpose()?;
            let p = db.create_project(NewProject {
                name,
                client_id,
                ..Default::default()
            })?;
            println!("added project {}", p.name);
        }
        Command::Project(ProjectCmd::Ls) => {
            for p in db.projects(false)? {
                let owner = p.client_name.unwrap_or_else(|| {
                    if p.project.is_inbox {
                        String::new()
                    } else {
                        "Personal".into()
                    }
                });
                println!(
                    "{:<20} {:<28} {:>4} open  {:>4}/{:<4} done",
                    owner, p.project.name, p.open_tasks, p.done_tasks, p.total_tasks
                );
            }
        }
        Command::Seed { tasks, clients } => seed(&mut db, tasks, clients)?,
    }
    Ok(())
}

/// Deterministic sample data: clients with projects, personal projects, and
/// tasks with a realistic mix of dates, priorities, tags, subtasks and states.
fn seed(db: &mut Db, tasks: usize, clients: usize) -> Result<()> {
    const VERBS: [&str; 12] = [
        "Write", "Review", "Fix", "Design", "Plan", "Update", "Test", "Deploy", "Draft", "Call",
        "Prepare", "Refactor",
    ];
    const NOUNS: [&str; 12] = [
        "homepage",
        "invoice",
        "API docs",
        "login flow",
        "release notes",
        "wireframes",
        "database backup",
        "pricing page",
        "onboarding email",
        "client report",
        "sprint board",
        "test plan",
    ];
    const TAGS: [&str; 6] = ["design", "backend", "admin", "writing", "meeting", "bug"];

    let started = Instant::now();
    let mut projects = Vec::new();
    for c in 0..clients {
        let client = db.create_client(NewClient {
            name: format!("Client {:02}", c + 1),
            ..Default::default()
        })?;
        for p in 0..3 {
            projects.push(db.create_project(NewProject {
                name: format!(
                    "Project {}{}",
                    (b'A' + u8::try_from(c % 26)?) as char,
                    p + 1
                ),
                client_id: Some(client.id),
                ..Default::default()
            })?);
        }
    }
    for name in ["Portfolio", "Admin & invoices"] {
        projects.push(db.create_project(NewProject {
            name: name.into(),
            ..Default::default()
        })?);
    }

    let today = today();
    let mut rng: u64 = 0x5eed;
    let mut next = |n: usize| {
        // xorshift64: deterministic, no dependency.
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        usize::try_from(rng % n as u64).unwrap_or(0)
    };
    let mut parents: Vec<Id> = Vec::new();
    for i in 0..tasks {
        let project = &projects[next(projects.len())];
        let due = (next(3) > 0).then(|| {
            let offset = i64::try_from(next(90)).unwrap_or(0) - 20;
            today
                .checked_add(jiff::Span::new().days(offset))
                .unwrap_or(today)
                .to_string()
        });
        let parent = (next(8) == 0 && !parents.is_empty()).then(|| parents[next(parents.len())]);
        let task = db.create_task(NewTask {
            title: format!(
                "{} {} #{i}",
                VERBS[next(VERBS.len())],
                NOUNS[next(NOUNS.len())]
            ),
            project_id: if parent.is_some() {
                None
            } else {
                Some(project.id)
            },
            parent_id: parent,
            priority: Some(Priority::from_i64(i64::try_from(next(5)).unwrap_or(0))),
            due_date: due,
            estimate_minutes: (next(2) == 0)
                .then(|| i64::try_from(15 * (1 + next(16))).unwrap_or(30)),
            tags: (0..next(3))
                .map(|_| TAGS[next(TAGS.len())].to_owned())
                .collect(),
            ..Default::default()
        })?;
        if parent.is_none() && next(10) == 0 {
            parents.push(task.id);
        }
        if next(3) == 0 {
            db.set_task_done(task.id, true)?;
        }
        if (i + 1) % 5_000 == 0 {
            eprintln!("  {} tasks…", i + 1);
        }
    }
    println!(
        "seeded {clients} clients, {} projects, {tasks} tasks in {:.1}s",
        projects.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
