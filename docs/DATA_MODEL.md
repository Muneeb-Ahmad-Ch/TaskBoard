# Taskboard — Data Model

Status: **draft for approval** before M1. The SQL below becomes migration 2.
Conventions: all tables `STRICT`; ids are UUIDv7 stored as 16-byte `BLOB`;
instants are UTC milliseconds (`INTEGER`); plan dates are civil `TEXT`
(`YYYY-MM-DD`, no time zone); every syncable row has `hlc` + `device_id`.

## 1. Hierarchy

```
Client ──< Project ──< Task ──< Task (subtasks, any depth, no cycles)
                │         └──< task_tag >── Tag
                └──< Status (per-project workflow)
Task ──< Reminder, TimeEntry, Attachment
op_log: every change to any row above (undo/redo, audit, future sync)
```

- **Client**: a customer or employer. Optional for a project.
- **Project**: a job / engagement. `client_id IS NULL` → shown under **Personal**.
- **Inbox**: a built-in project (`is_inbox = 1`, no client) that cannot be
  deleted. Quick-added tasks without `@Project` land here, so
  `task.project_id` is never NULL.

## 2. Schema

```sql
CREATE TABLE client (
  id          BLOB PRIMARY KEY NOT NULL,
  name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
  color       TEXT NOT NULL,              -- OKLCH hue or token name
  icon        TEXT,
  notes_md    TEXT NOT NULL DEFAULT '',
  contact     TEXT NOT NULL DEFAULT '',   -- free text: name / email / phone
  sort_key    TEXT NOT NULL,              -- fractional index
  archived_at INTEGER,
  deleted_at  INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL
) STRICT;

CREATE TABLE project (
  id          BLOB PRIMARY KEY NOT NULL,
  client_id   BLOB REFERENCES client(id),
  name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
  code        TEXT,                       -- optional job / PO number
  color       TEXT NOT NULL,
  icon        TEXT,
  state       TEXT NOT NULL DEFAULT 'active'
              CHECK (state IN ('active','on_hold','completed')),
  start_date  TEXT,
  deadline    TEXT,
  is_inbox    INTEGER NOT NULL DEFAULT 0 CHECK (is_inbox IN (0,1)),
  sort_key    TEXT NOT NULL,
  archived_at INTEGER,
  deleted_at  INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL,
  CHECK (is_inbox = 0 OR client_id IS NULL)
) STRICT;
CREATE UNIQUE INDEX one_inbox ON project(is_inbox) WHERE is_inbox = 1;

CREATE TABLE status (
  id          BLOB PRIMARY KEY NOT NULL,
  project_id  BLOB NOT NULL REFERENCES project(id),
  name        TEXT NOT NULL,
  color       TEXT NOT NULL,
  category    TEXT NOT NULL CHECK (category IN ('todo','active','done','cancelled')),
  wip_limit   INTEGER CHECK (wip_limit IS NULL OR wip_limit > 0),
  sort_key    TEXT NOT NULL,
  deleted_at  INTEGER,
  updated_at  INTEGER NOT NULL,
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL
) STRICT;

CREATE TABLE task (
  id               BLOB PRIMARY KEY NOT NULL,
  project_id       BLOB NOT NULL REFERENCES project(id),
  parent_id        BLOB REFERENCES task(id),
  status_id        BLOB NOT NULL REFERENCES status(id),
  title            TEXT NOT NULL CHECK (length(trim(title)) > 0),
  description_md   TEXT NOT NULL DEFAULT '',
  priority         INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
                   -- 0 none, 1 low, 2 med, 3 high, 4 urgent
  start_date       TEXT,                  -- civil date
  due_date         TEXT,                  -- civil date
  due_time         TEXT,                  -- civil HH:MM, floating
  estimate_minutes INTEGER CHECK (estimate_minutes IS NULL OR estimate_minutes >= 0),
  sort_key         TEXT NOT NULL,
  series_id        BLOB,                  -- recurring series
  recurrence_rule  TEXT,                  -- RRULE subset, or 'AFTER:P3D'
  completed_at     INTEGER,
  archived_at      INTEGER,
  deleted_at       INTEGER,
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL,
  hlc              TEXT NOT NULL,
  device_id        BLOB NOT NULL,
  CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date),
  CHECK (due_time IS NULL OR due_date IS NOT NULL)
) STRICT;

CREATE TABLE tag (
  id         BLOB PRIMARY KEY NOT NULL,
  name       TEXT NOT NULL COLLATE NOCASE UNIQUE,
  color      TEXT NOT NULL,
  deleted_at INTEGER,
  updated_at INTEGER NOT NULL,
  hlc        TEXT NOT NULL,
  device_id  BLOB NOT NULL
) STRICT;

CREATE TABLE task_tag (
  task_id BLOB NOT NULL REFERENCES task(id),
  tag_id  BLOB NOT NULL REFERENCES tag(id),
  PRIMARY KEY (task_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE reminder (
  id            BLOB PRIMARY KEY NOT NULL,
  task_id       BLOB NOT NULL REFERENCES task(id),
  offset_min    INTEGER,                  -- relative to due date+time
  at            INTEGER,                  -- absolute instant
  fired_at      INTEGER,
  snoozed_until INTEGER,
  CHECK ((offset_min IS NULL) <> (at IS NULL))
) STRICT;

CREATE TABLE time_entry (
  id         BLOB PRIMARY KEY NOT NULL,
  task_id    BLOB NOT NULL REFERENCES task(id),
  started_at INTEGER NOT NULL,
  ended_at   INTEGER,                     -- NULL = timer running
  note       TEXT NOT NULL DEFAULT '',
  CHECK (ended_at IS NULL OR ended_at >= started_at)
) STRICT;
CREATE UNIQUE INDEX one_running_timer ON time_entry((1)) WHERE ended_at IS NULL;

CREATE TABLE attachment (
  id         BLOB PRIMARY KEY NOT NULL,
  task_id    BLOB NOT NULL REFERENCES task(id),
  file_name  TEXT NOT NULL,
  sha256     TEXT NOT NULL,               -- file at attachments/<sha256>
  size_bytes INTEGER NOT NULL,
  mime       TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE saved_view (
  id         BLOB PRIMARY KEY NOT NULL,
  name       TEXT NOT NULL,
  spec_json  TEXT NOT NULL,               -- versioned ViewSpec, validated by core
  sort_key   TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE setting (
  key   TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL                     -- JSON
) STRICT;

CREATE TABLE op_log (
  seq         INTEGER PRIMARY KEY,        -- monotonic
  group_id    BLOB NOT NULL,              -- one user action = one group = one undo
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL,
  entity      TEXT NOT NULL,              -- 'task', 'project', ...
  entity_id   BLOB NOT NULL,
  op          TEXT NOT NULL CHECK (op IN ('insert','update','delete')),
  before_json TEXT,                       -- changed fields only
  after_json  TEXT,
  undone      INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0,1))
) STRICT;
CREATE INDEX op_log_group ON op_log(group_id);

-- Full-text search (external content, kept in sync by triggers)
CREATE VIRTUAL TABLE task_fts USING fts5(
  title, description_md, tags,
  content='', contentless_delete=1,
  tokenize='unicode61 remove_diacritics 2',
  prefix='2 3'
);
```

