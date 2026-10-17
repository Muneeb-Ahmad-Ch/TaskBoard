# Custom sections: organize work your own way

Status: approved (2026-10-17), for the release after 0.2.0. The owner asked
Claude to recheck the design and go with its best recommendations; the
revisions from that recheck are folded in below.

## Why

Taskboard's sidebar is hard-wired for a freelancer: a **Clients** section
(clients → projects) and a **Personal** section (projects with no client).
Anyone else has to squeeze their life into those words. A company owner
might think in *Teams* and *Departments*, a parent in *Family* and *Home*,
someone with a day job in *Work*, *Side business* and *Personal*. The owner
wants Taskboard to be a general task manager that anyone can shape to fit,
**with full customizability and with ease**.

## The idea (chosen: "your own sections")

The sidebar is made of **sections the user creates and names freely**.
A section holds **groups**, groups hold **projects**, and a project can
also sit directly in a section without a group (today's "Personal"
behaviour). Two levels below a section, no deeper.

```
CLIENTS            ← section (user-named)
  Acme Ltd         ← group
    Website        ← project
TEAMS
  Design team
    Q4 launch
FAMILY
  Home
  Vacation plans   ← project directly in the section
+ New section
```

Rejected alternatives (shown to the owner as mockups): *renaming the two
existing labels* (only one kind of group, so "Clients *and* Teams" is
impossible) and *folders in folders* (unlimited nesting; harder to scan,
and new users must build structure first).

## Goals

1. Create, rename, recolor, reorder, hide and delete sections, with any name.
2. Groups and projects work the same in every section: add, rename,
   archive, move (between groups, sections), reorder (menu and Alt+↑/↓).
3. Existing users see **no change** after upgrading until they choose to:
   their clients appear as groups in a section named **Clients**, their
   personal projects in a section named **Personal**, in the same order.
4. New users start from a ready-made setup instead of an empty sidebar.
5. Words in the rest of the app stop assuming "client" (see *Wording*).

## Non-goals

- Folders deeper than section → group → project.
- Custom fields per section, per-section statuses or templates for tasks.
- Sharing, multiple users, permissions, sync (sync stays phase 2).
- Changing the Inbox: it stays a fixed, special project at the top.

## Anti-goals (would be a failure even if it technically works)

- Any loss, duplication or silent re-ordering of an existing user's data.
- A freelancer's experience getting worse or wordier than today.
- Setup feeling like work: no required wizard steps; every choice editable later.
- Features that depend on client details (meeting-slot finder, client hours,
  hours-per-client) breaking or disappearing.

## How it behaves

### Sidebar
- Sections are listed in the user's order, each with a header showing its
  name, a **+** that adds a group or a project (small menu: "Group…",
  "Project…"), and a collapse toggle.
- A **"+ New section"** row at the end of the sidebar list.
- Right-click a section header: Rename, Color, Move up/down, Hide (goes to
  Settings → Layout like other hiding), Delete. Sections have a color only
  (no icon picker): less setup, and it matches how projects look. Right-click group / project:
  today's menus, plus "Move to section…".
- Groups show their badge (initial on color) as clients do today.
- **Clicking a section's name** opens all work in that section (every group
  and project in it), with the same header stats as a client or project
  today (progress, time left, meetings this week, overdue).
- A project belongs to exactly one section. If it's in a group, it's in that
  group's section; **moving a group to another section moves its projects**
  with it (one undo step).
