# Taskboard — Product & Engineering Spec (v2)

A fast, local-first work planner and tracker for desktop (Windows, macOS, Linux)
and mobile (iOS, Android). One local database, three views (Table, Kanban,
Calendar), no account, works fully offline, never loses data.

This document is the source of truth for the build. Where it says
**"verify at kickoff"**, check the current stable version or status, record the
result in `docs/ARCHITECTURE.md`, and pin it.

> **Scope decision (2026-10-16): Windows first.** v1 targets **Windows 10 (1809+)
> and Windows 11, x64 and arm64** only. macOS, Linux, iOS and Android are
> deferred; the architecture stays portable (no Windows-only code in
> `taskboard-core`), but nothing is built, tested or packaged for those
> platforms until the user asks. Where this spec mentions mobile or other
> desktop platforms, read it as "later". Windows-specific choices:
> - UI font: **Segoe UI Variable** (ships with Windows 11; Segoe UI fallback on
>   Windows 10). No bundled font.
> - Window: **Mica** backdrop on Windows 11 (solid fallback on 10), native
>   title bar with snap layouts, follows the Windows accent color by default.
> - Reminders: Windows toast notifications fired by the tray scheduler.
> - Installers: NSIS `.exe` (default, smallest) and MSI. Signing via Azure
>   Trusted Signing (or another Authenticode certificate).
> - CI: `windows-latest` runners only. Real-app E2E via `tauri-driver` +
>   Microsoft Edge WebDriver.

---

## 1. Goal and principles

**Product goal.** Users add tasks and manage the same data in Table, Kanban and
Calendar views. The app feels instant, works offline, and survives crashes,
power loss and upgrades without losing a single committed edit.

**Principles (in priority order when they conflict):**
1. **Data is sacred.** Every write is transactional, logged, backed up and undoable.
2. **Fast by construction.** The UI thread never waits on I/O; no view ever loads
   all tasks; every budget in §7 is measured in CI, not assumed.
3. **Simple over feature-rich.** A feature that slows the core loop (add → plan →
   complete) is cut or hidden.
4. **Native-feeling on every platform.** Keyboard-first on desktop,
   thumb-first on mobile, light and dark designed as equals.
5. **Local-first, private by default.** No account, no telemetry, no network
   calls unless the user turns on a feature that needs one.

**Non-goals for v1:** accounts, cloud sync (designed now, built in phase 2),
real-time collaboration, a web app, AI features, Gantt charts, plugins,
time-tracking reports beyond the simple task timer.

---

## 2. Tech stack

Rule: use the latest stable release of each item at kickoff, pin it in
`Cargo.lock` / `pnpm-lock.yaml` / `rust-toolchain.toml`, and keep it current
with Renovate (weekly, grouped, CI-gated). Every dependency gets a one-line
justification in `docs/ARCHITECTURE.md`.

### 2.1 Rust (core + app shell)

