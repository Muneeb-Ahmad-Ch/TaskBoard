-- Migration 6: indexes for the queries the app runs before its first paint
-- (SPEC §7 cold-start budget on large databases).

-- Per-project task counts in the sidebar: one index-only pass over live
-- top-level tasks instead of reading every task row.
CREATE INDEX task_top_counts ON task(project_id, status_id)
  WHERE deleted_at IS NULL AND parent_id IS NULL;

-- Opening a database seeds the hybrid logical clock from max(hlc); without
-- this it scans the whole append-only log.
CREATE INDEX op_log_hlc ON op_log(hlc);
