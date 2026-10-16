-- Migration 2: domain schema (docs/DATA_MODEL.md).
-- ids: UUIDv7 as 16-byte BLOB. Instants: UTC ms INTEGER. Civil dates: 'YYYY-MM-DD' TEXT.

CREATE TABLE client (
  id          BLOB PRIMARY KEY NOT NULL,
  name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
  color       TEXT NOT NULL,
  icon        TEXT,
  notes_md    TEXT NOT NULL DEFAULT '',
  contact     TEXT NOT NULL DEFAULT '',
  time_zone   TEXT,
  work_start  TEXT,
  work_end    TEXT,
  meetings_count_as_time INTEGER NOT NULL DEFAULT 1 CHECK (meetings_count_as_time IN (0,1)),
  sort_key    TEXT NOT NULL,
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
  code        TEXT,
  color       TEXT NOT NULL,
  icon        TEXT,
  state       TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','on_hold','completed')),
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
  name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
  color       TEXT NOT NULL,
  category    TEXT NOT NULL CHECK (category IN ('todo','active','done','cancelled')),
  wip_limit   INTEGER CHECK (wip_limit IS NULL OR wip_limit > 0),
  sort_key    TEXT NOT NULL,
  deleted_at  INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL
) STRICT;

-- `rid` is an explicit rowid so the FTS mapping survives VACUUM.
CREATE TABLE task (
  rid              INTEGER PRIMARY KEY,
  id               BLOB NOT NULL UNIQUE,
  project_id       BLOB NOT NULL REFERENCES project(id),
  parent_id        BLOB REFERENCES task(id),
  status_id        BLOB NOT NULL REFERENCES status(id),
  title            TEXT NOT NULL CHECK (length(trim(title)) > 0),
  description_md   TEXT NOT NULL DEFAULT '',
  priority         INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
  start_date       TEXT,
  due_date         TEXT,
  due_time         TEXT,
  estimate_minutes INTEGER CHECK (estimate_minutes IS NULL OR estimate_minutes >= 0),
  sort_key         TEXT NOT NULL,
  series_id        BLOB,
  recurrence_rule  TEXT,
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
  name       TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
  color      TEXT NOT NULL,
  deleted_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  hlc        TEXT NOT NULL,
  device_id  BLOB NOT NULL
) STRICT;