| Concern | Choice | Why |
|---|---|---|
| Language | Rust stable, **edition 2024**, toolchain pinned in `rust-toolchain.toml` | Reproducible builds across 5 targets |
| App shell | **Tauri 2** (latest 2.x) | One codebase for desktop + iOS + Android, small installers, uses the OS webview |
| Tauri plugins (official only) | `single-instance`, `window-state`, `global-shortcut`, `notification`, `dialog`, `fs` (scoped), `opener`, `updater`, `autostart`, `haptics` (mobile) | Each covers a spec feature; no custom native code needed for v1 |
| Database | **`rusqlite` with `bundled`** + `backup` features | Bundled SQLite gives the same version and FTS5 on all 5 targets (system SQLite on mobile varies) |
| Migrations | `rusqlite_migration` | Forward-only, uses `user_version`, small |
| Date/time | **`jiff`** | DST-correct time zones and civil dates; replaces `chrono`. Verify its tz database works on all 5 targets in M0 |
| IDs | `uuid` (v7) | Time-ordered, index-friendly, sync-safe |
| Serialization | `serde`, `serde_json` | Standard |
| Errors | `thiserror` in core; `anyhow` only in CLI/app edges | Typed errors cross the IPC boundary |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` (rolling local files) | Local-only diagnostics, "Copy diagnostics" button |
| TS bindings | `tauri-specta` if a stable 2.x exists at kickoff, else `ts-rs` + a generated invoke wrapper (**verify at kickoff**) | UI talks to Rust only through generated, typed commands |
| Markdown | `pulldown-cmark` + `ammonia` (sanitizer), rendered in core | One renderer for all platforms; HTML is sanitized before it reaches the webview |
| Ordering | String fractional indexing (maintained crate or ~150 lines in-house, decided at kickoff) | Reorder writes one row; property-tested either way |
| Recurrence | RFC 5545 RRULE **subset** implemented in core on `jiff` | `rrule` crate is chrono-based; the subset is small and proptestable; stored as RRULE text so ICS export is lossless |
| Import/export | `csv`, `icalendar` | Small, maintained |
| CLI | `clap` | Standard |
| Async | No `tokio` in core (sync API). The app uses Tauri's runtime plus a dedicated DB writer thread | Keeps core simple and testable |

### 2.2 Frontend (UI)

| Concern | Choice | Why |
|---|---|---|
| Framework | **Svelte 5 (runes)** + **TypeScript (strict)** | Compiled, tiny runtime, fine-grained reactivity |
| Build | **Vite 8** (Rolldown bundler) | Rust-based bundler, fast builds |
| App structure | Plain Vite SPA (no SvelteKit) | No SSR or file routing needed; fewer moving parts, smaller bundle |
| Styling | **Tailwind CSS v4** on top of CSS custom-property design tokens | CSS-first config, fast, tokens drive both themes |
| Accessible primitives | **Bits UI** (Svelte 5 headless components) | Dialogs, menus, popovers, combobox, date picker with correct ARIA |
| Virtualization | `@tanstack/virtual-core` wrapped in a small rune-based adapter | Framework-agnostic, proven at 50k+ rows |
| Drag and drop | Pointer-events-based DnD layer (maintained framework-agnostic library chosen at kickoff, or small in-house module) | Must handle mouse, touch (long-press), pen, auto-scroll, keyboard moves and screen-reader announcements; HTML5 DnD is unreliable on touch |
| Icons | Lucide (per-icon imports) | Consistent, tree-shaken |
| Font | Inter Variable (Latin subset, self-hosted) with system fallbacks for Arabic script | Modern look, tabular numerals; no large Nastaliq bundle |
| i18n | **Paraglide JS** (compiled messages) + `Intl` APIs for dates/numbers | Type-safe keys, tree-shaken, no runtime parser |
| Lint/format | ESLint (flat config, `typescript-eslint`, `eslint-plugin-svelte`), Prettier, `svelte-check` | Mature Svelte support |
| Package manager | pnpm | Fast, strict, lockfile |

### 2.3 Testing and quality tools

- Rust: `cargo-nextest`, `proptest` (recurrence, ordering, parser, query engine),
  `insta` snapshots (parser output, exports), `cargo-llvm-cov` (coverage gate),
  `divan` or `criterion` benches, `cargo-fuzz` (quick-add parser, importers),
  `cargo-deny` (advisories, licenses, duplicates).
- UI: Vitest + Testing Library for components; **Playwright** against the UI in
  Chromium and WebKit with a mocked IPC layer (Tauri `mockIPC` + fixtures) for
  flows, screenshots in light and dark, and `axe-core` accessibility scans.
- Real-app E2E: WebdriverIO + `tauri-driver` smoke tests on Linux and Windows CI
  (Playwright cannot drive the Tauri webview on every platform; macOS has no
  WKWebView driver).

---

## 3. Architecture

```
UI (Svelte)  ──typed commands──▶  src-tauri (thin)  ──▶  taskboard-core  ──▶  SQLite
   ▲                                   │                     │
   └────────── change events ◀─────────┴─────────────────────┘
