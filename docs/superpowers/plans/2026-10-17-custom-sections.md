# Custom Sections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fixed Clients / Personal sidebar with user-named sections that hold groups (today's clients) and projects, without changing anything an existing user sees until they choose to.

**Architecture:** A new syncable `section` table; `client.section_id` and `project.section_id` columns (migration 7, with a Rust hook that creates "Clients" and "Personal" for existing data). Internally the `client` table and `Client` type keep their names (they are "groups" in the UI). Core enforces "a grouped project lives in its group's section". The UI replaces the `client`/`personal` scope split with a `section` scope and renders sections from data.

**Tech Stack:** Rust (rusqlite, rusqlite_migration `M::up_with_hook`, ts-rs), Tauri 2 commands, Svelte 5 runes, Tailwind 4.

**Spec:** `docs/superpowers/specs/2026-10-17-custom-sections-design.md`

## Global Constraints

- Every write goes through `Db::transact` / `Tx` (undoable, logged); one user action = one undo step.
- Migration 7 is forward-only; the existing pre-migration backup runs first; it must keep cold start < 400 ms on the 50k-task bench database.
- Existing users see no change after upgrading: sections **Clients** (item name `client`) and **Personal**, same order, same contents.
- Old JSON exports (no `section` table) must import with the same mapping as the migration.
- First-run setups, verbatim: Freelancer = Clients (item: client), Personal · Company = Departments (item: department), Clients (item: client), Internal · Personal life = Home, Family, Health, Personal · Blank = My work.
- Wording, verbatim: table column "Group / Project"; dashboard "Groups"; "Hours per group"; command palette groups "Groups" and "Sections", plus "New section…"; item-name buttons "Add {item}" (default item "group").
- No section is special; sections have a color, no icon; two levels under a section (group → project, or project directly).
- UI: keyboard-first, WCAG 2.2 AA (axe clean), light + dark, works at 720×480.
- Owner preference: batch UI work; **ask before rebuilding / running the QA suite**; push only after an audit (public repo, fictional sample data).

## Review Focus

1. **Upgrade with archived or soft-deleted clients/projects** — every row (archived included) gets a section, so unarchiving later still shows it in place. Test in Task 1.
2. **Re-importing an old JSON export into a database that already has sections** — must reuse the existing "Clients"/"Personal" sections by name, not create duplicates on every import. Test in Task 5.
3. **Undo after deleting a section that moved its contents** — one undo restores the section and every moved group's and project's `section_id`. Test in Task 2.
4. **Moving a project into a group that lives in another section** (via `ProjectPatch.client_id`) — the project's `section_id` follows the group. Test in Task 3.
5. **Deleting the last section** — refused with "Keep at least one section" (new projects need a home); duplicate or blank names — blank refused, duplicates allowed. Test in Task 2.

---

### Task 1: Section table, migration 7 and read API

**Files:**
- Create: `crates/taskboard-core/src/migrations/0007_sections.sql`
- Create: `crates/taskboard-core/src/sections.rs`
- Modify: `crates/taskboard-core/src/storage.rs` (MIGRATIONS), `src/tx.rs` (`Table::Section`, `ALL_TABLES`, `syncable`), `src/lib.rs`, `src/model.rs` (`Client.section_id`, `Project.section_id`)
- Test: `crates/taskboard-core/tests/sections.rs`

**Interfaces:**
- Produces: `pub struct Section { id: Id, name: String, item_name: String, color: String, sort_key: String, created_at: i64, updated_at: i64 }` (ts-rs export); `Db::sections(&self) -> Result<Vec<Section>>` (live, by `sort_key`); `Db::section(&self, id: Id) -> Result<Section>`; `Client.section_id: Option<Id>`, `Project.section_id: Option<Id>` (None only for the Inbox).

- [ ] **Step 1: Write failing tests** in `tests/sections.rs`:
  - `migration_maps_existing_clients_and_personal_projects`: build a DB at schema 6 via the existing fixture approach (open, create 2 clients incl. one archived, 1 project per client, 2 personal projects incl. one archived, Inbox), then run migration 7. Assert `sections()` names == `["Clients", "Personal"]`, Clients `item_name == "client"`, every client (archived included) has the Clients id, every non-Inbox project without client has the Personal id, grouped projects have their client's section, Inbox `section_id == None`, client and project `sort_key`s unchanged.
  - `fresh_database_has_no_sections`: `Db::open_in_memory()` → `sections()` is empty.
  - `migration_is_one_undo_free_system_change`: after migration, `history_state().undo` is `None`.
