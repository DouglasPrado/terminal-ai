# Data Model: AI Terminal Workspace

**Phase 1 artifact** · Feature: [spec.md](./spec.md) · Plan: [plan.md](./plan.md)

## Storage boundaries

- **SQLite `app.db`** (via `rusqlite` with the `bundled` feature, FTS5 enabled) holds structured
  **state only** — projects, layouts, sessions history, bindings, usage snapshots, settings.
- **Markdown files** on disk hold portable **content** — skill instructions and memory bodies.
  The DB stores a `content_path` pointer, never the full mutable body (except FTS mirror +
  revisions). This keeps content diffable, portable, and out of the relational store.
- **Secrets never touch the DB** (Principle III — Credential-Safe by Default). API keys and OAuth
  tokens are read at runtime from the macOS **Keychain** and the provider CLIs' own files
  (`~/.claude/.credentials.json`, `~/.codex/auth.json`, …). No column below stores a credential.

Storage roots (macOS): `~/Library/Application Support/AITerminal/` → `app.db`, `config.toml`,
`skills/`, `memory/`, `sessions/`, `logs/`, `cache/`.

All timestamps are ISO-8601 `TEXT` (UTC). All `id` columns are `TEXT` (UUIDv4) unless noted.
`*_json` columns hold serialized JSON validated in the Rust layer, not by SQLite.

---

## Entities

### 1. `projects`
A cloned git repository the user works in.

```sql
CREATE TABLE projects (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    path              TEXT NOT NULL UNIQUE,          -- absolute repo path
    remote            TEXT,                          -- primary remote URL
    default_branch    TEXT,
    color             TEXT,                          -- accent hex, optional
    default_provider  TEXT,                          -- provider_profiles.id
    default_layout_id TEXT REFERENCES layout_presets(id) ON DELETE SET NULL,
    trusted           INTEGER NOT NULL DEFAULT 0,    -- 0/1; gates automation (Principle I)
    last_opened_at    TEXT,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_projects_last_opened ON projects(last_opened_at DESC);
```
Relationships: root of the graph. `trusted=0` means only a plain shell may open — no provider
automation, startup scripts, or local-config loading until the user trusts the project.

### 2. `worktrees`
A git worktree (branch on its own directory) belonging to a project.

```sql
CREATE TABLE worktrees (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    path        TEXT NOT NULL UNIQUE,                -- absolute worktree dir
    branch      TEXT NOT NULL,
    status      TEXT,                                -- clean|dirty|detached|missing
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_worktrees_project ON worktrees(project_id);
```
Relationships: `project 1—N worktrees`. Removal detaches the worktree without touching the main
working copy (worktree-manager). `status` is refreshed by git polling.

### 3. `workspaces`
A working tab bound to a project or worktree; owns one active layout tree.

```sql
CREATE TABLE workspaces (
    id               TEXT PRIMARY KEY,
    project_id       TEXT REFERENCES projects(id) ON DELETE CASCADE,      -- nullable
    worktree_id      TEXT REFERENCES worktrees(id) ON DELETE SET NULL,    -- nullable
    title            TEXT NOT NULL,
    position         INTEGER NOT NULL DEFAULT 0,     -- tab order in the top bar
    active_layout_id TEXT REFERENCES workspace_layouts(id) ON DELETE SET NULL,
    created_at       TEXT NOT NULL
);
CREATE INDEX idx_workspaces_project ON workspaces(project_id);
CREATE INDEX idx_workspaces_position ON workspaces(position);
```
Relationships: `project 1—N workspaces`; a workspace may pin a `worktree_id`. `position` restores
tab order on relaunch (Principle V — zero layout loss).

### 4. `workspace_layouts`
The serialized split tree for a workspace (one active row, history-capable).

```sql
CREATE TABLE workspace_layouts (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    layout_json  TEXT NOT NULL,                      -- serialized LayoutNode tree
    updated_at   TEXT NOT NULL
);
CREATE INDEX idx_layouts_workspace ON workspace_layouts(workspace_id, updated_at DESC);
```
`layout_json` conforms to `contracts/layout-node.schema.json` (pane | horizontal/vertical split
with `sizes[]` and `children[]`). Saved on every structural change so restore is lossless.

### 5. `layout_presets`
Named reusable layouts (Review / Implementation / Debug / Multi-agent) — US5/FR.