```

- **Layers.** `taskboard-core` owns all domain logic, storage, queries,
  parsing, recurrence, import/export. `src-tauri` only maps commands to core
  calls and core events to UI events. The UI renders and caches query results;
  it holds no business rules (no date math, no progress math, no filtering).
- **Single mutation path.** Every write is a `Command` → validated by core →
  turned into `Op`s → applied in **one SQLite transaction** → appended to
  `op_log` with its inverse → emits `Changed { entities, ids, seq }`.
  Undo/redo, the audit trail and future sync all read from this one log.
  Bulk edits are one op group, so one undo reverts them.
- **Threading.** One writer thread owns the only write connection (writes are
  serialized; no `SQLITE_BUSY` inside the app). A pool of 2–4 read-only
  connections serves queries in parallel. Tauri commands are async and talk to
  these over channels; the UI thread never touches the database.
- **Query model.** No view loads all tasks. Views send a `ViewSpec`
  (filters, sort, grouping, visible columns) plus a range:
  Table asks for a row window, Kanban for the first N cards per column with
  lazy loading, Calendar for a date range. Core returns compact rows (only
  visible fields), group headers and totals. Large results stream over Tauri
  2 channels.
- **Reactivity.** The UI caches results keyed by `ViewSpec + range`. Change
  events invalidate only the affected cached windows, which are re-queried.
- **Optimistic UI.** The UI applies the patch locally, sends the command, and
  on error rolls back and shows a toast with the reason and a retry.
- **Errors.** Core errors serialize as `{ code, message, details }`. Every
  command is wrapped in `catch_unwind`; a panic is logged and returned as an
  error, never a crash.
- **Process model.** `single-instance`: a second launch focuses the running
  window and forwards its arguments (e.g. quick-add text). The window is
  created hidden and shown after the first paint with the correct theme
  background, so there is no white flash.
- **Security.** Least-privilege Tauri capabilities per window; strict CSP (no
  remote content, no `eval`, no inline scripts); filesystem access limited to
  the app data dir and files the user picks in a dialog; markdown sanitized in
  core. Network use in v1: updater, and the ICS overlay only when the user adds
  a feed URL.

---

## 4. Data model

All tables are `STRICT`, `foreign_keys=ON`, enums enforced with `CHECK`.

### 4.1 Time: the most important correction
- **Instants** (`created_at`, `updated_at`, `completed_at`, `archived_at`,
  `deleted_at`, reminder fire times): UTC milliseconds, `INTEGER`.
- **Plan dates** (`start_date`, `due_date`): civil dates `YYYY-MM-DD`, `TEXT`,
  with no time zone. A task due "Oct 20" stays due Oct 20 when the user travels.
- **`due_time`**: civil `HH:MM`, floating in the device's current time zone.
  Reminders are computed from date + time + current zone.
- **Sync clock**: each row also carries `hlc` (hybrid logical clock) and
  `device_id`, so phase-2 merging does not depend on wall clocks.

### 4.2 Entities
- **Task**: `id` (UUIDv7), `title`, `description_md`, `status_id`, `priority`
  (none/low/med/high/urgent), `project_id` (NOT NULL; tasks without a project
  live in a built-in, undeletable **Inbox** project), `parent_id` (subtasks;
  core rejects cycles), `start_date`, `due_date`, `due_time`,
  `estimate_minutes`, `sort_key`, `series_id` + `recurrence_rule`,
  `completed_at`, `archived_at`, `deleted_at` (soft delete / tombstone),
  `created_at`, `updated_at`, `hlc`, `device_id`.
- **Client**: `id`, `name`, `color`, `icon`, `notes_md`, optional
  `contact` (name / email / phone, free text), `sort_key`, `archived_at`.
  A client groups the projects (jobs) done for one customer or employer.
- **Project** (a job or engagement): `id`, `client_id` (nullable: projects
  without a client are shown under **Personal**), `name`, `color`, `icon`,
  `code` (optional short reference, e.g. a job or PO number), `state`
  (`active` / `on_hold` / `completed`), `start_date`, `deadline`,
  `sort_key`, `archived_at`, plus its ordered **Status** set.
- **Status**: `id`, `project_id`, `name`, `color`, `category`
  (`todo` / `active` / `done` / `cancelled`), `sort_key`, optional
  `wip_limit`. Defaults: Backlog, To Do, In Progress, Review, Done.
  `cancelled` counts as closed but is excluded from progress totals.
- **Tag** + **task_tag** join table (not an array column), so tag filters are
  indexed.
- **Reminder**: `task_id`, `kind` (relative offset / absolute), `offset_min`
  or `at`, `fired_at`, `snoozed_until`.
- **TimeEntry**: `task_id`, `started_at`, `ended_at` (task timer).
- **SavedView**: `name`, `spec_json` (versioned schema: filter AST, sort,
  grouping, layout, visible columns), validated by core.
- **Attachment**: copied into `attachments/<sha256>` inside the app data dir
  (content-addressed, deduplicated). External path references break on mobile
  and when files move.
- **Setting**: key/value JSON in the DB, so it is backed up with the data.
  Per-device UI state (window size, last view) lives in a separate small file.
- **op_log**: `seq`, `group_id`, `hlc`, `device_id`, `entity`, `entity_id`,
  `op`, `before_json`, `after_json`. Undo is unlimited within a session and
  survives restarts; entries older than a configurable retention (default 90
  days) are compacted.

### 4.3 Recurrence
- Supported: `FREQ` daily/weekly/monthly/yearly, `INTERVAL`, `BYDAY` (with
  ordinals, e.g. 2nd Tuesday), `BYMONTHDAY`, `BYSETPOS=-1` (last weekday),
  `COUNT`, `UNTIL`, plus **"repeat N days after completion"** mode.
- Instances are not pre-materialized. Completing an instance creates the next
  one (same `series_id`). Calendar shows future occurrences in the visible
  range as read-only "ghost" items computed on the fly.

### 4.4 Search and indexes
- `task_fts`: external-content FTS5 over title, description and tag names,
  tokenizer `unicode61 remove_diacritics 2` with prefix indexes for
  type-ahead; kept in sync by triggers in the same transaction. A trigram
  index is added only if substring/CJK search tests require it.
- Partial indexes on live rows (`WHERE deleted_at IS NULL`), at minimum:
  `(project_id, status_id, sort_key)`, `(due_date)`, `(parent_id)`,
  `(completed_at)`, `task_tag(tag_id, task_id)`.

### 4.5 Progress
Per project and per parent task: done / total, optionally weighted by
estimate, computed by core in SQL and returned with query results. Shown as a
bar and a percentage.

---

## 5. Features

Priorities inside v1: **P0** must ship, **P1** should ship, **P2** ships if
budgets allow.

### Clients and projects (P0)
Organization hierarchy: **Client → Project (job) → Task → Subtask**, with tags
cutting across all of them.
- **Sidebar**: Inbox, Today, Upcoming, then clients (collapsible, sorted by
  drag), each listing its active projects, then **Personal** (projects
  without a client), then saved views. Archived clients and projects are
  hidden behind a "Show archived" toggle.
- **Client page**: all of a client's projects with progress bars, open /
  overdue counts, upcoming deadlines, and total tracked time (from the task
  timer). Project page shows the same for one project.
- **Scope switcher**: every view (Table, Kanban, Calendar, Dashboard) can be
  scoped to All, one client, one project, or a saved view. Filters and
  group-by support `client:` and `project:`; Kanban swimlanes and Table
  grouping can be by client.
- **Move** projects between clients, and tasks between projects (single and
  bulk), as one undoable action. Archiving a client archives its projects;
  unarchiving restores them.
- Quick Add: `@Project` assigns the project (and so its client); the project
  picker shows `Client / Project` and fuzzy-matches both.
- Color coding: a task shows its project color; the project chip shows the
  client name on hover and in the detail panel.

### Core (P0)
- **Quick Add** with natural-language parsing in core (shared by app and CLI):
  `Write report tomorrow 3pm #work !high @Project every monday`. Recognized
  tokens show as live chips while typing, and ambiguous dates show a preview
  before saving. English first; the grammar is locale-pluggable. Global hotkey
  on desktop opens a small always-on-top quick-add window.
