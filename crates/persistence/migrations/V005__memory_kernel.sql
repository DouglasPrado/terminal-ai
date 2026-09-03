-- Feature 002: ai-memory becomes the memory kernel.
--
-- Memory CONTENT leaves this database entirely — it now lives in the kernel's git-versioned wiki,
-- in a store shared with whatever ai-memory the user runs outside the app. What stays here is
-- records *about* the kernel: what wiring we wrote where, and what we imported.
--
-- Purely additive. The legacy memory_entries / memory_revisions / memory_fts tables and their
-- triggers are deliberately NOT dropped: they are the migration's source and its rollback path.
-- Dropping them is recorded in docs/deferred.md for a later release, on the same reasoning already
-- applied to projects.trusted.

-- What the app wrote into an agent's configuration, precisely enough to remove only that.
--
-- Not a reuse of skill_bindings: that table's skill_id is a FK into skills, and memory wiring has
-- no skill. Not a reuse of skill-manager's AppliedArtifact either — that type assumes whole-file
-- ownership (remove() deletes the file), while ai-memory MERGES into files it does not own. Hence
-- before/after hashes and a backup, rather than a marker and a delete.
CREATE TABLE memory_wiring_bindings (
    id             TEXT PRIMARY KEY,
    agent          TEXT NOT NULL,                 -- claude-code|codex|opencode
    kind           TEXT NOT NULL,                 -- mcp|hooks
    scope          TEXT NOT NULL,                 -- global|project|worktree|workspace|session
    scope_ref_id   TEXT,                          -- NULL for global
    enabled        INTEGER NOT NULL DEFAULT 1,
    artifacts_json TEXT NOT NULL DEFAULT '[]',    -- Vec<MemoryWiringArtifact>
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE(agent, kind, scope, scope_ref_id)
);

CREATE INDEX idx_memory_wiring_scope ON memory_wiring_bindings(scope, scope_ref_id);

-- One row per legacy entry imported into the kernel, keyed by the legacy id.
--
-- This is the authority layer of the import's idempotency; body_sha256 is the second, and the
-- deterministic page_path is the third, which is what keeps a re-run safe even if this table is
-- lost with an old app.db restore.
CREATE TABLE memory_migration_log (
    entry_id    TEXT PRIMARY KEY,                 -- legacy memory_entries.id
    workspace   TEXT NOT NULL,
    project     TEXT NOT NULL,
    page_path   TEXT NOT NULL,
    body_sha256 TEXT NOT NULL,
    imported_at TEXT NOT NULL
);

CREATE INDEX idx_memory_migration_page ON memory_migration_log(page_path);

-- Kernel settings. Settings, not secrets: a bearer token lives only in the macOS Keychain.
--
-- memory_kernel_hybrid_search defaults to false on purpose. Enabling it is what authorises the
-- kernel's ~87 MB local embedding-model download; until then the app starts the kernel with
-- AI_MEMORY_EMBEDDING_PROVIDER=none so the first run reaches no network the user did not ask for.
INSERT OR IGNORE INTO app_settings(key, value_json) VALUES
  ('memory_kernel_server_url',    '"http://127.0.0.1:49374"'),
  ('memory_kernel_auto_start',    'true'),
  ('memory_kernel_binary',        'null'),
  ('memory_kernel_hybrid_search', 'false');
