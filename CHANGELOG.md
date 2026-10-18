# Changelog

All notable changes to Taskboard are listed here, newest first.
Versions follow [semantic versioning](https://semver.org/); the format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Download any version from the
[Releases page](https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases).

## [Unreleased]

### Added
- **Updates in place:** Settings → Updates checks GitHub for a new version
  (at startup and every few hours; can be turned off), shows what's new, and
  installs it without a trip to the Releases page: a signed download with
  progress, a backup of your data, then Taskboard closes and reopens on the
  new version. A status-bar chip says when one is ready. 0.4 and earlier
  need one manual install of this version first.
- **Waiting column** for tasks waiting on someone's reply or approval: part of
  the default columns for new projects (after In Progress, counts as open),
  with an hourglass in the table. Ctrl+K → "Add a Waiting column to every
  project" adds it to existing projects in one undoable step.
- **Archive tasks** instead of deleting them: right-click a row or card, select
  and press A, or "Archive N completed" in the table footer (also in Ctrl+K
  and a Done column's menu). Archived tasks leave every list and come back
  with their subtasks from Settings → Data → Archived tasks.
- **Schedule strip** above the task list: the meetings and time blocks coming
  up for the open group, project, section or day, with + Meeting / + Time
  block for that group or project, and the task a block is for.
- **Time blocks in the task panel:** the blocks booked for a task, with "Block
  time for this task".
- **Progress on every list:** Today, Upcoming, Inbox and All work show
  progress (% and done/total) like sections, groups and projects do.
- **Zoom** from 80% to 150% (Ctrl + / Ctrl −, Ctrl + mouse wheel, Ctrl 0;
  Settings → Appearance). It stops where the window would get too small, and
  the status bar shows the level when it isn't 100%.
- **Resizable sidebar and task panel:** drag their edges, or set the width in
  Settings; double-click an edge to reset.
- **AI assistants can do everything the app can:** set a task's column by
  name (the bug where a status stayed "To Do" while progress moved), edit
  many tasks at once, archive, reorder board columns and cards, pin, edit one
  occurrence of a meeting or a whole series, book a time block for a task
  (`schedule_task`), filter meetings and blocks by kind, group, project or
  task, add all-day events, keep an event's length when moving it, timers,
  working hours, days off, slots, saved views, reminders and backups. They
  still can't change the app's settings or their own access.
- **"tb" for short:** assistants are set up as `tb` and know that "tb" means
  Taskboard ("what's due in tb today?").
- Row and card right-click menus (also the Menu key / Shift+F10).

### Changed
- **Settings → Layout** is now three tidier pages: **Sidebar** (items, a
  checkbox grid for counts and color dots, width), **Sections**, and **Views &
  table** (view tabs, add-a-task box, columns, task panel width). Reorder
  arrows appear on the row you point at; each page has its own Restore
  defaults.
- Pinned projects are stored with your data (so assistants see them) instead
  of per device.
- Empty lists say what belongs there, and "Add one above / below" follows
  where the add-a-task box is; a list whose tasks are all done says so, with
  a Show completed button.

- **Wide windows use their space:** board columns grow (up to 480px) to fill
  the width, a collapsed column keeps a one-line head with its name down its
  strip, the dashboard shows as many project columns as fit, and the Meetings
  and Agenda lists keep a readable width so Join sits next to its meeting.
- **Short windows keep their rows:** the schedule strip uses one-line chips
  and starts collapsed below 640px of height (remembered once you toggle it).
- The header's progress, time left, meetings and overdue drop out one by one
  as the window narrows, instead of squeezing the page title.

### Fixed
- The empty Inbox said "Add a task below" with the add-a-task box at the top.
- **Column resizing** tracks the mouse in every layout (task panel open,
  small windows, zoom): the Task column stops at its minimum instead of
  growing its neighbour, grows by the whole drag when the table already
  scrolls sideways, and no column appears or vanishes during or after a
  resize (which columns fit depends on the window, not on custom widths).
- Escape closes a dialog even after switching Settings pages, and no longer
  closes the task panel behind an open dialog.
- Pins saved by an older version on this device move into the database
  only when that database has those projects (a second database no longer
  takes them).
- A selected row's dimmed text (a due time) is readable in the dark theme.

### Robustness
- A view, the task panel or a dialog that fails to render shows a short
  message with Try again (and is logged) instead of a blank window; other UI
  errors are logged and reported once instead of failing silently.

## [0.4.1] - 2026-10-18

### Changed
- **Calmer table:** empty cells stay blank (a dash appears only on the row you
  point at), and progress shows only for tasks with subtasks or set progress.
- **Today** says how many tasks are overdue and due today, and overdue rows
  carry a thin red rail.
- **Board** columns are narrower so five fit beside the sidebar at 1440 px;
  "Columns from N projects"; a clearer collapse arrow.
- **Task panel** dates read like the rest of the app ("Fri, 23 Oct") with the
  system picker behind them and a clear button; the time appears once a due
  date is set.
- **Meetings:** only the next meeting's Join button is highlighted.
- **Dashboard:** "Due today" with overdue underneath, a friendly empty chart,
  trend lines only once there is history, and spacing fixes ("0 days").
- The header no longer carries theme icons (Settings → Appearance, Ctrl+K);
  the completed-tasks toggle says "Show completed" / "Hide completed".
- The status bar shows the version and Undo / Redo only (database details are
  in Settings → About); larger Undo / Redo buttons.
- Group names in the sidebar use the full row until you hover it.
- Dialogs open without a focus ring on Close; AI assistant buttons line up.

### Fixed
- Right-clicking no longer shows the browser's menu (Back, Refresh, Save as,
  Print, Share), and browser shortcuts (F5, Ctrl+R, Ctrl+P, Ctrl+S, Alt+←/→,
  mouse back/forward) no longer reload or navigate the window. Text fields keep
  their cut / copy / paste menu.

## [0.4.0] - 2026-10-18

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
- The colored dot before "Section › Group" in the page header is hidden by
  default; Settings → Layout can show it (and the section dots).

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

[Unreleased]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Muneeb-Ahmad-Ch/TaskBoard/releases/tag/v0.1.0
