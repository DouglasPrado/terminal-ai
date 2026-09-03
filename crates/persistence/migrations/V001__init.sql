PRAGMA foreign_keys = ON;
CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE, remote TEXT, default_branch TEXT, color TEXT, default_provider TEXT, default_layout_id TEXT REFERENCES layout_presets(id) ON DELETE SET NULL, trusted INTEGER NOT NULL DEFAULT 0, last_opened_at TEXT, created_at TEXT NOT NULL);
CREATE INDEX idx_projects_last_opened ON projects(last_opened_at DESC);
CREATE TABLE worktrees (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE, path TEXT NOT NULL UNIQUE, branch TEXT NOT NULL, status TEXT, created_at TEXT NOT NULL);
CREATE INDEX idx_worktrees_project ON worktrees(project_id);
CREATE TABLE workspaces (id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id) ON DELETE CASCADE, worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL, title TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0, active_layout_id TEXT, created_at TEXT NOT NULL);
CREATE INDEX idx_workspaces_project ON workspaces(project_id);
CREATE INDEX idx_workspaces_position ON workspaces(position);
CREATE TABLE workspace_layouts (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE, layout_json TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX idx_layouts_workspace ON workspace_layouts(workspace_id, updated_at DESC);
CREATE TABLE layout_presets (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, layout_json TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE provider_profiles (id TEXT PRIMARY KEY, label TEXT NOT NULL, command TEXT NOT NULL, args_json TEXT NOT NULL DEFAULT '[]', env_json TEXT NOT NULL DEFAULT '{}', color TEXT, kind TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE panes (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE, pane_key TEXT NOT NULL, provider_id TEXT REFERENCES provider_profiles(id) ON DELETE SET NULL, project_id TEXT REFERENCES projects(id) ON DELETE SET NULL, worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL, title TEXT, color TEXT, size_hint REAL, created_at TEXT NOT NULL, UNIQUE(workspace_id, pane_key));
CREATE INDEX idx_panes_workspace ON panes(workspace_id);
CREATE TABLE terminal_sessions (id TEXT PRIMARY KEY, pane_id TEXT REFERENCES panes(id) ON DELETE SET NULL, project_id TEXT REFERENCES projects(id) ON DELETE CASCADE, worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL, provider_id TEXT NOT NULL, cwd TEXT NOT NULL, title TEXT, state TEXT NOT NULL, exit_code INTEGER, resume_ref TEXT, started_at TEXT NOT NULL, ended_at TEXT);
CREATE INDEX idx_sessions_project_started ON terminal_sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_state ON terminal_sessions(state);
CREATE TABLE skills (id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, name TEXT NOT NULL, version TEXT NOT NULL DEFAULT '0.1.0', description TEXT, providers_json TEXT NOT NULL DEFAULT '[]', content_path TEXT NOT NULL, scope_default TEXT, created_at TEXT NOT NULL);
CREATE TABLE skill_bindings (id TEXT PRIMARY KEY, skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE, scope TEXT NOT NULL, scope_ref_id TEXT, enabled INTEGER NOT NULL DEFAULT 1, precedence INTEGER NOT NULL, applied_artifacts_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL);
CREATE INDEX idx_bindings_skill ON skill_bindings(skill_id);
CREATE INDEX idx_bindings_scope ON skill_bindings(scope, scope_ref_id);
CREATE TABLE memory_entries (id TEXT PRIMARY KEY, scope TEXT NOT NULL, scope_ref_id TEXT, type TEXT NOT NULL, title TEXT NOT NULL, content_path TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
CREATE INDEX idx_memory_scope ON memory_entries(scope, scope_ref_id);
CREATE INDEX idx_memory_type ON memory_entries(type);
CREATE TABLE memory_revisions (id TEXT PRIMARY KEY, entry_id TEXT NOT NULL REFERENCES memory_entries(id) ON DELETE CASCADE, content_path TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE INDEX idx_revisions_entry ON memory_revisions(entry_id, created_at DESC);
CREATE VIRTUAL TABLE memory_fts USING fts5(title, body, entry_id UNINDEXED, tokenize = 'unicode61 remove_diacritics 2');
CREATE TRIGGER memory_fts_ai AFTER INSERT ON memory_entries BEGIN INSERT INTO memory_fts(rowid, title, body, entry_id) VALUES (new.rowid, new.title, '', new.id); END;
CREATE TRIGGER memory_fts_ad AFTER DELETE ON memory_entries BEGIN DELETE FROM memory_fts WHERE rowid = old.rowid; END;
CREATE TRIGGER memory_fts_au AFTER UPDATE ON memory_entries BEGIN UPDATE memory_fts SET title = new.title WHERE rowid = new.rowid; END;
CREATE TABLE usage_snapshots (provider_id TEXT PRIMARY KEY, snapshot_json TEXT NOT NULL, fetched_at TEXT NOT NULL, stale INTEGER NOT NULL DEFAULT 0);
CREATE TABLE app_settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
INSERT INTO provider_profiles(id,label,command,args_json,env_json,color,kind,created_at) VALUES
('claude','Claude','claude','[]','{}','#e879f9','builtin',datetime('now')),
('codex','Codex','codex','[]','{}','#22d3ee','builtin',datetime('now')),
('opencode','OpenCode','opencode','[]','{}','#a78bfa','builtin',datetime('now')),
('shell','Shell','/bin/zsh','["-l"]','{}','#a3a3a3','builtin',datetime('now'));
INSERT INTO app_settings(key,value_json) VALUES
('project_root_dirs','["~/www"]'),('keybindings','{}'),('scrollback_lines','10000'),('memory_auto_capture','false'),('usage_refresh_seconds','300');

