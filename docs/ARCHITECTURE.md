# Taskboard — Architecture

Status: M0 (foundations). Scope: **Windows 10/11 only** (see SPEC.md).

## 1. Why Tauri 2 + Svelte 5 + Rust

| Option | Verdict |
|---|---|
| **Tauri 2 + Svelte 5 + Rust core** (chosen) | WebView2 is evergreen Chromium on Windows 10/11, so every modern CSS feature works (OKLCH, `light-dark()`, View Transitions). That gives the most polished UI per hour of work. Rust core + bundled SQLite keeps data work fast and off the UI thread. Small installer (~5–10 MB), and other platforms stay possible later. |
| GPUI (pure Rust, GPU-rendered) | Fastest rendering, but young: table, Kanban and calendar would be built from scratch. Rejected for build cost. |
| C# + WinUI 3 | Most native look, but slower cold start, heavier runtime, harder packaging. Rejected on speed. |
| Dioxus | Same webview as Tauri on desktop with a less mature component ecosystem than Svelte. No advantage for us. |

## 2. Layers

```
ui/ (Svelte 5)  ── invoke(typed) ──▶  src-tauri/ (thin)  ──▶  taskboard-core  ──▶  SQLite (bundled)
      ▲                                    │
      └──────── events (M1+) ◀─────────────┘
```

- **`crates/taskboard-core`** — all domain logic: schema, migrations, commands,
  queries, parsing, recurrence, import/export. Synchronous API, no Tauri, no
  async runtime. Fully testable with `cargo test`.
- **`apps/taskboard-app/src-tauri`** — maps Tauri commands to core calls,
  owns window setup (Mica, single instance, window state). No business logic.
- **`apps/taskboard-app/ui`** — renders and caches query results. No date
  math, no filtering, no progress math.
- **`crates/taskboard-cli`** — `taskboard` binary for scripting, seeding and
  debugging against the same database file as the app.

## 3. Data flow (target design, built in M1)

1. **Single mutation path.** UI sends a `Command` → core validates → produces
   `Op`s → applies them in one transaction → appends to `op_log` with
   inverses → emits `Changed { entities, ids, seq }`.
2. **Threads.** One writer thread owns the only write connection (writes are
   serialized, no `SQLITE_BUSY`); 2–4 read-only connections serve queries.
   Tauri commands are async and reach these through channels. *M0 uses a single
   `Mutex<Db>`; replaced in M1.*
3. **Windowed queries.** Views send a `ViewSpec` + range; core returns only
   visible rows, group headers and totals. No view loads all tasks.
4. **Cache + invalidation.** UI caches results by `ViewSpec + range`; change
   events invalidate affected windows only.
5. **Optimistic UI** with rollback and a toast on error.

### As built in M1

- `Tx` (`core/src/tx.rs`) is the only way to write. `insert` / `update` /
  `delete` stamp `updated_at`, `hlc`, `device_id` on syncable tables and append
  to `op_log` (`before_json` / `after_json`, typed-value JSON that round-trips
  exactly). Updates log only columns that actually changed; a no-op edit
  records nothing.
- `history` is the undo stack (one row per user action, `undone` flag). Undo
  and redo replay `op_log` for a group in reverse / forward order and write new
  `undo` / `redo` log records, so `op_log` is append-only (sync-ready).
- Each transaction gets a timestamp strictly after the previous one, so cascade
  stamps (`archived_at`, `deleted_at`) identify exactly what one action touched.
- Proven by a property test: random edit sequences → undo all → state equals
  the start → redo all → state equals the end.

### As built in M2 (app shell)

- Every Tauri command is `async` and runs its DB work via `spawn_blocking`
  on one `Arc<Mutex<Db>>` (non-async Tauri commands run on the main thread and
  would stall the window). Reads take ~1–10 ms, so a single connection is
  enough for now; a writer thread + read pool remains the upgrade path.
- Mutations emit `data-changed`; the UI store bumps `dataVersion`, re-fetches
  sidebar data, and views re-query only what is on screen.
- `TASKBOARD_DB` overrides the database path (demos, development), matching
  the CLI.
- UI components are hand-written instead of Bits UI: the table, menus and
  palette need tight control over keyboard handling and rendering, and the
  whole UI ships in ~47 KB gzipped JS. Fixed-height row virtualization is
  ~30 lines, so TanStack Virtual was not needed.

### As built in M3–M4

- **Board columns** are statuses. One project → its own columns (editable);
  several projects → columns merged by status name. Order keys are generated
  after the last key of the whole merged column, so keys stay distinct across
  projects; a drop between equal keys falls back to "just after the upper".
