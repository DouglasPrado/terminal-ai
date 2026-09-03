-- Per-workspace project folder: when set, the sidebar lists only the repositories found under
-- this path instead of the globally configured project roots.
ALTER TABLE workspaces ADD COLUMN root_path TEXT;

-- Archived projects stay in the database (and keep being rediscovered) but are hidden from the
-- sidebar until restored. NULL means active.
ALTER TABLE projects ADD COLUMN archived_at TEXT;
CREATE INDEX idx_projects_archived ON projects(archived_at);
