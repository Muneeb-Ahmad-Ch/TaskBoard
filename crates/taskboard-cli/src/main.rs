//! `taskboard` CLI: scripting and testing the core against a real database.
// Printing results is this binary's job.
#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use taskboard_core::Db;

#[derive(Debug, Parser)]
#[command(name = "taskboard", version, about = "Taskboard command-line tools")]
struct Cli {
    /// Database file. Defaults to the desktop app's database.
    #[arg(long, global = true, env = "TASKBOARD_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open the database (creating and migrating it if needed) and print diagnostics as JSON.
    Info,
}

/// Same location the app uses: `%APPDATA%\app.taskboard.desktop\taskboard.db`.
fn default_db_path() -> Result<PathBuf> {
    let base = std::env::var_os("APPDATA").context("APPDATA is not set; pass --db")?;
    Ok(PathBuf::from(base)
        .join("app.taskboard.desktop")
        .join("taskboard.db"))
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

    match cli.command {
        Command::Info => {
            let db = Db::open(&path).with_context(|| format!("opening {}", path.display()))?;
            let info = db.info()?;
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
    }
    Ok(())
}
