# Roadmap

Status as of 2026-10-25. Scope is the Windows desktop app (SPEC §14); other
platforms and sync come later.

## Milestones

| # | Milestone | Status |
|---|---|---|
| M0 | Foundations | Done |
| M1 | Core (schema, op log + undo, queries, quick add, CLI) | Done |
| M2 | Desktop shell + Table | Done |
| M3 | Kanban | Done |
| M4 | Calendar, meetings, working hours, slot finder | Done |
| M5 | Repeating tasks, reminders, progress, dashboard, timer | Done |
| M6 | Import/export, backups, settings, accessibility | Done, except i18n/RTL (below) |
| M7 | Other platforms | Deferred until asked |
| M8 | Release hardening | Done, except code signing (needs the owner's certificate); in-app updates live from 0.5 |
| M9 | Sync | Phase 2 (design in SPEC §12) |

Measured budgets are in [ARCHITECTURE.md §11](ARCHITECTURE.md#11-measured-performance-2026-10-16).

## Needs the owner

These need accounts or keys only the project owner can create. Steps are in
[RELEASING.md](RELEASING.md).

- **Code signing** (Azure Trusted Signing or an Authenticode certificate).
  Until then SmartScreen warns on first install.
- **Update signing secrets**: in-app updates are built in; add the two
  repository secrets (RELEASING.md §4) so releases carry signed update
  packages.

## Deferred from v1 spec

| Item | Why deferred | Notes |
|---|---|---|
| i18n + RTL (Paraglide) | Single English-speaking user today | UI strings are plain literals; dates and numbers already use `Intl` |
| `op_log` compaction | Not needed yet | The log grows ~2 rows per edit (≈ 114k rows / 92 MB on the 50k-task bench); undo only needs recent groups |
| Schema fixtures under `fixtures/` | No released schema yet | Add a fixture per released version from v0.1.0 on |
| Fuzzing (quick add, importers) | Needs a nightly CI job | Importers already reject malformed input with errors, covered by tests |
| Bits UI primitives | Own small components were enough | Dialogs, menus and palette pass the axe audit |
| Nightly startup trace in CI | Needs a Windows runner with a display | Measured by hand per release (RELEASING checklist) |

## Usability to-do (2026-10-17)

Small, high value (done after 0.3.0):

- [x] **Archived list with Restore / Delete** for groups and projects. Today
      archiving hides them and only Ctrl+Z right after brings them back.
- [x] **Hover + on projects and groups** (like the Meetings +): on a project,
      focus quick add with that project set; on a group, start a new project in it.
- [x] **Reopen where you left off**: restore the last scope (project / group /
      section) and view on start instead of always opening Today.
- [x] **Shortcut cheat sheet**: press `?` to see every keyboard shortcut in the
      app (N, Space, F2, Alt+↑/↓…), not only in SHORTCUTS.md.

Bigger (done after 0.3.1):

- [x] **Drag and drop in the sidebar**: reorder sections, groups and projects;
      drag a project into another group or section.
- [x] **Drop tasks on the sidebar**: drag tasks from the table or board onto a
      project to move them.
- [x] **Pinned projects**: a short "Pinned" list at the top of the sidebar for
      the projects used daily.

## AI assistants (done after 0.3.1)

- [x] MCP server (`taskboard-app.exe --mcp`, `taskboard mcp`): 72 tools over
      sections, groups, projects, tasks, board columns, events, slots,
      working hours, time, saved views, reminders, settings, backups and
      undo/redo; arguments are checked against their schemas.
- [x] App refreshes on outside commits (SQLite `data_version` watcher).
- [x] Settings → AI assistants: switch, configs, self-test, connection log.
- [ ] MCP resources (read-only views of today / a project) and prompts.
- [ ] Per-assistant permissions (read-only assistants).

## Next improvements

- Table: drag column headers to reorder (grouping shipped in 0.5.2;
  reordering works today from Settings → Views & table).
- Large databases (50k tasks): the All work board, grouped table, dashboard,
  month view and Reminders page reload in 200–450 ms per change. Needs new
  indexes (a schema migration): covering index for board columns,
  rules-only index for repeating tasks, joined sorts for status / project.
- Meeting notes: turn `- [ ]` action items into tasks.
- Calendar: keyboard moving and resizing of events.
- Subscribe to an ICS feed (read-only calendar layer; schema exists).
- Per-task reminder offsets (today: one global default per kind).
- Task-list items in notes rendered without the bullet.
