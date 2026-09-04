# Contract: Tauri Commands (Typed Frontend ↔ Rust Boundary)

This is the **closed** set of commands the WebView frontend may invoke. There is **no**
generic command-execution primitive (Principle I). Every command is a typed Tauri `#[command]`
handler in `src-tauri` that delegates to a crate and, before acting, runs the **Validation**
listed for it. Anything not in this catalog is not callable from the frontend.

**Global validation applied to every command** (in addition to per-command notes):
- The referenced `projectId` (if any) MUST exist. There is no per-project trust gate: a project
  reachable from a configured root is launchable. The allowed-path rule below is the boundary.
- Any filesystem `path`/`cwd` MUST resolve (canonicalized, symlinks followed) to a location
  under a configured project root or worktree directory. Paths escaping via `..` are rejected.
- Any `providerId` MUST be a known built-in (`claude` | `codex` | `opencode` | `shell`) or a
  user-defined profile id.
- The environment used to launch processes is the **resolved login-shell env** (see
  `resolve_env`), never the raw GUI-inherited env.
- All results are `Result<T, AppError>`; `AppError` carries a machine code + human message.

> **Terminal output is NOT a command result.** `create_session` opens a Tauri
> `ipc::Channel<TerminalChunk>`; the Rust side streams batched output over that channel
> (see `contracts/daemon-events.md`). Commands here are request/response only.

---

## Sessions

### `create_session`
Purpose: start a PTY session for a provider and open its output channel.
```ts
// request
{ projectId?: string, worktreeId?: string, providerId: string,
  cols: number, rows: number, onOutput: Channel<TerminalChunk>,
  resume?: { kind: "continue" } | { kind: "byId", ref: string } }
// response
{ sessionId: string, pid: number, title: string, state: SessionState }
```
Validation: `cwd` derived from
project/worktree is inside an allowed root; provider known & its executable detected; env
resolved. If `resume` is set, the provider MUST advertise a matching `ResumeCapability`.

### `write_input`
Purpose: send raw bytes (keystrokes/paste) to a session's PTY.
```ts
{ sessionId: string, data: string /* utf-8 */ } → { ok: true }
```
Validation: `sessionId` belongs to the current app instance and is alive. Paste size is
bounded; no interpretation of the bytes.

### `resize_session`
Purpose: propagate new terminal dimensions to the PTY.
```ts
{ sessionId: string, cols: number, rows: number } → { ok: true }
```
Validation: session alive; `cols`/`rows` within sane bounds (1..1000).

### `send_signal`
Purpose: deliver a signal (e.g. SIGINT, SIGTERM) to the session's process group.
```ts
{ sessionId: string, signal: "SIGINT" | "SIGTERM" | "SIGKILL" | "SIGHUP" } → { ok: true }
```
Validation: session alive; signal in the allow-list above.

### `close_session`
Purpose: terminate a session and close its channel.
```ts
{ sessionId: string } → { ok: true, exitCode?: number }
```
Validation: session belongs to this instance. Records an end entry in session history.

### `restart_session`
Purpose: kill and respawn the same provider in the same cwd (fresh).
```ts
{ sessionId: string } → { sessionId: string, pid: number }
```
Validation: same as `create_session`; reuses the prior `LaunchContext` minus `resume`.

### `list_sessions`
Purpose: enumerate live sessions for UI reconciliation.
```ts
{} → { sessions: Array<{ sessionId: string, providerId: string, projectId?: string,
        worktreeId?: string, title: string, state: SessionState, pid: number }> }
```
Validation: none beyond global.

### `get_scrollback`
Purpose: fetch buffered output for re-attaching a pane (bounded).
```ts
{ sessionId: string, maxBytes?: number } → { data: string, truncated: boolean }
```
Validation: session alive; `maxBytes` clamped to the configured scrollback cap.