- Create, edit, duplicate, archive, soft-delete, subtasks, tags, priorities,
  start/due dates, recurring tasks.
- **Undo/redo** for every action (Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z), plus an
  "Undo" toast after destructive actions.
- **Search**: FTS plus filter syntax (`status:done due:<7d #work !high`),
  same syntax in the filter bar, saved views and command palette.
- **Command palette** (Ctrl/Cmd+K) for every action, with fuzzy matching and
  recent commands.

### Table view (P0)
Virtualized rows; resizable, reorderable, hideable columns; inline editing;
multi-select and bulk edit; sort; multi-level group-by; filter bar; summary
row (count, total estimate, % done). On phones it renders as a compact
grouped list driven by the same `ViewSpec`.

### Kanban view (P0, swimlanes P1)
Columns = statuses; drag within and between columns (fractional index, one
row written); keyboard moves; soft WIP limits (column header warns, never
blocks); collapsible columns; inline quick-add per column; swimlanes by
project, priority or tag (P1). Touch: long-press to drag, haptic feedback,
plus a "Move to…" bottom sheet fallback.

### Calendar view (P0, ICS overlay P2)
Month, week, day, agenda. Tasks on their due date; multi-day bars when start
and due are set; timed tasks in the time grid. Drag to reschedule, resize to
change span, drag from the unscheduled sidebar. Configurable week start
(default from locale). Optional read-only ICS feed overlay (P2, opt-in network).