- **No section is special**: a section named "Personal" behaves like any
  other (today's separate "Personal" scope becomes an ordinary section).
- Hidden sections behave like other hidden sidebar items: the data remains,
  it's reachable from Ctrl+K, and it's shown/hidden in Settings → Layout.

### Each section's item name
- A section has an optional **item name** in the singular (e.g. section
  "Clients" → "client", "Teams" → "team"). It's used in buttons and
  menus: "Add client", "Move to another team". Default when empty: "group".
- When creating a section, the item name is pre-filled by the builder's best
  singular guess of the section name and can be edited.

### Deleting
- An **empty** section deletes immediately (undoable).
- A section that still has groups or projects asks where to move them
  (another section) or offers "Delete section and everything in it". Both
  are one undo step. (Mirrors today's rule that a client can't silently
  take its projects with it.)

### Group details (today's client details)
Every group can optionally have: color, notes, contact, time zone,
working hours (start–end), and "meetings count as work time". They're
optional and hidden behind a "Details" area so a "Family / Home" group
isn't cluttered. Everything that uses client details keeps working with
groups:
- **Find a meeting slot**: "With" picker lists groups that have a time zone
  or hours; labelled "Section › Group".
- **Hours per client** (dashboard, week summary): becomes hours per group.
  Projects that sit directly in a section (no group) roll up under the
  section's name, as "Personal" does today.
- **Board swimlanes** "by client" become "by group".
- The **meeting editor's** "Client / project" picker lists groups and
  projects organized by section.
- **Meetings**: a meeting can be linked to a group or project as today;
  the Meetings page filter lists groups.

### First run
The welcome screen offers a starting setup (one click, then editable):

| Setup | Sections created |
|---|---|
| Freelancer | Clients (item: client), Personal |
| Company | Departments (item: department), Clients (item: client), Internal |
| Personal life | Home, Family, Health, Personal |
| Blank | My work |

"Explore with sample data" stays and fills a Clients/Personal setup.

### Upgrade (existing databases)
A migration creates two sections, **Clients** (item "client") and
**Personal**, assigns every existing client to Clients and every
non-Inbox project without a client to Personal, preserving order. A backup
is taken first (existing pre-migration backup behaviour). If a user had no
clients at all, Clients is still created so the sidebar looks as before.

### Wording across the app
Neutral words replace "client" where the meaning is general:

| Today | After |
|---|---|
| Table column "Client / Project" | "Group / Project" (shows *Section › Group / Project* on hover) |
| Header subtitle "Client" | the section's name (e.g. "Teams") |
| Dashboard "Clients" table | "Groups" |
| "Hours per client" | "Hours per group" |
| Command palette group "Clients" | "Groups", plus a "Sections" group and "New section…" |
| Quick Add `@"Acme/Website"` | unchanged (group/project) |

Where a section has an item name, UI text uses it ("Add team").

### Settings → Layout
Gains the user's sections (show/hide, reorder) in place of today's fixed
"Clients / Personal projects" switches. Saved views stay a separate,
fixed section.

### Import / export
- JSON export includes sections. Importing an older export (no sections)
  applies the same mapping as the upgrade migration.
- CSV and ICS are unchanged (CSV "Project" column may include
  "Group/Project" as today's "Client/Project").

## Constraints

- Same architecture: Rust core owns data and rules; every write goes
  through `Tx` (undoable, logged with HLC for future sync); the Svelte UI
  only calls typed commands; bindings via ts-rs.
- Forward-only migration with the existing pre-migration backup; must pass
  on a 50k-task database without hurting the 400 ms cold-start budget.
- Keyboard-first and WCAG 2.2 AA, both themes, down to 720×480 — as the
  rest of the app.
- Public repo: sample/demo data stays fictional.

## Decided by the owner

- Approach B: user-named sections → groups → projects (not renaming, not
  nested folders).
- Full customizability with ease: any names, any number of sections.
- The rest of this document's behaviour was played back and approved in
  chat ("Yes go ahead").

## Calls made by Claude (owner to check)

1. Exactly two levels under a section (group, project); projects may also
   sit directly in a section.
2. Optional singular **item name** per section, used in buttons/menus.
3. Deleting a non-empty section asks to move contents or delete all
   (one undo step); empty sections delete immediately.
4. Group details (time zone, hours, contact, "meetings count") are
   available on every group, tucked under "Details".
5. First-run setups and their section names as in the table above
   (revised: Company = Departments, Clients, Internal; Personal life =
   Home, Family, Health, Personal).
6. Wording table above ("Group / Project", "Groups", "Hours per group").
7. Section visibility is a per-device preference (like other layout
   settings); the sections themselves are data (in the database, exported,
   undoable). If the user had hidden today's Clients or Personal section,
   the matching new section starts hidden.
8. Clicking a section name shows all its work with header stats.
9. Moving a group between sections moves its projects; no section is
   special (a "Personal" section is ordinary).
10. Sections have a color but no icon; section-level projects roll up
    under the section name in hours tables; Board swimlanes and the meeting
    editor's picker use groups by section; Ctrl+K lists sections.

## Left to the builder

- Whether the `client` table is renamed internally or kept with a new
  `section` table alongside (the UI must say "group" either way; internal
  names may stay `client`; old JSON exports must still import).
- CLI subcommands for sections/groups (keep `client` working as an alias).
- Exact menus, icons, colors and copy beyond the wording table.
- Section color/icon picker details; collapse-state storage.
- Test plan, migration test fixtures, and QA additions.
