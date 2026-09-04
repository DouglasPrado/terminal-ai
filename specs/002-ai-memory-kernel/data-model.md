# Data Model: ai-memory as the Memory Kernel

## Storage boundaries

This feature moves the memory **content** out of Terminal AI's storage entirely.

| Store | Owner | Holds |
| --- | --- | --- |
| Kernel wiki (`<ai-memory data dir>/wiki/`, git-versioned) | ai-memory | Every memory page. The source of truth. Shared with the user's own ai-memory usage. |
| Kernel index (`<ai-memory data dir>/db/memory.sqlite`) | ai-memory | Derived FTS5 / entity / graph / embedding index. Rebuildable from the wiki. Terminal AI never opens it. |
| `app.db` | Terminal AI | Terminal AI's own state, plus **records about** the kernel: what wiring was applied where, and what was imported. Never memory content. |
| macOS Keychain | the OS | A bearer token, only when attaching to a server that requires one. |
| Legacy `memory_entries` / `memory_revisions` / `memory_fts` + `AppPaths.memory/*.md` | Terminal AI | Read-only legacy. The migration source and the rollback path. Not dropped. |

Rule carried over from 001 and now sharper: **`app.db` holds structured state, never memory content
and never secrets.** After this feature it does not even hold a mirror of the content.

## Entities

### 1. `memory_wiring_bindings` (new, `V005`)

Records exactly what the app wrote into which agent configuration file, for which project, with
enough information to remove precisely that and to detect drift.

```sql
CREATE TABLE memory_wiring_bindings (
    id            TEXT PRIMARY KEY,
    agent         TEXT NOT NULL,             -- claude-code|codex|opencode
    kind          TEXT NOT NULL,             -- mcp|hooks
    scope         TEXT NOT NULL,             -- global|project|worktree|workspace|session
    scope_ref_id  TEXT,                      -- NULL for global
    enabled       INTEGER NOT NULL DEFAULT 1,
    artifacts_json TEXT NOT NULL DEFAULT '[]',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE(agent, kind, scope, scope_ref_id)
);
CREATE INDEX idx_memory_wiring_scope ON memory_wiring_bindings(scope, scope_ref_id);
```

`artifacts_json` is a `Vec<MemoryWiringArtifact>`:

```
MemoryWiringArtifact {
  agent, kind,
  path,                    // the configuration file touched
  created_file: bool,      // true = the file did not exist before the app wrote it
  backup_path: Option,     // pre-apply snapshot; only when created_file == false
  before_sha256: Option,
  after_sha256,            // what the app left behind — the drift gate
  binary_path,             // the sidecar path baked into hook commands; staleness detector
  marker,                  // "terminal-ai-memory:<agent>:<kind>"
  applied_at
}
```

> Not modelled as a reuse of `skill_bindings`: that table's `skill_id` is a
> `REFERENCES skills(id) ON DELETE CASCADE`, and memory wiring has no skill. Not modelled as a reuse
> of `skill-manager`'s `AppliedArtifact` either — that type assumes whole-file ownership
> (`remove()` deletes the file), while ai-memory merges into files it does not own.

**Drift detection (FR-057, FR-059)**: removal compares `sha256(current)` against `after_sha256`;
startup compares `binary_path` against the resolved sidecar path and marks the binding `stale` when
they differ, because hook commands bake an absolute path.

### 2. `memory_migration_log` (new, `V005`)

One row per legacy entry imported, keyed by the legacy id, which is what makes the import idempotent
and undoable.

```sql
CREATE TABLE memory_migration_log (
    entry_id    TEXT PRIMARY KEY,            -- the legacy memory_entries.id
    workspace   TEXT NOT NULL,
    project     TEXT NOT NULL,
    page_path   TEXT NOT NULL,               -- the kernel address it landed at
    body_sha256 TEXT NOT NULL,
    imported_at TEXT NOT NULL
);
CREATE INDEX idx_memory_migration_page ON memory_migration_log(page_path);
```

**Idempotency has three independent layers**, so losing any one of them still holds:
1. `entry_id` as primary key — a second run skips what is logged.
2. `body_sha256` — a changed body is rewritten, and the kernel's own page versioning supersedes
   rather than duplicating.