- **Progress** (migration 4): manual 0–100 on tasks without subtasks; list
  rows return effective progress (100 when closed, else the average of direct
  subtasks, else manual).
- **Calendar**: events are anchored instants + IANA zone; recurring series are
  expanded per requested range (`core/src/rrule.rs`, DST-safe in the event's
  zone) with `event_exception` rows for moved/cancelled occurrences; "this and
  following" splits the series. Meetings that count as work time are *derived*
  into logged time rather than copied into `time_entry`, so editing or
  cancelling a meeting can never leave stale hours behind.
- **Slot finder**: working hours (weekly template + day overrides) ∩ client
  hours (converted from the client's zone) − busy (meetings, non-flexible
  blocks, ± buffer), then suggestions at window edges (`core/src/slots.rs`,
  property-tested interval arithmetic).

### As built in M5–M8

- **Repeating tasks**: an RRULE subset, or `AFTER:<n><D|W|M>` for "after
  completion". Completing a task creates the next instance inside the same
  transaction, so one undo removes both and nothing is duplicated.
- **Reminders** (migration 5): a `reminders` thread checks every 20 s for due
  meeting/task reminders and the daily digest, records each in
  `notification_log` (fires once), and shows a Windows toast plus an in-app
  banner with Join (http/https links only, via the opener plugin).
- **Dashboard**: streaks, a 14-day trend, per-client summary and project
  burndown, all computed in core (`dashboard.rs`).
- **Settings** live as JSON in the `setting` table and are not undoable.
- **Backups**: `VACUUM INTO` + integrity check of the copy; daily automatic
  backup (first check 2 min after launch, then hourly) with 7 daily / 4 weekly
  / 12 monthly retention; restore swaps the file and restarts.
- **Import/export**: JSON (merge by key, deferred foreign-key checks), CSV
  (Taskboard, Todoist, Trello, Notion), ICS (with EXDATE and UID dedup).
- **Desktop**: tray icon, close-to-tray, autostart with `--minimized`,
  single instance, rolling daily log files (7 kept) in
  `%APPDATA%\app.taskboard.desktop\logs`.
- **Startup path** (M8): the database opens without `quick_check`; a separate
  `integrity` thread runs it 30 s later on its own read-only connection, so it
  never holds the lock the UI needs, and emits `integrity-failed` to a banner.
  Migration 6 adds the indexes the pre-paint queries need (per-project counts,
  `max(hlc)`). `main.ts` reveals the window even if boot fails, with the error
  in the page, so a failure is never an invisible app.
- **Batches**: `Db::batch(label, f)` runs several actions in one SQLite
  transaction (each inner `transact` is a savepoint logged under the batch's
  history group). Bulk edit / complete / delete are one undo step and apply
  all or nothing.
- **Saved views**: `saved_view.spec_json` is a JSON object owned by the UI
  (scope, search, sort, view, lanes); core validates it is an object ≤ 16 KB.
- **Markdown** (task, meeting and client notes): `pulldown-cmark` renders in
  core, raw HTML is shown as text, task lists become ☐/☑, then `ammonia`
  sanitizes (links limited to http/https/mailto with `noopener noreferrer`,
  no images). The UI intercepts link clicks and opens them in the browser.
- **Ordering**: `Db::move_client` / `move_project` swap an item with its
  neighbour among the visible siblings (one undo step); groups with tied
  sort keys are renumbered in the same action. A project moved to another
  client goes last there.
- **Layout preferences** (`ui/src/lib/layout.svelte.ts`,
  `ui/src/lib/tableLayout.svelte.ts`): sidebar item order and visibility,
  sections, view tabs, and table column order / visibility / widths, per
  device in `localStorage`; edited in Settings → Layout; hidden sidebar items
  stay in Ctrl+K. Widening a column never auto-hides others (auto-hide uses
  the smaller of default and chosen width). Below 1000 px the sidebar is a drawer, below
  1200 px the task panel overlays the list; checked at 720×480 up to
  1280×800.
- **Subtasks in lists**: list scopes show top-level tasks; Today, Upcoming
  and searches include subtasks with their parent's title (`parent_title`).
- **Crash test** (`core/tests/crash.rs`): re-runs its own test binary as a
  writer, kills it 100 times at varying points, and checks `quick_check` plus
  every acknowledged write after each kill. It covers process kills; power
  loss relies on `synchronous=FULL`.

## 4. Storage

- SQLite bundled via `rusqlite` (`bundled` feature) — currently **3.53.2** with
  FTS5. Same engine everywhere, independent of the OS.
