-- Migration 5: which reminders have fired (local bookkeeping, not synced).
-- Keys look like `ev:<event id>:<occurrence start>`, `task:<id>:<due>`,
-- `digest:<date>`; old rows are pruned.
CREATE TABLE notification_log (
  key      TEXT PRIMARY KEY NOT NULL,
  fired_at INTEGER NOT NULL
) STRICT;
CREATE INDEX notification_log_age ON notification_log(fired_at);
