-- Migration 7: user-named sidebar sections (docs/superpowers/specs/
-- 2026-10-17-custom-sections-design.md). A section holds groups (the
-- `client` table) and projects; the Inbox belongs to no section.
-- Existing data is assigned to "Clients" / "Personal" by a Rust hook.

CREATE TABLE section (
  id          BLOB PRIMARY KEY NOT NULL,
  name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
  item_name   TEXT NOT NULL DEFAULT '',
  color       TEXT NOT NULL,
  sort_key    TEXT NOT NULL,
  deleted_at  INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  hlc         TEXT NOT NULL,
  device_id   BLOB NOT NULL
) STRICT;

ALTER TABLE client ADD COLUMN section_id BLOB REFERENCES section(id);
ALTER TABLE project ADD COLUMN section_id BLOB REFERENCES section(id);

CREATE INDEX client_section ON client(section_id, sort_key) WHERE deleted_at IS NULL;
CREATE INDEX project_section ON project(section_id, client_id, sort_key) WHERE deleted_at IS NULL;