CREATE TABLE task_tag (
  task_id BLOB NOT NULL REFERENCES task(id),
  tag_id  BLOB NOT NULL REFERENCES tag(id),
  PRIMARY KEY (task_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE calendar_feed (
  id              BLOB PRIMARY KEY NOT NULL,
  name            TEXT NOT NULL,
  url             TEXT NOT NULL,
  color           TEXT NOT NULL,
  client_id       BLOB REFERENCES client(id),
  enabled         INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  last_fetched_at INTEGER,
  last_error      TEXT,
  etag            TEXT
) STRICT;

CREATE TABLE event (
  id              BLOB PRIMARY KEY NOT NULL,
  kind            TEXT NOT NULL CHECK (kind IN ('meeting','block')),
  title           TEXT NOT NULL CHECK (length(trim(title)) > 0),
  client_id       BLOB REFERENCES client(id),
  project_id      BLOB REFERENCES project(id),
  task_id         BLOB REFERENCES task(id),
  start_at        INTEGER,
  end_at          INTEGER,
  time_zone       TEXT NOT NULL,
  all_day_start   TEXT,
  all_day_end     TEXT,
  location        TEXT NOT NULL DEFAULT '',
  meeting_url     TEXT NOT NULL DEFAULT '',
  attendees       TEXT NOT NULL DEFAULT '',
  notes_md        TEXT NOT NULL DEFAULT '',
  status          TEXT NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed','tentative','cancelled')),
  counts_as_time  INTEGER NOT NULL DEFAULT 0 CHECK (counts_as_time IN (0,1)),
  flexible        INTEGER NOT NULL DEFAULT 0 CHECK (flexible IN (0,1)),
  recurrence_rule TEXT,
  feed_id         BLOB REFERENCES calendar_feed(id),
  external_uid    TEXT,
  deleted_at      INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  hlc             TEXT NOT NULL,
  device_id       BLOB NOT NULL,
  CHECK ((start_at IS NOT NULL AND end_at > start_at AND all_day_start IS NULL)
      OR (start_at IS NULL AND end_at IS NULL AND all_day_start IS NOT NULL
          AND all_day_end >= all_day_start))
) STRICT;
CREATE UNIQUE INDEX event_feed_uid ON event(feed_id, external_uid) WHERE feed_id IS NOT NULL;

CREATE TABLE event_exception (
  event_id       BLOB NOT NULL REFERENCES event(id),
  original_start INTEGER NOT NULL,
  cancelled      INTEGER NOT NULL DEFAULT 0 CHECK (cancelled IN (0,1)),
  start_at       INTEGER,
  end_at         INTEGER,
  title          TEXT,
  notes_md       TEXT,
  PRIMARY KEY (event_id, original_start)
) STRICT, WITHOUT ROWID;

CREATE TABLE working_hours (
  weekday    INTEGER NOT NULL CHECK (weekday BETWEEN 1 AND 7),
  start_time TEXT NOT NULL,
  end_time   TEXT NOT NULL,
  CHECK (start_time < end_time),
  PRIMARY KEY (weekday, start_time)
) STRICT, WITHOUT ROWID;

CREATE TABLE day_override (
  date      TEXT PRIMARY KEY NOT NULL,
  kind      TEXT NOT NULL CHECK (kind IN ('off','custom')),
  intervals TEXT,
  note      TEXT NOT NULL DEFAULT '',
  CHECK ((kind = 'custom') = (intervals IS NOT NULL))
) STRICT;

CREATE TABLE reminder (
  id            BLOB PRIMARY KEY NOT NULL,
  task_id       BLOB REFERENCES task(id),
  event_id      BLOB REFERENCES event(id),
  offset_min    INTEGER,
  at            INTEGER,
  fired_at      INTEGER,
  snoozed_until INTEGER,
  CHECK ((offset_min IS NULL) <> (at IS NULL)),
  CHECK ((task_id IS NULL) <> (event_id IS NULL))
) STRICT;

CREATE TABLE time_entry (
  id         BLOB PRIMARY KEY NOT NULL,
  task_id    BLOB REFERENCES task(id),
  event_id   BLOB REFERENCES event(id),
  project_id BLOB REFERENCES project(id),
  client_id  BLOB REFERENCES client(id),
  started_at INTEGER NOT NULL,
  ended_at   INTEGER,
  note       TEXT NOT NULL DEFAULT '',
  CHECK (ended_at IS NULL OR ended_at >= started_at),
  CHECK (task_id IS NOT NULL OR event_id IS NOT NULL OR project_id IS NOT NULL OR client_id IS NOT NULL)
) STRICT;
-- NULLs are distinct in unique indexes, so index a constant to allow one running timer.
CREATE UNIQUE INDEX one_running_timer ON time_entry(coalesce(ended_at, 0)) WHERE ended_at IS NULL;

CREATE TABLE attachment (
  id         BLOB PRIMARY KEY NOT NULL,
  task_id    BLOB NOT NULL REFERENCES task(id),
  file_name  TEXT NOT NULL,
  sha256     TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  mime       TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE saved_view (
  id         BLOB PRIMARY KEY NOT NULL,
  name       TEXT NOT NULL,
  spec_json  TEXT NOT NULL,
  sort_key   TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE setting (
  key   TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
) STRICT;

-- Append-only change log: every row change, including undo/redo (for audit and sync).
CREATE TABLE op_log (
  seq         INTEGER PRIMARY KEY,
  group_id    BLOB NOT NULL,
  kind        TEXT NOT NULL CHECK (kind IN ('do','undo','redo','system')),
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL,
  entity      TEXT NOT NULL,
  key_json    TEXT NOT NULL,
  op          TEXT NOT NULL CHECK (op IN ('insert','update','delete')),
  before_json TEXT,
  after_json  TEXT,
  at          INTEGER NOT NULL
) STRICT;
CREATE INDEX op_log_group ON op_log(group_id);

-- Undo/redo stack: one entry per user action. New actions drop the redo tail.
CREATE TABLE history (
  pos      INTEGER PRIMARY KEY,
  group_id BLOB NOT NULL UNIQUE,
  label    TEXT NOT NULL,
  undone   INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0,1)),
  at       INTEGER NOT NULL
) STRICT;

-- Full-text search over live tasks. Contentless; rowid = task.rid.
CREATE VIRTUAL TABLE task_fts USING fts5(
  title, description_md, tags,
  content = '', contentless_delete = 1,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER task_fts_ai AFTER INSERT ON task WHEN NEW.deleted_at IS NULL BEGIN
  INSERT INTO task_fts(rowid, title, description_md, tags)
  VALUES (NEW.rid, NEW.title, NEW.description_md,
          coalesce((SELECT group_concat(g.name, ' ') FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                    WHERE tt.task_id = NEW.id), ''));
END;

CREATE TRIGGER task_fts_au AFTER UPDATE OF title, description_md, deleted_at ON task BEGIN
  DELETE FROM task_fts WHERE rowid = OLD.rid;
  INSERT INTO task_fts(rowid, title, description_md, tags)
  SELECT NEW.rid, NEW.title, NEW.description_md,
         coalesce((SELECT group_concat(g.name, ' ') FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                   WHERE tt.task_id = NEW.id), '')
  WHERE NEW.deleted_at IS NULL;
END;

CREATE TRIGGER task_fts_ad AFTER DELETE ON task BEGIN
  DELETE FROM task_fts WHERE rowid = OLD.rid;
END;

CREATE TRIGGER task_tag_fts_ai AFTER INSERT ON task_tag BEGIN
  DELETE FROM task_fts WHERE rowid = (SELECT rid FROM task WHERE id = NEW.task_id);
  INSERT INTO task_fts(rowid, title, description_md, tags)
  SELECT t.rid, t.title, t.description_md,
         coalesce((SELECT group_concat(g.name, ' ') FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                   WHERE tt.task_id = t.id), '')
  FROM task t WHERE t.id = NEW.task_id AND t.deleted_at IS NULL;
END;

CREATE TRIGGER task_tag_fts_ad AFTER DELETE ON task_tag BEGIN
  DELETE FROM task_fts WHERE rowid = (SELECT rid FROM task WHERE id = OLD.task_id);
  INSERT INTO task_fts(rowid, title, description_md, tags)
  SELECT t.rid, t.title, t.description_md,
         coalesce((SELECT group_concat(g.name, ' ') FROM task_tag tt JOIN tag g ON g.id = tt.tag_id
                   WHERE tt.task_id = t.id), '')
  FROM task t WHERE t.id = OLD.task_id AND t.deleted_at IS NULL;
END;

CREATE TRIGGER tag_fts_au AFTER UPDATE OF name ON tag BEGIN
  DELETE FROM task_fts WHERE rowid IN
    (SELECT t.rid FROM task t JOIN task_tag tt ON tt.task_id = t.id WHERE tt.tag_id = NEW.id);
  INSERT INTO task_fts(rowid, title, description_md, tags)
  SELECT t.rid, t.title, t.description_md,
         coalesce((SELECT group_concat(g.name, ' ') FROM task_tag x JOIN tag g ON g.id = x.tag_id
                   WHERE x.task_id = t.id), '')
  FROM task t JOIN task_tag tt ON tt.task_id = t.id
  WHERE tt.tag_id = NEW.id AND t.deleted_at IS NULL;
END;

-- Indexes on live rows.
CREATE INDEX task_board     ON task(project_id, status_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX task_due       ON task(due_date) WHERE deleted_at IS NULL AND due_date IS NOT NULL;
CREATE INDEX task_parent    ON task(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX task_done      ON task(completed_at) WHERE completed_at IS NOT NULL;
CREATE INDEX task_status    ON task(status_id);
CREATE INDEX status_project ON status(project_id, sort_key);
CREATE INDEX project_client ON project(client_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX tag_tasks      ON task_tag(tag_id, task_id);
CREATE INDEX time_by_task   ON time_entry(task_id, started_at);
CREATE INDEX time_by_start  ON time_entry(started_at);
CREATE INDEX event_range    ON event(start_at, end_at) WHERE deleted_at IS NULL;
CREATE INDEX event_allday   ON event(all_day_start) WHERE deleted_at IS NULL AND all_day_start IS NOT NULL;
CREATE INDEX event_client   ON event(client_id, start_at) WHERE deleted_at IS NULL;
CREATE INDEX reminder_task  ON reminder(task_id);
CREATE INDEX reminder_event ON reminder(event_id);
