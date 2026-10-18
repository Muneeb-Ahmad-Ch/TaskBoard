# Changelog

All notable changes to Taskboard are listed here, newest first.
Versions follow [semantic versioning](https://semver.org/); the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Download any version from the
[Releases page](https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases).

## [Unreleased]

### Added
- **Drag and drop in the sidebar:** reorder sections, groups and projects,
  and drag a project into another group or section (or a group into another
  section). Each move is one Undo. Alt+↑/↓ and the right-click menu still work.
- **Drop tasks on the sidebar:** drag task rows from the table (a selection
  moves together) or cards from the board onto a project, or onto Inbox.
- **Pinned projects:** right-click a project → **Pin to top** (or drag it onto
  Pinned) for a short list at the top of the sidebar; drag to reorder. Pins are
  kept per device and can be hidden in Settings → Layout.
- **AI assistants (MCP):** Taskboard is now an MCP server. **Settings → AI
  assistants** has copyable configs for Claude Desktop, Claude Code, Codex and
  other MCP clients, a self-test, an on/off switch, and a list of assistants
  that have connected. An assistant can read, add, change and remove sections,
  groups, projects, tasks, meetings and logged time; every change is undoable
  and the app refreshes within a second. CLI: `taskboard mcp`.

### Fixed
- **Column resizing** keeps following the mouse once the columns to the right
  are at their minimum: the table scrolls sideways instead of the edge stopping.

## [0.3.1] - 2026-10-17

### Added
- **Archived** list in **Settings → Data**: restore or delete archived groups
  and projects (restoring a group brings its projects back).
- Hover **+** on a project adds a task to it; on a group, starts a new
  project; on **Meetings**, opens a new meeting.
- Taskboard **reopens where you left off** (the last list and view).
- Press **?** (or "Keyboard shortcuts" in Ctrl+K) to see every shortcut.
- **More colors:** 18 colors for sections, groups, projects and statuses
  (was 12), and 12 accent themes (was 6): Indigo, Purple, Magenta, Orange,
  Lime and Sky are new.
- **Settings → Layout:** edit, delete and add sections; switch each sidebar
  count on or off (view counts, overdue badge, section and project counts).

### Changed
- **Sidebar redesign:** section names in normal case with a count
  ("Clients · 2") that stand out more, fold arrows on the right, and each
  project shows a small **progress ring** in its color. The dot before a
  section name can be hidden in Settings → Layout.
- **Quick add moved to the bottom** of the Table view (choose Top or Bottom
  in Settings → Layout); what it will create shows next to it. Typing `@`
  opens a **project dropdown** (Tab inserts, or ↑ ↓ then Enter).

### Fixed
- **Column resizing:** every column edge now follows the mouse. Before, edges
  right of the Task column moved the wrong edge, and some stopped early.
- Quick add highlights (dates, tags, `@project`) no longer sit offset from
  the text.

### Security
- Updated the Markdown sanitizer (ammonia 4.2.4, RUSTSEC-2026-0373).
  Taskboard already stripped the affected `style` attributes.

## [0.3.0] - 2026-10-17

### Added
- **Your own sidebar sections.** Organize work under sections you name —
  Clients, Teams, Departments, Family, anything — each holding groups and
  projects. Choose what one group is called ("client", "team", "person"),
  pick a color, reorder sections, and move groups and projects between them.
- **First-run setups:** Freelancer (Clients, Personal), Company (Departments,
  Clients, Internal), Personal life (Home, Family, Health, Personal), or
  start empty.
- Click a section's name to see **all its work**, with progress, time left,
  meetings and overdue tasks in the header.
- Deleting a section asks whether to move its contents to another section or
  delete everything; either way one Undo brings it back.
- Command palette: jump to sections, and **New section…**.
- CLI: `taskboard section add` / `section ls`, and `client add --section`.

### Changed
- "Client" is now **group** across the app (table column "Group / Project",
  board swimlanes by group, dashboard "Groups", meetings filter); dialogs use
  each section's own item name.
- Existing databases upgrade with no visible change: clients go to a
  **Clients** section and projects without a client to **Personal**.
  Older JSON exports still import the same way.

## [0.2.0] - 2026-10-17

### Added
- The app shows its version in the status bar and in **Settings → About**;
  clicking it opens this changelog. **What's new** and **About Taskboard**
  are also in the command palette (Ctrl+K).
- **Settings → About** lists the developer, with contact and websites.
- **Table columns:** reorder, show and hide them in **Settings → Layout**
  (or "Reorder columns…" in the table's column menu).
- Client and project headers show **time left** (estimates of open tasks),
  **meetings this week** and **overdue** tasks next to progress.
- Calendar **+ New** menu: create a **Meeting** or a **Time block** (focus
  time). "New time block…" is also in the command palette.
- The **Clients** sidebar section can be hidden.

### Changed
- **Settings redesigned:** categories on the left (General, Appearance,
  Layout, Calendar, Reminders, Data & backups, About); every change saves
  immediately.
- Hiding and reordering sidebar items now lives only in **Settings →
  Layout**; right-click a sidebar item for "Customize sidebar…".
- **Progress bars** change color with progress (blue → teal → green) and show
  a ✓ when complete.
- Reminders appear at the **bottom-right**; short confirmations with Undo at
  the bottom-centre, so neither covers the page.
- The accent color picker moved from the top bar to **Settings →
  Appearance**; scrollbars are thinner and appear only when needed.

### Fixed
- Widening a table column no longer hides other columns (the table scrolls
  sideways instead). The Task column can be resized; double-click a handle to
  reset it, or use ← / → on a focused handle.
- An empty table no longer shows a sideways scrollbar.

## [0.1.0] - 2026-10-17

First release, for Windows 10 (1809+) and Windows 11.

### Added
- **Clients → projects → tasks → subtasks.** Sidebar grouped by client,
  progress per client and project, archive, and move projects between
  clients. Reorder clients and projects (right-click or Alt+↑/↓).
- **Three views of the same tasks:** a virtualized Table (50,000 tasks scroll
  smoothly), a Kanban Board (drag and drop by mouse or keyboard, custom
  columns, WIP limits, swimlanes) and a Calendar (month, week, day, agenda).
- **Quick Add from anywhere** (Ctrl+Shift+Space) with dates, times, tags,
  priority, project, estimates and repeats in plain text, e.g.
  `Send invoice fri 3pm #admin !high @website ~30m`.
- **Meetings:** links with a Join button, attendees, Markdown notes,
  reminders, repeating series, and a Meetings page listing what's coming up.
- **Working hours and a meeting-slot finder** that respects your hours and
  your client's hours in their time zone.
- **Time tracking:** a timer on any task and logged hours per client.
- **Repeating tasks** on a schedule or after completion.
- **Saved views**, a Dashboard (streaks, trends, burndown) and a command
  palette (Ctrl+K).
- **Safety:** undo for everything (bulk edits as one step), crash-tested
  storage, daily backups with restore, and export/import as JSON, CSV and
  calendar (.ics) files, including imports from Todoist, Trello and Notion.
- **Windows integration:** tray icon, start with Windows, Windows
  notifications, Mica, light and dark themes with accent colors.
- **Customizable layout:** show, hide and reorder sidebar items and view tabs.
  Works down to 720×480 (the sidebar becomes a drawer).
- Keyboard shortcuts throughout; accessibility checked with axe-core against
  WCAG 2.2 AA.

### Known limitations
- Installers are not code-signed yet, so Windows SmartScreen may warn on
  first install ("More info" → "Run anyway").
- No automatic updates yet: download new versions from the Releases page.

[Unreleased]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases/tag/v0.1.0