- [ ] **Step 2: Run** `cargo test -p taskboard-core --test sections` — expect FAIL (no `sections`).
- [ ] **Step 3: Implement.** SQL: `section` table (`id BLOB PK, name TEXT NOT NULL CHECK(length(trim(name))>0), item_name TEXT NOT NULL DEFAULT '', color TEXT NOT NULL, sort_key TEXT NOT NULL, deleted_at INTEGER, created_at, updated_at, hlc TEXT NOT NULL, device_id BLOB NOT NULL`, STRICT); `ALTER TABLE client ADD COLUMN section_id BLOB REFERENCES section(id)`; same for `project`; indexes `client_section ON client(section_id, sort_key) WHERE deleted_at IS NULL` and `project_section ON project(section_id, client_id, sort_key) WHERE deleted_at IS NULL`. Register with `M::up_with_hook`; the hook creates Clients/Personal only when the DB has any client or non-Inbox project (fresh DBs get sections from first-run setup), with UUIDv7 ids, the device id from `app_meta`, colors from the existing palette, and keys from `order::key_between`; then sets the columns with UPDATEs. Migration writes are not logged in `op_log`/`history`.
- [ ] **Step 4: Run** `cargo test -p taskboard-core` — all pass (existing tests unaffected).
- [ ] **Step 5: Commit** `feat(core): sections table and migration 7`.

### Task 2: Section create / edit / reorder / delete

**Files:**
- Modify: `crates/taskboard-core/src/sections.rs`, `src/reorder.rs` (sections reuse `step`)
- Test: `crates/taskboard-core/tests/sections.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces: `NewSection { name: String, item_name: Option<String>, color: Option<String> }`, `SectionPatch { name, item_name, color: Option<String> }` (serde default, ts-rs optional); `#[serde(tag = "kind", rename_all = "snake_case")] enum SectionDelete { MoveTo { section_id: Id }, Everything }`; `Db::create_section(NewSection) -> Result<Section>`; `Db::update_section(Id, SectionPatch) -> Result<Section>`; `Db::move_section(Id, Direction) -> Result<bool>`; `Db::delete_section(Id, SectionDelete) -> Result<()>`.

- [ ] **Step 1: Write failing tests**:
  - `create_rename_recolor_and_undo`: create "Teams" with `item_name` None → stored `item_name == ""` (UI shows "group"); patch name "Departments", color; `undo()` restores the previous name.
  - `blank_name_refused_duplicates_allowed`: `"  "` → `Err(InvalidInput)`; creating "Teams" twice → two sections.
  - `move_section_up_down_and_ends`: order A,B,C → move C up twice → C,A,B; moving first up returns `false` and adds no history.
  - `delete_empty_section_is_undoable`.
  - `delete_moving_contents_is_one_undo_step`: section S with group G (+ project P in G) and direct project Q; `delete_section(S, MoveTo{T})` → G, P, Q now in T, S gone; one `undo()` → S back and G, P, Q in S again.
  - `delete_everything_soft_deletes_contents`: groups, projects and their tasks deleted; one undo restores all.
  - `cannot_delete_last_section`: → `Err(InvalidInput("Keep at least one section"))`.