### Tracking (P1)
Dashboard: completed this week, overdue, upcoming, per-project burndown,
streaks, and a per-client summary (open tasks, overdue, hours tracked this
week). Optional task timer that records `TimeEntry` rows; tracked time rolls
up task → project → client, and can be exported as CSV per client and date
range (no invoicing in v1).

### Notifications (P0)
- **Desktop:** OS notification APIs do not schedule ahead, so the app keeps a
  lightweight tray/menu-bar process running (default on, can be turned off) and
  fires reminders itself. Autostart is opt-in. Missed reminders are shown on
  next launch.
- **Mobile:** schedule OS local notifications ahead of time and re-sync the
  schedule after every relevant change. iOS allows 64 pending notifications,
  so schedule the nearest 60. On Android 14+ exact alarms need permission; fall
  back to inexact alarms and explain this in settings.
- Optional daily digest notification. No server.

### Data safety (P0)
- **Automatic backups** using SQLite's online backup API (or `VACUUM INTO`):
  daily, before every migration and before every import. Retention: 7 daily,
  4 weekly, 12 monthly. Each backup is verified with `quick_check`. Restore is
  available from Settings.
- **Export/import:** JSON (full, versioned schema, round-trip tested), CSV,
  ICS. Best-effort import from Todoist, Trello and Notion CSV with a preview
  and a mapping report. Every import is one op group, so one undo reverts it.
- **Atomic file writes** for all exports and settings (write temp, fsync,
  rename).
- **Startup integrity:** `PRAGMA quick_check` on launch (fast); full
  `integrity_check` weekly in the background. On failure, a recovery screen
  offers "restore latest good backup" or "export everything readable".
- Refuse to open a database whose schema is newer than the app, and say why.

### Sync (phase 2, design only)
See §12.

---

## 6. UX and visual design