### Indexes (live rows only)

```sql
CREATE INDEX task_board   ON task(project_id, status_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX task_due     ON task(due_date)     WHERE deleted_at IS NULL AND due_date IS NOT NULL;
CREATE INDEX task_parent  ON task(parent_id)    WHERE parent_id IS NOT NULL;
CREATE INDEX task_done    ON task(completed_at) WHERE completed_at IS NOT NULL;
CREATE INDEX project_client ON project(client_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX tag_tasks    ON task_tag(tag_id, task_id);
CREATE INDEX time_by_task ON time_entry(task_id, started_at);
```

## 3. Rules enforced by core (not expressible in SQL)

- A task's `status_id` must belong to its project. Moving a task to another
  project maps its status by category (todo → first todo status, etc.).
- `parent_id` must be in the same project and must not create a cycle.
- Completing a task sets `completed_at`; moving it to a `done` status does the
  same, and vice versa, so both stay consistent.
- Archiving a client archives its projects in the same op group; unarchiving
  restores exactly those.
- Deleting is soft (`deleted_at`). Purge of rows deleted > 30 days ago runs
  at startup; purged ids stay in `op_log` as tombstones for sync.
- Every change to a syncable row bumps `updated_at`, `hlc`, `device_id`.

## 4. Progress

`progress(project | parent task) = done / (total − cancelled)`, optionally
weighted by `estimate_minutes` (tasks without an estimate count as the median
estimate of the set, or 1 if none have one). Computed in SQL by core.

## 5. Time roll-up

`time_entry` → task → project → client. Reports: hours per client / project
per date range, CSV export.

## 6. Open questions

1. `cancelled` status category — included above (counts as closed, excluded
   from progress). Remove?
2. Client `contact` as one free-text field (simple) vs structured fields
   (name / email / phone). Draft uses free text.
3. Hourly rate per client/project for time reports — not in the draft (would
   lead toward invoicing, which is out of scope). Add?