3. A deterministic `page_path` derived from the legacy id — so even with the log lost (a restored old
   `app.db`), the same entry maps to the same page and the write upserts.

The log is written **per item, not batched**, so an interrupted run resumes exactly where it stopped
(FR-053).

### 3. `app_settings` additions (`V005`)

Settings, not secrets. Seeded so a fresh install behaves correctly with no user action.

```sql
INSERT OR IGNORE INTO app_settings(key, value_json) VALUES
  ('memory_kernel_server_url',   '"http://127.0.0.1:49374"'),
  ('memory_kernel_auto_start',   'true'),
  ('memory_kernel_binary',       'null'),   -- an explicit override; the sidecar is tried first
  ('memory_kernel_hybrid_search','false');  -- opt-in; enabling it fetches ~87 MB (FR-062)
```

`memory_auto_capture` (seeded `false` in `V001`) stops being inert: it becomes the master gate on
lifecycle-capture wiring (FR-058). Per-project consent is the wiring binding's existence; the setting
is the machine-wide off switch above it.

### 4. Kernel page (not a Terminal AI table — the shape the app reads and writes)

```
workspace  = "default"
project    = basename(repository root)          -- what an agent derives on its own
path       = terminal-ai/<scope>/<...>/<slug>-<id8>.md
frontmatter:
  terminal_ai_type       fact|decision|constraint|preference|glossary|known_issue|command|todo
  terminal_ai_scope      global|project|worktree|workspace|session
  terminal_ai_ref_id     the scoped entity's id (absent for global)
  terminal_ai_entry_id   the legacy uuid — imported pages only
  terminal_ai_created_at RFC3339
body       = markdown, first `# H1` is the title upstream derives
```

Every key is prefixed `terminal_ai_` so it can never collide with ai-memory's own frontmatter. The
type also appears as a path segment, so filtering by type needs no page parse.

**Pages without this frontmatter are agent-authored** and MUST still list (FR-049), degrading to
`type = fact` with a path-derived title. Refusing to show them would blind the panel to exactly the
content this feature exists to surface.

**Path validation**: `/api/v1`'s page route is a `{*path}` wildcard, so traversal is a real risk.
Reject `..`, a leading `/`, NUL, anything outside `[A-Za-z0-9._/-]`, and anything over 200 chars —
matching the `validate_slug` discipline already in `skill-manager`.

## Enumerations

- **kernel state**: `not_installed` · `probing` · `starting` · `ready` · `attached` · `degraded` ·
  `port_conflict` · `failed` — each carries actionable guidance (FR-044). `attached` implies
  `owned = false`, which is the sole gate on terminate/restart (FR-039).
- **wiring kind**: `mcp` · `hooks`.
- **wiring status**: `applied` · `stale` (binary moved) · `drifted` (file edited after apply) ·
  `unmanaged` (a pre-existing entry the app did not create and must not touch).
- **handoff state**: `open` · `accepted` · `expired`.
- **memory type / scope**: unchanged from 001 — the `MemoryType` and `ScopeLevel` enums in
  `crates/domain` are reused verbatim, so the frontend's existing wire types keep working.

## Migrations

`refinery` sequential SQL in `crates/persistence/migrations/`. `V005__memory_kernel.sql` is
**purely additive**: two new tables and four seeded settings. It drops nothing.

Not dropping the legacy memory tables is deliberate, for the same reason already recorded for
`projects.trusted` in `docs/deferred.md`: a destructive migration against users' existing `app.db`
buys nothing, and here those tables are also the rollback path. A follow-up entry in
`docs/deferred.md` records dropping them after two releases.

The `tables >= 15` assertion in `crates/persistence/src/lib.rs` is bumped to 17.

## ER overview

```
memory_wiring_bindings ──(scope, scope_ref_id)──▶ projects | worktrees | workspaces | sessions
memory_migration_log   ──(entry_id)────────────▶ memory_entries   (legacy, read-only)
memory_migration_log   ──(workspace, project, page_path)──▶ kernel page   (outside app.db)

legacy (read-only, retained):
  memory_entries ──1:N── memory_revisions
  memory_entries ──mirror──▶ memory_fts
```