### 6.1 Look and feel
Calm, modern, content-first: neutral surfaces, one accent color, clear type
hierarchy, generous but efficient spacing. Density setting: Comfortable /
Compact.

### 6.2 Light and dark themes (designed as equals)
- **Design tokens** as CSS custom properties, colors in **OKLCH**. Semantic
  tokens only in components: `bg`, `surface-1/2/3`, `border`, `text`,
  `text-muted`, `accent`, `on-accent`, `focus-ring`, `danger/warn/success`,
  `priority-*`, `category-*`.
- **Dark is designed, not inverted:** near-black base (not `#000`), raised
  surfaces get lighter, accents slightly desaturated, shadows replaced by
  borders and surface steps.
- Themes: System / Light / Dark, switching instantly with no reload. CSS uses
  `color-scheme` and `light-dark()`; the native title bar and window background
  follow the theme through Tauri.
- **No flash of the wrong theme at startup:** the theme is resolved before
  first paint and the window is shown only after it.
- **Accent picker:** the user picks a hue; the app generates an OKLCH scale for
  both themes and enforces at least 4.5:1 contrast for text on accent.
- **Project colors:** a curated palette of about 12 colors, each checked for
  contrast in both themes.
- A unit test checks every token pair for contrast in both themes, and
  Playwright screenshot tests cover every view in light and dark.

### 6.3 Typography and motion
- Inter Variable with tabular numerals for dates, counts and estimates. Base
  size 13–14 px on desktop and 16 px on mobile (prevents iOS input zoom).
- Motion of 120–200 ms with ease-out; FLIP animation for reorders; View
  Transitions API for view switches where the webview supports it. Everything
  is disabled under `prefers-reduced-motion`.
- Local data is fast, so no spinners or skeletons under 150 ms; show content
  directly.

### 6.4 Desktop
- Full keyboard control, shortcuts documented in `docs/SHORTCUTS.md`,
  customizable, with conflict detection. Defaults: `N` new task, `Ctrl/Cmd+K`
  palette, `/` search, `1/2/3` switch view, `J/K` move, `Enter`/`E` edit, `X`
  select, `D` due date, `P` priority, `?` shortcut help.
- Platform polish (optional, with a solid-color fallback): Mica on Windows 11,
  vibrancy on macOS sidebar, native app menu on macOS.

### 6.5 Mobile
- Bottom tab bar: List / Board / Calendar / Search, plus a thumb-reachable
  quick-add button.
- Tap targets at least 44 px, safe-area insets respected, bottom sheets instead
  of popovers, swipe actions (complete, schedule), haptics on drag and
  complete.
- Responsive from 320 px phones to ultrawide monitors (sidebar collapses,
  detail panel becomes a sheet).

### 6.6 Accessibility (WCAG 2.2 AA)
- Visible focus everywhere. Table uses the ARIA grid pattern with roving
  tabindex. Kanban keyboard drag: Space to pick up, arrows to move, Space to
  drop, Esc to cancel, with live-region announcements. Calendar has full
  keyboard navigation.
- Minimum target size 24 px desktop, 44 px mobile. `axe-core` runs in CI;
  manual screen-reader passes with NVDA, VoiceOver (macOS and iOS) and
  TalkBack before release.

### 6.7 Internationalization
- Paraglide messages; `Intl` for dates, numbers and plurals; CSS logical
  properties only (`margin-inline-start`, never `margin-left`), so RTL works
  by switching `dir`.
- Urdu and Arabic test locales plus a pseudo-locale (long strings) in CI
  screenshots. First day of week from the locale, overridable.

### 6.8 Onboarding
First launch creates a sample project showing all three views, removable in
one click (one op group, undoable). No account, no sign-up screens.

---

## 7. Performance and robustness budgets

Budgets are measured on defined reference hardware: a 4-core 2020-era laptop,
a mid-range Android phone (Pixel 6a class) and an iPhone 12 class device, with
a **50,000-task seeded database** (`taskboard-cli seed --tasks 50000`).

