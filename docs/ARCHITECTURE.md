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

## 4. Storage

- SQLite bundled via `rusqlite` (`bundled` feature) — currently **3.53.2** with
  FTS5. Same engine everywhere, independent of the OS.
- Pragmas: `journal_mode=WAL`, `synchronous=FULL` (power-loss safe),
  `foreign_keys=ON`, `busy_timeout=5s`, `temp_store=MEMORY`.
- Startup: refuse a newer schema → **back up** (`VACUUM INTO
  backups/pre-migrate-v{n}-{ms}.db`) if an existing database needs migrating →
  migrate (forward-only, `user_version`) → `PRAGMA quick_check` →
  `PRAGMA optimize=0x10002` (refreshes planner stats when stale).
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
  `core:window:allow-set-theme`, `window-state:default`. Each new plugin adds
  only the permissions it needs.
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
`tauri` script; `.npmrc` enables pnpm's shell emulator so this works in cmd,
PowerShell and Git Bash).

## 11. Measured performance (M1, 2026-10-16)

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