### `get_session_history`
Purpose: list a project's past sessions (resumable history — FR-029).
```ts
{ projectId: string, limit?: number } → { entries: Array<SessionHistoryEntry> }
// SessionHistoryEntry
{ id: string, providerId: string, worktreeId?: string, cwd: string,
  startedAt: string, endedAt?: string, title: string,
  resume?: { kind: "continue" } | { kind: "byId", ref: string } }
```
Validation: project exists.

### `resume_session`
Purpose: reopen a past session from history using the agent's native resume (FR-030).
```ts
{ historyId: string, cols: number, rows: number, onOutput: Channel<TerminalChunk> }
  → { sessionId: string, pid: number, resumed: boolean }
```
Validation: history entry exists; provider still detected; if the provider has
`ResumeCapability::None`, a fresh session is started in the same cwd and `resumed:false` is
returned.

---

## Layout / Workspaces

### `list_workspaces`
```ts
{} → { workspaces: Array<{ id: string, title: string, projectId?: string, active: boolean, rootPath?: string }> }
```
Validation: none.

### `create_workspace`
```ts
{ title?: string, projectId?: string, worktreeId?: string } → { workspaceId: string }
```
Validation: project/worktree (if given) exist.

### `close_workspace`
```ts
{ workspaceId: string } → { ok: true }
```
Validation: workspace exists; live sessions in it are closed and recorded to history.

### `save_layout`
Purpose: persist a workspace's split tree (Principle V — lossless).
```ts
{ workspaceId: string, layout: LayoutNode,
  paneBindings: Record<string, { providerId?: string, projectId?: string,
    worktreeId?: string, title?: string }> } → { ok: true }
```
Validation: `layout` conforms to `layout-node.schema.json`; `sizes.length === children.length`;
each binding key is a `paneId` in the layout; provider/project/worktree ids are known. Pane rows
are created or updated in the same persistence operation as the layout so provider assignments
round-trip losslessly on first save as well as later saves.

### `load_layout`
```ts
{ workspaceId: string } → { layout: LayoutNode,
  paneBindings: Record<string, { providerId?: string, projectId?: string,
    worktreeId?: string, title?: string }> }
```
Validation: workspace exists.

### `list_presets`
```ts
{} → { presets: Array<{ id: string, name: string }> }
```

### `save_preset`
```ts
{ name: string, layout: LayoutNode, paneProviders: Record<string, string> } → { presetId: string }
```
Validation: layout schema valid; provider ids known.

### `create_workspace_from_preset`
```ts
{ presetId: string, projectId?: string } → { workspaceId: string }
```
Validation: preset exists; project (if given) exists. Panes are created but sessions are only
started on user action.

---

## Projects

### `list_projects`
```ts
{ workspaceId?: string } → { projects: Array<ProjectSummary> }
// ProjectSummary
{ id: string, name: string, path: string, remote?: string, branch?: string,
  dirty: boolean, ahead: number, behind: number,
  activeSessions: number, color?: string, archived: boolean }
```
Validation: when `workspaceId` names a workspace with a pinned `rootPath`, discovery and the
returned list are both scoped to that folder; otherwise the configured project roots apply
(FR-033). Archived projects are returned with `archived: true` so the caller can show them in a
separate view (FR-034).

### `set_project_archived`
```ts
{ projectId: string, archived: boolean } → { ok: true }
```
Validation: the project exists. Archiving only sets `projects.archived_at`; the row, its history
and its worktrees are untouched, and rediscovery does not clear the flag (FR-034).

### `pick_directory`
```ts
{} → string | null
```
Validation: opens the OS folder chooser and returns the chosen absolute path, or `null` when
cancelled. Picking alone grants nothing — the path still has to pass `set_workspace_root` (or
`add_project_folder`) to take effect. The dialog runs Rust-side; the WebView is never granted the
dialog plugin's JS API (Principle I).