| Budget | Target | Measured by |
|---|---|---|
| Cold start to interactive (desktop) | < 400 ms | Startup trace, nightly CI + release checklist |
| Cold start (mobile) | < 1 s | Manual on reference devices per release |
| Any interaction (input → paint) | < 50 ms | Playwright performance traces |
| Table scroll at 50k | 60 fps, no long task > 50 ms | Playwright trace |
| Search at 50k (core) | p95 < 30 ms | Core bench, CI gate |
| View switch | < 100 ms | Playwright trace |
| Single write command (core) | p95 < 10 ms | Core bench, CI gate |
| Idle RAM (desktop, incl. webview) | < 150 MB | Release checklist |
| Installer size (desktop) | < 25 MB | CI artifact size check |
| Initial JS bundle | < 150 KB gzipped, views lazy-loaded | CI bundle-size check |

CI fails when a core bench regresses by more than 20% against `main`.

**Robustness rules:**
- **No panics in production paths:** deny `clippy::unwrap_used`,
  `expect_used`, `panic`, `todo`, `unimplemented` outside tests, set as
  workspace lints. `catch_unwind` at the command boundary as a last resort.
- **Durability:** WAL mode with `synchronous=FULL`. Task apps write rarely, so
  the cost is negligible and committed edits survive power loss, not just app
  crashes. `busy_timeout` set for the CLI.
- **Crash test:** a CI test runs a writer loop in a child process, kills it
  at random points (100+ iterations) and verifies integrity and that every
  acknowledged write is present.
- **Migrations:** forward-only, each in a transaction, automatic backup first.
  A fixture database for **every** released schema version is committed under
  `fixtures/`; CI migrates each to the latest and checks data and integrity.
- **Fuzzing:** quick-add parser and every importer (untrusted input), run in a
  nightly CI job.
- **Privacy:** no telemetry. Any future analytics is opt-in and documented.

---

## 8. Repository layout

```
taskboard/
  Cargo.toml              # workspace: shared deps + workspace lints
  rust-toolchain.toml
  deny.toml               # cargo-deny config
  renovate.json
  crates/
    taskboard-core/       # domain, storage, queries, parser, recurrence, import/export
      benches/
      fuzz/
    taskboard-cli/        # scripting, seeding, debugging the core
    xtask/                # dev tasks: gen-bindings, seed, check-sizes, release
  apps/
    taskboard-app/
      src-tauri/          # thin command + event layer, capabilities
      ui/                 # Svelte 5 + TS + Vite
        src/lib/bindings/ # generated types; CI fails if stale
  fixtures/               # one DB per released schema version + import samples
  docs/
    SPEC.md               # this file
    ARCHITECTURE.md       # incl. dependency justification table, pinned versions
    DATA_MODEL.md
    ROADMAP.md
    SHORTCUTS.md
    adr/                  # one short ADR per non-trivial decision
  .github/workflows/
```

SQLite is the single source of truth. UI state is a cache of query results.

---

## 9. CI/CD

- **Every PR:** `rustfmt`, `clippy -D warnings`, `nextest`, coverage gate
  (core ≥ 80% lines), `cargo-deny`, `svelte-check`, ESLint, Prettier, Vitest,
  Playwright (Chromium + WebKit, light + dark, `axe-core`), bindings drift
  check, bundle-size check, core bench comparison.
- **Builds:** Windows (x64, arm64), macOS (universal), Linux (x64: AppImage,
  deb, rpm), Android (AAB + APK), iOS (IPA on a macOS runner), using
  `tauri-apps/tauri-action`, with Rust and pnpm caching.
- **Nightly:** fuzzing, crash-kill test at higher iteration counts, Playwright
  performance traces, real-app smoke tests via `tauri-driver`.
- **Release (on tag):** signed builds (Windows code signing, macOS
  notarization, Android keystore, iOS distribution certificate), signed
  updater artifacts, release notes from Conventional Commits. Required secrets
  are documented in `docs/RELEASING.md`.
