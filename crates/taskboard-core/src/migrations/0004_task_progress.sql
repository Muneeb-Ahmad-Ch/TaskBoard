-- Migration 4: manual progress for tasks without subtasks (0-100).
-- Tasks with subtasks derive progress from them; closed tasks are 100%.
ALTER TABLE task ADD COLUMN progress INTEGER CHECK (progress IS NULL OR progress BETWEEN 0 AND 100);