### `set_project_name`
```ts
{ projectId: string, name?: string } → { ok: true }
```
Validation: the project exists; a blank or omitted `name` clears the override. Stored in
`projects.display_name` so discovery's rewrite of `projects.name` cannot clobber it (FR-036).

### `rename_workspace`
```ts
{ workspaceId: string, title: string } → { ok: true }
```
Validation: the workspace exists; a blank title is rejected (`INVALID_NAME`). Persists to
`workspaces.title` (FR-037).

### `set_workspace_root`
```ts
{ workspaceId: string, path?: string } → { rootPath?: string }
```
Validation: `path` expands `~`, MUST canonicalize to an existing directory (`PATH_NOT_FOUND` /
`PATH_NOT_A_DIRECTORY` otherwise). Omitting `path` clears the pin. A pinned root joins the
allowed-root set used by the allowed-path check (FR-033, FR-025).

### `add_project_folder`
```ts
{ path: string } → { project: ProjectSummary }
```
Validation: `path` exists, is a directory, contains a `.git`; must be under an allowed root or
explicitly picked via the native folder dialog.

### `clone_project`
```ts
{ url: string, destRoot: string, name?: string } → { project: ProjectSummary }
```
Validation: `destRoot` is a configured root; `url` is a well-formed git URL; clone runs via
`git2` (no arbitrary shell).

### `remove_project`
```ts
{ projectId: string, deleteFiles?: false } → { ok: true }
```
Validation: `deleteFiles` is always `false` in v1 (removes from list only; never deletes files).

### `get_git_status`
```ts
{ projectId: string } → { branch: string, dirty: boolean, ahead: number, behind: number,
                          worktrees: Array<{ id: string, branch: string, path: string }> }
```

### `create_worktree`
```ts
{ projectId: string, branch: string, createBranch: boolean } → { worktree: { id, branch, path } }
```
Validation: branch not already checked out elsewhere; target dir created
under the project's worktree root.

### `list_worktrees`
```ts
{ projectId: string } → { worktrees: Array<{ id: string, branch: string, path: string }> }
```

### `remove_worktree`
```ts
{ worktreeId: string } → { ok: true }
```
Validation: no live session bound to it (or user confirms closing them); detaches cleanly via
`git2`, never touching the main working copy.

---

## Providers

### `list_providers`
```ts
{} → { providers: Array<{ id: string, label: string, kind: "builtin" | "custom",
        color?: string, detected: boolean, auth: "ok" | "expired" | "unknown" }> }
```

### `detect_provider`
```ts
{ providerId: string } → { detected: boolean, path?: string, version?: string,
                           auth: "ok" | "expired" | "unknown", message?: string }
```
Validation: provider known. `message` gives an actionable install hint when not detected.

### `upsert_provider_profile`
```ts
{ id: string, label: string, command: string, args: string[], color?: string,
  env?: Record<string,string> } → { ok: true }
```
Validation: `command` resolved against the login-shell PATH; stored in `config.toml` (never
secrets).

### `resolve_env`
Purpose: resolve & cache the login-shell environment (fixes Finder PATH).
```ts
{ force?: boolean } → { path: string, env: Record<string,string>, cachedAt: string }
```
Validation: runs the user's login shell in a controlled, non-interactive way; only allow-listed
vars are surfaced to the UI.

---

## Usage

### `get_usage`
Purpose: read the latest shared usage snapshot (no network — Principle IV).
```ts
{} → { providers: Record<string, UsageCard>, updatedAt: string, offline: boolean }
// UsageCard
{ label: string, lines: Array<{ label: string, value: string, pct?: number,
  resetsAt?: string }>, auth: "ok" | "expired", stale: boolean }
```
Validation: none. Returns the last snapshot even when offline.