- [ ] **Step 2: Run** `cargo test -p taskboard-core --test sections` — FAIL.
- [ ] **Step 3: Implement** in `sections.rs` with `transact` labels `Create section “X”`, `Edit section “X”`, `Delete section “X”`; `Everything` reuses the project/client delete cascades inside one transaction; `MoveTo` updates `client.section_id` and `project.section_id` (keys appended after the target's last). `move_section` reuses `reorder::step` with `Table::Section`.
- [ ] **Step 4: Run** tests — PASS.
- [ ] **Step 5: Commit** `feat(core): create, edit, reorder and delete sections`.

### Task 3: Groups and projects belong to sections

**Files:**
- Modify: `crates/taskboard-core/src/clients.rs`, `src/projects.rs`, `src/model.rs` (`NewClient.section_id`, `ClientPatch.section_id`, `NewProject.section_id`, `ProjectPatch.section_id`, `ProjectSummary.section_id`), `src/reorder.rs` (project siblings = same `section_id` and `client_id`), `src/query.rs` (`TaskQuery.section_id`, remove `personal`), `src/sample.rs`
- Test: `crates/taskboard-core/tests/sections.rs`, update `tests/domain.rs` where `personal: true` was used

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces: `NewClient.section_id: Option<Id>` and `NewProject.section_id: Option<Id>` (None → first section; if none exist, create "My work" in the same transaction); `ClientPatch.section_id: Option<Id>` (moving a group moves its projects); `ProjectPatch.section_id: Option<Id>` (only for groupless projects; setting `client_id` sets `section_id` to the group's); `TaskQuery.section_id: Option<Id>` (all live tasks in the section's projects); `Db::default_section(&mut Tx) -> Result<Id>` (crate-private).

- [ ] **Step 1: Write failing tests**:
  - `new_group_and_project_default_to_first_section`.
  - `fresh_db_creates_my_work_on_first_project`: no sections → `create_project` → one section "My work".
  - `moving_group_moves_its_projects_in_one_undo`.
  - `project_follows_group_into_other_section` (Review Focus 4).
  - `section_query_lists_all_work_in_section`: tasks from grouped and direct projects; excludes other sections and Inbox.
  - `reorder_projects_within_section_and_group_only`.
  - `sample_workspace_uses_or_creates_clients_and_personal` and removing it leaves user sections untouched.
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement**; replace every `personal` filter use (`client_id IS NULL AND is_inbox = 0`) with `section_id = ?`.
- [ ] **Step 4: Run** `cargo test -p taskboard-core` — PASS.
- [ ] **Step 5: Commit** `feat(core): groups and projects live in sections`.

### Task 4: Reporting, first-run setups

**Files:**
- Modify: `crates/taskboard-core/src/dashboard.rs` (`ClientSummary`), `src/planning.rs` (`ClientHours`), `src/sections.rs`
- Test: `crates/taskboard-core/tests/sections.rs`, `tests/planning.rs`

**Interfaces:**
- Produces: `ClientSummary` and `ClientHours` gain `section_id: Option<Id>` and `section_name: String`; rows with `client_id == None` are one per section (name = section name) instead of one "Personal" row. `#[serde(rename_all = "snake_case")] enum Setup { Freelancer, Company, PersonalLife, Blank }`; `Db::apply_setup(Setup) -> Result<Vec<Section>>` (one undo step; appends sections; never deletes).

- [ ] **Step 1: Write failing tests**:
  - `dashboard_rolls_up_groupless_projects_per_section`.
  - `hours_by_group_include_section_rows`.
  - `apply_setup_creates_exact_sections`: each setup → names and item names exactly as in Global Constraints.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(core): section roll-ups and first-run setups`.

### Task 5: Import / export and CLI

**Files:**
- Modify: `crates/taskboard-core/src/exchange.rs` (`TABLES` gains `section` before `client`), `crates/taskboard-cli/src/main.rs`
- Test: `crates/taskboard-core/tests/exchange.rs`

**Interfaces:**
- Produces: JSON exports include `section`; importing an export without `section` maps like the migration, reusing live sections named "Clients"/"Personal" (Review Focus 2). CLI: `taskboard section ls`, `taskboard section add <name> [--item <noun>]`; `taskboard client add <name> [--section <name>]`.

- [ ] **Step 1: Write failing tests**: `json_round_trip_keeps_sections`; `old_export_imports_into_clients_and_personal`; `reimporting_old_export_reuses_sections` (import twice → still exactly one "Clients", one "Personal").
- [ ] **Step 2–4:** run (FAIL), implement, run (PASS). Old export fixture: build JSON from today's `export_json` with the `section` key removed.
- [ ] **Step 5: Commit** `feat(core,cli): sections in import/export and CLI`.

### Task 6: Tauri commands and typed IPC

**Files:**
- Modify: `apps/taskboard-app/src-tauri/src/lib.rs`, `apps/taskboard-app/ui/src/lib/ipc.ts`; bindings regenerate via `cargo test`

**Interfaces:**
- Produces commands (write helper for mutations): `list_sections`, `create_section(input: NewSection)`, `update_section(id, patch)`, `move_section(id, direction)`, `delete_section(id, how: SectionDelete)`, `apply_setup(setup: Setup)`; `ipc.listSections()`, `ipc.createSection(input)`, `ipc.updateSection(id, patch)`, `ipc.moveSection(id, dir)`, `ipc.deleteSection(id, how)`, `ipc.applySetup(setup)`.

- [ ] **Step 1:** add commands + ipc; `cargo test -p taskboard-core --lib` (bindings) and `cargo clippy --workspace --all-targets -- -D warnings` clean; `pnpm check` 0 errors.
- [ ] **Step 2: Commit** `feat(app): section commands`.

### Task 7: UI state — section scope and layout preferences

**Files:**
- Modify: `apps/taskboard-app/ui/src/lib/store.svelte.ts` (`app.sections`, `Scope` adds `{ kind: 'section'; id }`, removes `'personal'`; `refreshMeta` loads sections; `scopeQuery` uses `section_id`; `scopeTitle` uses section name), `src/lib/layout.svelte.ts` (`SectionId` becomes `'savedViews'` only plus `hiddenSections: Id[]`; migrate saved `sections.clients/personal === false` into `hiddenSections` using the sections named Clients/Personal on first load), `src/components/LayoutSettings.svelte` (user sections list: show/hide + reorder via `ipc.moveSection`)

**Interfaces:**
- Produces: `app.sections: Section[]`; `itemName(section: Section): string` (`section.item_name || 'group'`) in `store.svelte.ts`; `layout.hiddenSections: Id[]`, `setSectionHidden(id: Id, hidden: boolean)`.

- [ ] **Step 1:** implement; saved views pointing at a removed `personal` scope fall back to `{ kind: 'all' }`.
- [ ] **Step 2:** `pnpm check` 0 errors. **Step 3: Commit** `feat(ui): section scope and layout preferences`.

### Task 8: Sidebar sections

**Files:**
- Create: `apps/taskboard-app/ui/src/components/SectionDialog.svelte` (name, item name pre-filled with a naive singular — strip a trailing "s"/"es" — color)
- Create: `apps/taskboard-app/ui/src/components/DeleteSectionDialog.svelte` (move contents to [select] / delete everything)
- Modify: `apps/taskboard-app/ui/src/components/Sidebar.svelte`

**Interfaces:**
- Consumes: Task 6 ipc, Task 7 `app.sections`, `itemName`, `layout.hiddenSections`.
- Produces: per section: header (click → section scope; collapse; **+** menu "{Item}…" / "Project…"), groups as today's client rows, direct projects; "+ New section" row; header context menu: Rename…, Color, Move up/down (Alt+↑/↓ on focused header), Hide, Delete…; group/project menus gain "Move to section…".

- [ ] **Step 1:** implement; empty-section hint text "Add a {item} or a project."
- [ ] **Step 2:** `pnpm check` 0 errors. **Step 3: Commit** `feat(ui): user sections in the sidebar`.

### Task 9: Wording and pickers across the app

**Files:**
- Modify: `TaskTable.svelte` + `lib/tableLayout.svelte.ts` (column label "Group / Project"; hover title "Section › Group / Project"), `App.svelte` (header subtitle = section name), `ScopeStats.svelte` (`kind: 'client' | 'project' | 'section'`, query `section_id`), `Board.svelte` + `lib/board.ts` (lanes `'group'` replaces `'client'`; label "Group"), `DashboardView.svelte` ("Groups", "Hours per group", section rows), `MeetingsView.svelte` (filter "All groups"), `FindSlotDialog.svelte` (picker "With", options "Section › Group" for groups with time zone or hours), `EventDialog.svelte` (picker optgroups per section), `CommandPalette.svelte` ("Groups", "Sections", "New section…"), `ClientDialog.svelte` (title "{Item} details"; time zone/hours/contact under a "Details" disclosure), `ProjectDialog.svelte` (section select for groupless projects)

- [ ] **Step 1:** implement; `pnpm check` 0 errors; grep the UI for user-visible "Client" text and confirm only item-name-driven or "Clients"-section-named text remains.
- [ ] **Step 2: Commit** `feat(ui): group wording, section-aware pickers`.

### Task 10: First-run setup, docs and QA

**Files:**
- Modify: `apps/taskboard-app/ui/src/components/Welcome.svelte` (four setup buttons → `ipc.applySetup`; "Explore with sample data" stays; "Start empty" = Blank), `README.md`, `CHANGELOG.md` (Unreleased), `docs/DATA_MODEL.md` (migration 7), `docs/ARCHITECTURE.md`, `docs/SHORTCUTS.md`
- QA (scratchpad suites): extend flows 02/03/05/09 for sections; new flow `10-sections.js` covering Review Focus 1–5 in the real app.

- [ ] **Step 1:** implement Welcome + docs; `cargo test --workspace`, clippy, `pnpm check` clean.
- [ ] **Step 2: Ask the owner before rebuilding.** Then `pnpm tauri build --no-bundle`, run all QA flows, size checks (720×480 … 1280×800), axe (light/dark), startup timing on the 50k bench DB (migration 7 applied).
- [ ] **Step 3:** audit outgoing changes (no personal paths, fictional data), commit `feat: custom sidebar sections`, push, confirm CI green.