- `main` is protected: all required checks green before merge.

---

## 10. Milestones

Each milestone ends with a demo (CLI script or screenshots in **both** themes),
green CI and a short report: what was built, what was deferred, decisions to
review.

| # | Milestone | Exit criteria |
|---|---|---|
| **M0** | Foundations | Workspace, toolchain, lints, Windows CI. Tauri shell with a typed command. Design tokens with light/dark switching, Mica and no startup flash. Bench harness |
| **M1** | Core | Schema + migrations, op log with undo/redo, CRUD, FTS search, query engine (filters, sort, grouping, windowing), quick-add parser, CLI, benches meeting §7 core budgets |
| **M2** | Desktop shell + Table | App shell, Table view, quick add, global hotkey, command palette, search UI, theme and accent settings |
| **M3** | Kanban | Drag and drop (mouse, touch, keyboard), custom statuses, WIP limits, swimlanes |
| **M4** | Calendar | Month/week/day/agenda, drag to reschedule and resize, unscheduled sidebar |
| **M5** | Planning depth | Recurrence, reminders (tray scheduler), subtasks, progress, dashboard, timer |
| **M6** | Data and polish | Import/export, backups and restore UI, settings, i18n + RTL, accessibility audit |
| **M7** | *(Deferred)* Other platforms | macOS, Linux, iOS, Android: only when the user asks |
| **M8** | Release hardening | Measured budgets, installers and signing, auto-update, release docs |
| **M9** | Sync (phase 2) | See §12 |

---

## 11. Working rules for the agent

1. Write `docs/ARCHITECTURE.md` and `docs/DATA_MODEL.md` first (including
   pinned versions and the dependency justification table) and confirm with
   the user before writing code. Ask questions only when blocked.
2. Work milestone by milestone. After each, run fmt, clippy, tests and benches
   and report results honestly, including failures.
3. Write tests with the code (TDD for core logic). Core coverage ≥ 80%.
4. Prefer small, boring, well-maintained crates and packages; justify each.
5. Record every non-trivial decision as a short ADR in `docs/adr/`.
6. Small commits with Conventional Commit messages; never leave `main` broken.
7. Do not add features beyond this spec without asking.

---

## 12. Phase 2: sync (design now, do not build)

The core is already sync-ready: UUIDv7 ids, per-row `hlc` + `device_id`,
tombstones via `deleted_at`, and the `op_log`.

Options to decide in a phase-2 ADR:
- **Field-level last-writer-wins by HLC** for task fields, with text merging
  (`yrs`) only for descriptions. Much simpler than a full CRDT document model
  and usually enough for a personal task app.
- **Transport A:** folder-based sync (any synced folder) exchanging encrypted
  op-log segments.
- **Transport B:** optional self-hosted `axum` relay that stores only
  encrypted op logs.
- **Encryption:** end-to-end with XChaCha20-Poly1305 (`chacha20poly1305`
  crate); keys derived from a passphrase with Argon2id.

---

## 13. Definition of done (v1)

- All three views work the same on desktop and mobile against the same local
  database format.
- Every §7 budget is met and the measurements are recorded in the release
  notes.
- Data survives the crash-kill test, power-loss settings, and upgrades from
  every fixture schema.
- Signed installers build in CI for all five targets; auto-update works on
  desktop.
- Light and dark themes pass contrast tests and screenshot review for every
  view; accessibility audit passed.
- Docs complete: SPEC, ARCHITECTURE, DATA_MODEL, ROADMAP, SHORTCUTS,
  RELEASING, ADRs.

---

## 14. Open decisions to confirm before coding

1. **Windows code-signing** option (needed by M8; unsigned builds trigger
   SmartScreen warnings).
2. **Tray-resident by default** so reminders fire? (Recommended: yes, with an
   off switch.)
3. Add the **`cancelled`** status category? (Recommended: yes.)