- Pragmas: `journal_mode=WAL`, `synchronous=FULL` (power-loss safe),
  `foreign_keys=ON`, `busy_timeout=5s`, `temp_store=MEMORY`.
- Startup: refuse a newer schema → **back up** (`VACUUM INTO
  backups/pre-migrate-v{n}-{ms}.db`) if an existing database needs migrating →
  migrate (forward-only, `user_version`) → `PRAGMA optimize=0x10002`
  (refreshes planner stats when stale). The app runs `PRAGMA quick_check`
  in the background; the CLI and tests run it before returning.
- `task.rid` is an explicit `INTEGER PRIMARY KEY` so the FTS rowid mapping
  survives `VACUUM`.
- List queries (`core/src/query.rs`) filter on `task` only, use
  `completed_at IS NULL` for "open" (kept in sync with closed categories), sort
  with expressions that match the migration-3 partial indexes, select the page's
  `rid`s first (`WITH page AS MATERIALIZED`) and join names for those rows only.
- Location: `%APPDATA%\app.taskboard.desktop\taskboard.db` (Tauri
  `app_data_dir`). The CLI uses the same default; override with `--db` or
  `TASKBOARD_DB`.

## 5. UI foundations

- **Theme tokens** in `ui/src/app.css`: semantic CSS custom properties in
  OKLCH using `light-dark()`, exposed to Tailwind v4 via `@theme inline`.
  Components use only tokens (`bg-surface-1`, `text-muted`, `bg-accent` …).
- **Theme switching**: `data-theme` on `<html>` (`light`/`dark`, absent =
  system) + `--accent-h` for the accent hue. The native title bar follows via
  `window.setTheme`.
- **No flash at startup**: the window is created hidden (`visible: false`);
  `main.ts` applies the theme, mounts, waits two animation frames, then calls
  `show_main_window`.
- **Mica**: applied in Rust (`window-vibrancy`); `has_mica` tells the UI, which
  sets `data-mica="true"` and makes the sidebar transparent. Windows 10 gets
  solid surfaces.
- **Font**: Segoe UI Variable (Text / Display optical sizes), Segoe UI fallback.
- **Typed IPC**: Rust types derive `ts_rs::TS`; `cargo test` writes them to
  `ui/src/lib/bindings/`. All calls go through `ui/src/lib/ipc.ts`.

## 6. Security

- CSP: `default-src 'self'`, no remote scripts, no `eval`; IPC only.
- Capabilities (`src-tauri/capabilities/default.json`): `core:default`,
  `core:window:allow-set-theme`, `window-state:default`, dialog open/save,
  and opener URLs restricted to `http(s)://`. Each new plugin adds only the
  permissions it needs.
- No network requests of its own; no telemetry. Logs contain timings and
  errors, never task content.
- Markdown rendering (M2+) happens in core and is sanitized before the UI sees it.

## 7. Pinned versions (2026-10-16)

| Component | Version |
|---|---|
| Rust toolchain | 1.99 (edition 2024, `rust-toolchain.toml`) |
| Tauri | 2.12 (CLI 2.12.3) |
| SQLite (bundled) | 3.53.2 via rusqlite 0.40 |
| Svelte | 5.57 |
| Vite | 8.3 (Rolldown) |
| TypeScript | 6.0 |
| Tailwind CSS | 4.3 |
| Node / pnpm | 22 / 12 |

## 8. Dependencies and why

| Crate / package | Why |
|---|---|
| `tauri` | App shell, WebView2, IPC, bundling |
| `tauri-plugin-single-instance` | Second launch focuses the running window; prevents two writers |
| `tauri-plugin-window-state` | Restores window size/position |
| `window-vibrancy` | Mica backdrop on Windows 11 (same authors as Tauri) |
| `rusqlite` (`bundled`, `backup`) | SQLite with FTS5 compiled in; online backup API for M6 |
| `rusqlite_migration` | Forward-only migrations on `user_version`; tiny |
| `jiff` | Time zones and civil dates done right (DST); replaces `chrono` |
| `uuid` (`v7`) | Time-ordered ids |
| `serde`, `serde_json` | Serialization across IPC and exports |
| `thiserror` | Typed core errors with stable codes |
| `ts-rs` | Rust → TypeScript types. Chosen over `tauri-specta`, which is still RC (`specta` 2.0.0-rc.25) |
| `tracing`, `tracing-subscriber` | Structured local logs |
| `clap`, `anyhow` | CLI only |
| `proptest`, `tempfile` | Tests |
| `@tauri-apps/api` | Typed `invoke` and window APIs |
| `tailwindcss` + `@tailwindcss/vite` | Utility CSS on top of design tokens; no runtime |
| `@lucide/svelte` | Icons, tree-shaken per icon |