### `refresh_usage`
Purpose: a user-initiated poll-now for the given provider (or all). Because it is an explicit
user action — not autonomous per-card polling — it is bounded by the ~60s cache window, not the
300s autonomous floor (Principle IV: "≥300s floor, ~60s cache"). The single background poller
still runs on the 300s floor. `scheduled` is `true` when a network fetch was actually issued;
`nextAllowedAt` is when the next user refresh of that provider will be honored (last attempt + 60s).
```ts
{ providerId?: string } → { scheduled: boolean, nextAllowedAt: string }
```
Validation: honors the ~60s cache window per provider; coalesces concurrent requests. The caller
should reflect the returned snapshot immediately (re-read via `get_usage`) so a throttled click
(`scheduled: false`) still shows the freshest cached values instead of appearing inert.

---

## Skills

### `list_skills`
```ts
{} → { skills: Array<{ id: string, name: string, version: string, providers: string[] }>,
       bindings: Array<SkillBinding> }
```

### `preview_skill_apply`
Purpose: show the diff before writing anything (Principle III).
```ts
{ skillId: string, providerId: string, scope: Scope } → { diff: string, willCreate: string[] }
```

### `apply_skill`
```ts
{ skillId: string, providerId: string, scope: Scope } → { created: string[] }
```
Validation: only writes app-managed regions; records every created path.

### `remove_skill`
```ts
{ skillId: string, providerId: string, scope: Scope } → { removed: string[] }
```
Validation: removes **only** previously app-created content; provider's own config untouched.

### `set_skill_binding`
```ts
{ skillId: string, scope: Scope, active: boolean } → { ok: true }
// Scope
{ level: "global" | "project" | "worktree" | "workspace" | "session", refId?: string }
```
Validation: precedence session > workspace > worktree > project > global.

---

## Memory

### `list_memory`
```ts
{ scope: Scope } → { entries: Array<MemoryEntry> }
```

### `search_memory`
```ts
{ query: string, scope?: Scope } → { entries: Array<MemoryEntry> }
```
Validation: FTS5 query; project-scoped results never cross projects (FR-024).

### `add_memory`
```ts
{ scope: Scope, type: MemoryType, title: string, body: string } → { entryId: string }
```

### `capture_selection_to_memory`
Purpose: save a selected terminal snippet (explicit, opt-in — FR-023).
```ts
{ sessionId: string, text: string, scope: Scope, type: MemoryType } → { entryId: string }
```
Validation: only the user-selected `text` is stored; NO automatic full-output capture exists.

### `preview_memory_context`
Purpose: show what would be injected before composing agent context.
```ts
{ scope: Scope } → { composed: string, sources: Array<{ entryId: string, scope: Scope }> }
```

---

## Settings

### `get_settings`
```ts
{} → { settings: AppSettings }
```

### `set_settings`
```ts
{ patch: Partial<AppSettings> } → { settings: AppSettings }
```
Validation: `projectRoots` entries must be real directories; scrollback cap within bounds;
never accepts secret material.

When `patch.invisibleMode` differs from the stored value the command applies the OS-level mode
**before** persisting, and persists only if applying succeeded. The returned `settings.invisibleMode`
is therefore the state actually in force, never the state that was requested — a caller that sends
`true` and reads `false` back has been told it did not work. Error code
`INVISIBLE_MODE_APPLY_FAILED` when applying was rolled back. Added by feature 003.

### `notify`
```ts
{ title: string, body: string } → { ok: true, delivered: boolean }
```
`delivered` is `false` when the invisible mode is active and the notification was suppressed;
suppressed notifications are dropped, not queued. Title and body are sanitized before display.

### `AppSettings`
```ts
interface AppSettings {
  projectRoots: string[];
  keybindings: Record<string, string>;
  scrollbackLines: number;
  memoryAutoCapture: boolean;
  usageRefreshSeconds: number;
  invisibleMode: boolean;   // feature 003 — app-wide, default false
}
```

---

### Shared enums
```ts
type SessionState = "starting" | "running" | "exited" | "error";
type MemoryType = "fact" | "decision" | "constraint" | "preference" | "glossary"
                | "known_issue" | "command" | "todo";
```
