<p align="center">
  <img src="apps/taskboard-app/src-tauri/icons/icon.svg" width="112" height="112" alt="Taskboard logo">
</p>

<h1 align="center">Taskboard</h1>

<p align="center">
  <strong>A fast, local-first planner for people who juggle many projects, organized your way.</strong><br>
  Tasks, a Kanban board and a calendar with meetings, working hours and a meeting-slot finder.
</p>

<p align="center">
  <a href="https://github.com/Muneeb-Ahmad-Ch/TaskBoard/actions/workflows/ci.yml"><img src="https://github.com/Muneeb-Ahmad-Ch/TaskBoard/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D4" alt="Platform: Windows 10 and 11">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-PolyForm%20Noncommercial%201.0-6E56CF" alt="License: PolyForm Noncommercial 1.0"></a>
  <img src="https://img.shields.io/badge/built%20with-Tauri%202%20%C2%B7%20Svelte%205%20%C2%B7%20Rust-24C8DB" alt="Built with Tauri 2, Svelte 5 and Rust">
</p>

<p align="center">
  <a href="#install">Download</a> ·
  <a href="#features">Features</a> ·
  <a href="docs/SHORTCUTS.md">Shortcuts</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="#build-from-source">Build</a> ·
  <a href="#license">License</a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshot-dark.png">
    <img src="docs/assets/screenshot-light.png" width="900" alt="Taskboard showing the task table for a sample client, with a task's details and subtasks open on the right">
  </picture>
</p>

Windows desktop app. No account, no cloud, no telemetry: your data stays in a
single SQLite file on your computer. Free for personal use; commercial use
needs permission (see [License](#license)).

## Features

- **Your own sections → groups → projects → tasks → subtasks.** Name the
  sidebar sections anything (Clients, Teams, Departments, Family…) and say
  what one group in each is called. First run offers ready-made setups
  (Freelancer, Company, Personal life) or a blank start. Click a section to
  see all its work with progress; drag sections, groups and projects to
  reorder or move them, drop tasks onto a project, and pin the projects you
  use daily.
- **Three views of the same data.**
  - *Table*: virtualized (50,000 tasks scroll smoothly), inline editing,
    sorting, resizable columns, multi-select bulk edits.
  - *Board*: drag and drop (mouse or keyboard), custom columns per project,
    WIP limits, swimlanes by group / project / priority.
  - *Calendar*: month, week, day and agenda; drag to create, move and resize;
    working hours shading and daily capacity.
- **Quick add** from anywhere (Ctrl+Shift+Space):
  `Send invoice fri 3pm #admin !high @website ~30m every month`.
- **Meetings** with links, attendees, notes, reminders with a Join button,
  repeating series (edit one, this-and-following, or all), and optional
  "counts as work time". A Meetings page lists what's coming up (or past),
  by day and group, with Join buttons.
- **Find a slot**: free time inside your working hours *and* the other
  side's hours in their time zone (set per group), with buffers, meeting caps and "copy
  availability" text.
- **Saved views**: keep any combination of scope, filter, sort and view in
  the sidebar and command palette.
- **Markdown notes** on tasks, meetings and groups (rendered safely, links
  open in your browser).
- **Time and progress**: task timer, logged hours per group, task progress
  from subtasks (color-coded, ✓ when done), and for each section, group or project:
  time left, meetings this week and overdue tasks. Dashboard with streaks and
  project burndown.
- **Repeating tasks**: fixed schedules (every Friday) or after completion
  (every! 3 days).
- **Safe by design**: every change can be undone (bulk edits as one step),
  crash-tested storage, daily backups with retention, restore, and
  export/import (JSON, CSV, ICS; imports from Todoist, Trello, Notion CSV).
- **Modern Windows UI**: light and dark themes, accent colors, Mica, tray icon,
  start with Windows, Windows notifications; keyboard-first and checked with
  axe-core against WCAG 2.2 AA. Works down to small windows (the sidebar
  becomes a drawer).
- **Make it yours** (Settings → Layout): show, hide and reorder sidebar items,
  sections, view tabs and table columns; resize columns; order sections,
  groups and projects.

## Install

Download the latest installer from
[Releases](https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases)
(`Taskboard_x.y.z_x64-setup.exe`, or the `.msi`). Windows 10 (1809+) and
Windows 11.

Installers are currently **not code-signed**, so Windows SmartScreen may warn
on first run ("More info" → "Run anyway").

## Build from source

Requirements: Rust (the version in `rust-toolchain.toml` installs
automatically), Node 22+, pnpm, and the
[Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/)
(WebView2 is already part of Windows 10/11).

```bash
cd apps/taskboard-app/ui
pnpm install
pnpm app:dev       # run the app in development
pnpm app:build     # build NSIS + MSI installers into target/release/bundle
```

Checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace              # also regenerates TypeScript bindings
cd apps/taskboard-app/ui && pnpm check && pnpm build
```

The `taskboard` CLI (`cargo run -p taskboard-cli -- --help`) works on the same
database: add tasks with Quick Add syntax, list, undo, and `seed` a large demo
database for benchmarks.

## Project layout

```
crates/taskboard-core   domain logic, SQLite storage, queries, recurrence,
                        reminders, backups, import/export (no UI code)
crates/taskboard-cli    command-line tool over the core
apps/taskboard-app      Tauri 2 shell (src-tauri) + Svelte 5 UI (ui)
docs/                   SPEC, ARCHITECTURE, DATA_MODEL, SHORTCUTS, RELEASING, ROADMAP
```

## Privacy

Taskboard makes no network requests of its own. It only opens meeting links
you click, in your browser. Your data lives in
`%APPDATA%\app.taskboard.desktop\taskboard.db`, with backups in the
`backups` folder next to it.

## License

Taskboard is **free for personal use** and other noncommercial use under the
[PolyForm Noncommercial License 1.0.0](LICENSE). That covers individuals,
hobby projects, charities, schools and public institutions.

**Commercial use needs permission.** This includes using Taskboard in or for a
business, or selling or bundling it. Email
muneeb.ahmad.freelancing@gmail.com to ask for a commercial license.

Copyright 2026 Muneeb Ahmad.
