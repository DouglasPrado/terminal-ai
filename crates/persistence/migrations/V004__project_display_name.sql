-- A user-chosen name for a project. Kept separate from `name`, which discovery rewrites from the
-- directory on every scan and would otherwise overwrite the choice.
ALTER TABLE projects ADD COLUMN display_name TEXT;