## 9. Lints and quality gates

- Workspace lints deny `unwrap_used`, `expect_used`, `panic`, `todo`,
  `unimplemented`, `dbg_macro`; `unsafe_code` is forbidden. Test modules opt
  out locally with `#[allow]`.
- Every change: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace`, `pnpm check`, `pnpm build`.

## 10. Developer commands

```bash
# from apps/taskboard-app/ui
pnpm install
pnpm app:dev          # run the desktop app (Vite + Tauri)
pnpm app:build        # NSIS + MSI installers
pnpm check            # svelte-check + tsc

# from repo root
cargo test --workspace            # also regenerates TS bindings
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p taskboard-cli -- info
```

The Tauri CLI is run from `ui/` with `TAURI_APP_PATH=../src-tauri` (set by the
`tauri` script; `shellEmulator: true` in `pnpm-workspace.yaml` makes this work
in cmd, PowerShell and Git Bash).

## 11. Measured performance (2026-10-16)

### Core queries (M1)

Seeded with `taskboard seed --tasks 50000` (12 clients, 38 projects, subtasks,
tags, ~1/3 done). Release build, this dev machine, warm cache, timings include
the count and the page query.

| Query (50k tasks) | Budget | Measured |
|---|---|---|
| Open tasks, page of 200, sort by due / manual / priority / created | view switch < 100 ms | 8.7–9.9 ms |
| All tasks, page of 1000 | — | 17.5 ms |
| Full-text search ("report", 2,851 hits) | < 30 ms | 12.6 ms |
| One client (2,596 tasks) / one project (829 tasks) | — | 10.5 / 7.2 ms |
| Single write (create task, `synchronous=FULL`) | p95 < 10 ms | ≈ 4.4 ms avg |

Before migration 3 and the query rewrite, the same list took ~230 ms.
Seeding is one transaction per task (≈ 5 min for 50k); bulk import will batch.

### Desktop app (M8, 2026-10-16)

Release build (`pnpm app:build`), this dev machine, Windows 11, five launches
each, warm file cache. "First paint" is the `window shown` log line: process
start to the first themed frame with sidebar data loaded.

| Measure | Budget | Small database | 50k-task database (92 MB) |
|---|---|---|---|
| Cold start to first paint | < 400 ms | 357–371 ms | 358–386 ms |
| First launch after a rebuild (exe not cached) | — | ≈ 790 ms | — |
| Idle memory, all 7 processes, private working set (Task Manager) | < 150 MB | 112 MB | 125 MB |
| Installer size | < 25 MB | NSIS 4.6 MB, MSI 7.8 MB | |

Of the ~360 ms, about 215 ms is Tauri and WebView2 starting before setup runs.
Before the M8 fixes the 50k database took ~965 ms: the background integrity
check held the database lock for ~240 ms, and the sidebar queries scanned
every task (`projects` 95 → 3 ms, `sidebar_counts` 79 → 3 ms, `max(hlc)`
26 ms → 24 µs).

## 12. Decisions log

| Date | Decision |
|---|---|
| 2026-10-16 | Windows-only v1; stack Tauri 2 + Svelte 5 + Rust core (user choice) |
| 2026-10-16 | Client → Project → Task hierarchy (user requirement) |
| 2026-10-16 | Rust 1.99 pinned (rusqlite_migration 2.6 needs ≥ 1.95) |
| 2026-10-16 | `ts-rs` for bindings (tauri-specta still RC) |
| 2026-10-16 | Plain Vite SPA, no SvelteKit |
| 2026-10-16 | Generic, log-replay undo (one mechanism for every table) instead of per-command inverse code |
| 2026-10-16 | `op_log.key_json` instead of `entity_id`, to support composite keys (`task_tag`, `working_hours`) |
| 2026-10-16 | New tasks go to the last `todo` status ("To Do"), not "Backlog" |
| 2026-10-16 | Deleting a client is refused while it has projects (no silent mass delete); deleting a project soft-deletes its tasks (one undo) |
| 2026-10-16 | Tray-resident by default, autostart minimized, hours only (no rates) (user choices) |
| 2026-10-16 | Meeting work time is derived from events, not copied into `time_entry` |
| 2026-10-16 | Integrity check moved off the startup path to a delayed read-only connection |
| 2026-10-16 | Installers unsigned and no updater until the owner provides a signing identity and updater key (see RELEASING.md) |