```sql
CREATE TABLE layout_presets (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    layout_json TEXT NOT NULL,                       -- LayoutNode tree w/ provider assignments
    created_at  TEXT NOT NULL
);
```
Relationships: referenced by `projects.default_layout_id`; a new workspace can be instantiated
from a preset (tree + each pane's assigned provider).

### 6. `panes`
A leaf of the layout tree with its visual + provider configuration.

```sql
CREATE TABLE panes (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    pane_key     TEXT NOT NULL,                      -- matches paneId in layout_json
    provider_id  TEXT REFERENCES provider_profiles(id) ON DELETE SET NULL,
    project_id   TEXT REFERENCES projects(id)  ON DELETE SET NULL,   -- nullable override
    worktree_id  TEXT REFERENCES worktrees(id) ON DELETE SET NULL,   -- nullable override
    title        TEXT,
    color        TEXT,                               -- per-agent accent
    size_hint    REAL,                               -- fractional size within its split
    created_at   TEXT NOT NULL,
    UNIQUE(workspace_id, pane_key)
);
CREATE INDEX idx_panes_workspace ON panes(workspace_id);
```
Relationships: `workspace 1—N panes`; `pane_key` links a DB pane to its node in `layout_json`. A
pane can override its project/worktree independently of the workspace (per-pane cwd).

### 7. `terminal_sessions`
Per-project **session history**. The live process is not persisted — only metadata plus the CLI's
own resume reference (FR-029/FR-030).

```sql
CREATE TABLE terminal_sessions (
    id          TEXT PRIMARY KEY,
    pane_id     TEXT REFERENCES panes(id) ON DELETE SET NULL,        -- nullable (pane may be gone)
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    worktree_id TEXT REFERENCES worktrees(id) ON DELETE SET NULL,
    provider_id TEXT NOT NULL,                       -- provider_profiles.id snapshot
    cwd         TEXT NOT NULL,
    title       TEXT,
    state       TEXT NOT NULL,                       -- starting|running|exited|error
    exit_code   INTEGER,                             -- nullable
    resume_ref  TEXT,                                -- CLI session id / transcript path for resume
    started_at  TEXT NOT NULL,
    ended_at    TEXT
);
CREATE INDEX idx_sessions_project_started ON terminal_sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_state ON terminal_sessions(state);
```
Lifecycle: `starting → running → (exited | error)`. On app close, running rows are marked `exited`
(process gone) but retained as history. Clicking a history row re-spawns the provider with its
native resume using `resume_ref`; a brand-new pane creates a fresh row with `resume_ref = NULL`.
When a provider has no resume capability, the row reopens fresh in the same `cwd`.

### 8. `provider_profiles`
An agent/command launch definition (built-in or user-defined).

```sql
CREATE TABLE provider_profiles (
    id         TEXT PRIMARY KEY,                     -- e.g. 'claude','codex','opencode','shell'
    label      TEXT NOT NULL,
    command    TEXT NOT NULL,                        -- executable (resolved via login-shell PATH)
    args_json  TEXT NOT NULL DEFAULT '[]',
    env_json   TEXT NOT NULL DEFAULT '{}',           -- extra env (no secrets)
    color      TEXT,                                 -- agent accent hex
    kind       TEXT NOT NULL,                        -- builtin|custom
    created_at TEXT NOT NULL
);
```
Built-ins seeded on first run: `claude`, `codex`, `opencode`, `shell`. Custom profiles (e.g.
`gemini`, `aider`, `npm test`) are user-defined. No credentials stored here.

### 9. `skills`
A reusable instruction set stored as Markdown, with metadata in the DB.

```sql
CREATE TABLE skills (
    id            TEXT PRIMARY KEY,
    slug          TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    version       TEXT NOT NULL DEFAULT '0.1.0',
    description   TEXT,
    providers_json TEXT NOT NULL DEFAULT '[]',       -- ["claude","codex","opencode"]
    content_path  TEXT NOT NULL,                     -- skills/<slug>/instructions.md
    scope_default TEXT,                              -- suggested default scope
    created_at    TEXT NOT NULL
);
```
Relationships: `skill 1—N skill_bindings`. Content lives under `Application Support/AITerminal/
skills/<slug>/` (`skill.toml` + `instructions.md`).

### 10. `skill_bindings`
Where a skill is active, and exactly what the app created so removal is reversible (Principle III).

```sql
CREATE TABLE skill_bindings (
    id                    TEXT PRIMARY KEY,
    skill_id              TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    scope                 TEXT NOT NULL,             -- global|project|worktree|workspace|session
    scope_ref_id          TEXT,                      -- id of the scoped entity (NULL for global)
    enabled               INTEGER NOT NULL DEFAULT 1,
    precedence            INTEGER NOT NULL,          -- resolved rank; higher wins
    applied_artifacts_json TEXT NOT NULL DEFAULT '[]', -- files/blocks the app wrote, per provider
    created_at            TEXT NOT NULL
);
CREATE INDEX idx_bindings_skill ON skill_bindings(skill_id);
CREATE INDEX idx_bindings_scope ON skill_bindings(scope, scope_ref_id);
```
Precedence (resolved into `precedence`): **session > workspace > worktree > project > global**.
`applied_artifacts_json` records every path/marker written into a provider's config so deactivation
deletes only app-created content and never the provider's own configuration.

### 11. `memory_entries`
Scoped, typed memory. Body is Markdown on disk; DB holds metadata + pointer.

```sql
CREATE TABLE memory_entries (
    id           TEXT PRIMARY KEY,
    scope        TEXT NOT NULL,                      -- global|project|worktree|workspace|session
    scope_ref_id TEXT,                               -- id of the scoped entity (NULL for global)
    type         TEXT NOT NULL,                      -- fact|decision|constraint|preference|
                                                     -- glossary|known_issue|command|todo
    title        TEXT NOT NULL,
    content_path TEXT NOT NULL,                      -- memory/<scope>/<id>.md
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX idx_memory_scope ON memory_entries(scope, scope_ref_id);
CREATE INDEX idx_memory_type ON memory_entries(type);
```
Isolation guarantee (FR-024): queries always filter by `(scope, scope_ref_id)`, so project-scoped
memory is offered only to that project's agents and never leaks across projects.

### 12. `memory_revisions`
Append-only version history for a memory entry.

```sql
CREATE TABLE memory_revisions (
    id           TEXT PRIMARY KEY,
    entry_id     TEXT NOT NULL REFERENCES memory_entries(id) ON DELETE CASCADE,
    content_path TEXT NOT NULL,                      -- memory/<scope>/<id>/<rev>.md snapshot
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_revisions_entry ON memory_revisions(entry_id, created_at DESC);
```
Relationships: `memory_entry 1—N revisions`; a new revision is written on each edit.

### 13. `memory_fts`
FTS5 full-text index over memory title + body for keyword search.

```sql
CREATE VIRTUAL TABLE memory_fts USING fts5(
    title,
    body,
    entry_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Keep FTS in sync with the source-of-truth body (body text supplied by the app on write):
CREATE TRIGGER memory_fts_ai AFTER INSERT ON memory_entries BEGIN
    INSERT INTO memory_fts(rowid, title, body, entry_id)
    VALUES (new.rowid, new.title, '', new.id);      -- body backfilled by app after file write
END;
CREATE TRIGGER memory_fts_ad AFTER DELETE ON memory_entries BEGIN
    DELETE FROM memory_fts WHERE rowid = old.rowid;
END;
CREATE TRIGGER memory_fts_au AFTER UPDATE ON memory_entries BEGIN
    UPDATE memory_fts SET title = new.title WHERE rowid = new.rowid;
END;
```
Note: because the body is a Markdown file, the app writes the current body text into
`memory_fts.body` on each save (the triggers cover title/lifecycle; body is set explicitly by the
memory-manager after the file is written). Search returns entries ranked by `bm25(memory_fts)`,
filtered by scope.

### 14. `usage_snapshots`
Last known usage per provider — upserted by the single poller (Principle IV).

```sql
CREATE TABLE usage_snapshots (
    provider_id  TEXT PRIMARY KEY,                   -- 'claude'|'codex'|'opencode'(→openrouter)
    snapshot_json TEXT NOT NULL,                     -- consumption + reset timers + limits
    fetched_at   TEXT NOT NULL,
    stale        INTEGER NOT NULL DEFAULT 0          -- 1 when offline/expired → show last known
);
```
Exactly one row per provider; the UI reads only these rows (never triggers its own fetch). When
offline or rate-limited, `stale = 1` and the prior `snapshot_json` is shown.

### 15. `app_settings`
Key/value application preferences and caches.

```sql
CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
```
Seeded keys: `login_shell_env` (resolved PATH + allowed vars cache), `project_root_dirs`
(default `["~/www"]`), `keybindings`, `scrollback_lines`, `memory_auto_capture` (**default
`false`** — Principle III), `usage_refresh_seconds` (≥300), `theme` metadata.

---

## Enumerations

- **worktree.status**: `clean` · `dirty` · `detached` · `missing`
- **terminal_sessions.state**: `starting` · `running` · `exited` · `error`
- **provider_profiles.kind**: `builtin` · `custom`
- **skill_bindings.scope / memory_entries.scope**: `global` · `project` · `worktree` ·
  `workspace` · `session` (precedence high→low: session > workspace > worktree > project > global)
- **memory_entries.type**: `fact` · `decision` · `constraint` · `preference` · `glossary` ·
  `known_issue` · `command` · `todo`

## Migrations

- Schema is versioned with **`refinery`** using sequential SQL files in the `persistence` crate
  (`persistence/migrations/V001__init.sql`, `V002__…`). Migrations run on app start against
  `app.db`.
- FTS5 requires the **`bundled`** (and FTS-enabled) `rusqlite` feature; enable
  `PRAGMA foreign_keys = ON;` on every connection.
- Enum values are stored as `TEXT` (validated in Rust) rather than SQLite `CHECK` constraints, to
  keep migrations additive as new provider/memory types appear.

## ER overview

```text
projects ──1:N── worktrees
   │  └────1:N── workspaces ──1:N── panes ──1:N── terminal_sessions  (session history)
   │                   └──1:N── workspace_layouts (layout_json → LayoutNode tree)
   └── default_layout_id ─▶ layout_presets

provider_profiles ─▶ referenced by panes.provider_id / terminal_sessions.provider_id

skills ──1:N── skill_bindings          (scope + applied_artifacts_json)
memory_entries ──1:N── memory_revisions
memory_entries ──mirror──▶ memory_fts  (FTS5 search)
usage_snapshots  (one row per provider) ◀── single UsagePoller
app_settings     (key/value: env cache, root dirs, flags)
```
