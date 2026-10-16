-- Migration 3: indexes that let list views read rows in sort order and stop
-- after one page, instead of scanning and sorting every task (SPEC §7 budgets).
--
-- "Open" tasks are exactly those with completed_at IS NULL (core keeps
-- completed_at in sync with closed status categories), so the hottest filter
-- needs no join. Undated tasks sort last via coalesce(..., '~') ('~' > digits).
-- project_id / status_id are trailing columns so counts are index-only.

CREATE INDEX task_open_due ON task(
  coalesce(due_date, '~'), coalesce(due_time, '~'), sort_key, project_id, status_id
) WHERE deleted_at IS NULL AND archived_at IS NULL AND completed_at IS NULL;

CREATE INDEX task_open_manual ON task(sort_key, project_id, status_id)
  WHERE deleted_at IS NULL AND archived_at IS NULL AND completed_at IS NULL;

CREATE INDEX task_open_priority ON task(
  priority DESC, coalesce(due_date, '~'), sort_key, project_id, status_id
) WHERE deleted_at IS NULL AND archived_at IS NULL AND completed_at IS NULL;

CREATE INDEX task_live_due ON task(
  coalesce(due_date, '~'), coalesce(due_time, '~'), sort_key, project_id, status_id
) WHERE deleted_at IS NULL;

CREATE INDEX task_live_manual ON task(sort_key, project_id, status_id) WHERE deleted_at IS NULL;

CREATE INDEX task_live_created ON task(created_at, project_id, status_id) WHERE deleted_at IS NULL;
