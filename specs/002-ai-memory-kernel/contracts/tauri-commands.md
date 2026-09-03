# Contract: Tauri Commands — Feature 002 delta

This is a **delta** against the closed catalog in
[`../../001-ai-terminal-workspace/contracts/tauri-commands.md`](../../001-ai-terminal-workspace/contracts/tauri-commands.md).
The rules there still hold in full: there is no generic command-execution primitive (Principle I),
every `path`/`cwd` canonicalizes under a configured project root, every result is
`Result<T, AppError>`, and anything not in the catalog is not callable from the frontend.

**Added to the global validation rules for this feature**: the WebView never receives the kernel's
URL, port, data directory path or token, and never composes an argument that reaches a sub-process.
Any `Scope` is validated before it is mapped to a kernel scope; a memory read or write with no
resolved project is not representable (see [memory-kernel.md](./memory-kernel.md)).

---

## Memory (replaces the Memory section of the 001 catalog)

The five existing commands keep their names and request shapes so the frontend contract does not
churn. `MemoryEntry.id` is now the kernel page path — still a `string`, so existing keying works.

### `list_memory`
```ts
{ scope: Scope } → { entries: Array<MemoryEntry> }
```

### `search_memory`
```ts
{ query: string, scope: Scope } → { entries: Array<MemoryEntry> }
```
Validation: `scope` is now **required**. An unscoped kernel query returns pages from every project
(verified), so the app must never issue one (FR-046).

### `add_memory`
```ts
{ scope: Scope, type: MemoryType, title: string, body: string } → { entryId: string }
```

### `capture_selection_to_memory`
```ts
{ sessionId: string, text: string, scope: Scope, type: MemoryType } → { entryId: string }
```
Validation: unchanged from 001 — only the user-selected `text` is stored, the scope must belong to
the session, and no automatic full-output capture exists (FR-023 of 001, still binding).

### `preview_memory_context`
```ts
{ scope: Scope } → { composed: string, sources: Array<{ entryId: string, scope: Scope }> }
```

### `update_memory` *(new — closes a 001 gap)*
```ts
{ scope: Scope, path: string, title?: string, body: string } → { entryId: string }
```

### `delete_memory` *(new — closes a 001 gap)*
```ts
{ scope: Scope, path: string } → { removed: boolean }
```

### `read_memory_page` *(new)*
Purpose: open a search result. In 001 the list rows had no handler at all.
```ts
{ scope: Scope, path: string }
  → { path: string, title: string, type: MemoryType, body: string,
      author: "terminal-ai" | "agent", updatedAt: string }
```
Validation: `path` is rejected if it contains `..`, a leading `/`, NUL, characters outside
`[A-Za-z0-9._/-]`, or exceeds 200 chars — the page route is a wildcard.

### `MemoryEntry` *(shape, referenced but never defined in the 001 catalog)*

> The wire type is deliberately still called `MemoryEntry` while the domain type in
> [memory-kernel.md](./memory-kernel.md) is `MemoryPage`. That is not drift: keeping the wire name
> means `src/lib/ipc.ts` and every component already typed against `MemoryEntry` needs no churn.

```ts
{ id: string, scope: Scope, type: MemoryType, title: string, body: string,
  author: "terminal-ai" | "agent", createdAt: string, updatedAt: string }
```

---

## Memory kernel

### `get_memory_kernel_status`
```ts
{} → { state: "notInstalled"|"probing"|"starting"|"ready"|"attached"|"degraded"
              |"portConflict"|"failed",
       owned: boolean, serverUrl: string, dataDir?: string,
       version?: string, versionMatchesPin: boolean, hasToken: boolean,
       pages?: number, pendingMigration: number, hybridSearch: boolean,
       lastCheckedAt: string, lastError?: { code: string, message: string },
       guidance?: string }
```
Validation: none. **MUST read the supervisor's cached snapshot and MUST NOT perform its own network
call** — that is what makes SC-020 true no matter how many views are mounted (Principle IV).

### `start_memory_kernel`
```ts
{} → { status: KernelStatus }
```

### `stop_memory_kernel`
```ts
{} → { status: KernelStatus }
```
Validation: **refuses with `MEMORY_KERNEL_NOT_OWNED` when `owned === false`.** The app never stops a
server it did not start (FR-039, Principle VII).

### `restart_memory_kernel`
```ts
{} → { status: KernelStatus }
```
Validation: same ownership refusal; rate-limited to one call per 5s.

### `set_memory_kernel_settings`
```ts
{ serverUrl?: string, binaryPath?: string, autoStart?: boolean, hybridSearch?: boolean }
  → { status: KernelStatus }
```
Validation: `serverUrl` MUST parse and its host MUST be loopback (`127.0.0.1`, `localhost`, `::1`) —
anything else is refused (FR-063). `binaryPath` MUST canonicalize to an existing regular file.
Setting `hybridSearch: true` is what authorises the ~87 MB model fetch, and the frontend MUST have
disclosed the size first (FR-062). **No token is accepted here.**

### `set_memory_kernel_token`
```ts
{ token: string | null } → { ok: true }
```
Writes or clears the Keychain item. The value is never echoed back by any command; status reports
`hasToken: boolean` only (FR-061).

---

## Memory wiring

### `preview_memory_wiring`
```ts
{ agent: "claude-code" | "codex" | "opencode", scope: Scope, kinds: Array<"mcp"|"hooks"> }
  → { plans: Array<{ agent: string, kind: "mcp"|"hooks", path: string,
                     diff: string, willCreate: string[], willModify: string[],
                     conflict?: "unmanaged", captureEvents?: string[],
                     warnings: string[] }> }
```
Validation: runs the kernel's dry-run and **never writes** (verified: dry-run leaves target files
byte-identical). `captureEvents` lists exactly which lifecycle events would be captured, so the
consent is informed rather than nominal (FR-058).

### `apply_memory_wiring`
```ts
{ agent, scope, kinds: Array<"mcp"|"hooks"> } → { created: string[], modified: string[] }
```
Validation: `kinds` containing `"hooks"` requires `app_settings.memory_auto_capture === true`, else
`MEMORY_CAPTURE_NOT_CONSENTED`. An entry the app did not create is never overwritten — it is
reported as `unmanaged` and left alone (FR-056).

### `remove_memory_wiring`
```ts
{ agent, scope } → { removed: string[], restored: string[] }
```
Validation: removes only recorded artifacts. Refuses with `MEMORY_WIRING_DRIFTED`, returning the
diff and the backup path, when a merged configuration file no longer hashes to what the app left
(FR-057).

### `list_memory_wiring`
```ts
{} → { bindings: Array<{ id: string, agent: string, kind: "mcp"|"hooks", scope: Scope,
                         status: "applied"|"stale"|"drifted"|"unmanaged",
                         path: string, appliedAt: string }> }
```

---

## Handoffs

### `list_memory_handoffs`
```ts
{ scope: Scope, state?: "open" | "all" } → { handoffs: Array<Handoff> }
```

### `get_memory_briefing`
```ts
{ scope: Scope } → { briefing: string, generatedAt?: string }
```
Purpose: a short digest of what is in this project's memory — recent pages and counts — so the panel
can show activity without the user searching for it.

### `expire_memory_handoffs`
```ts
{ scope: Scope, olderThanDays: number } → { expired: number }
```
Validation: `olderThanDays` ≥ 1. Clears handoffs that have gone stale.

> There is deliberately **no** `begin_memory_handoff` and **no** `accept_memory_handoff`.
> Creating a handoff is an agent action at the end of its own session, and *accepting* one is an
> agent action at the start of the next — a handoff is consumed on acceptance, so an app that
> accepted it would take the context away from the agent that was about to receive it. The app
> shows that continuity is pending and can clear it when stale; it does not stand in the middle.

---

## Project identity

### `check_memory_project_identity`
Purpose: detect that a project's directory was renamed or moved since memory was written for it.
```ts
{ projectId: string }
  → { stale: boolean, currentProject: string,
      previousProject?: string, previousPath?: string }
```
Validation: `projectId` must exist. The first call for a project records what it resolved to, so a
later call has something to compare against; subsequent calls do not overwrite that record.

> This command exists because of the naming decision in [research.md §6](../research.md): a project
> is named by its directory basename so that agents deriving it from their working directory agree
> with the panel. The price is that renaming the directory re-points the project, and the old memory
> silently stops appearing. Without this, that reads as data loss (FR-064, SC-021).

---

## Migration

### `run_memory_migration`
```ts
{ dryRun: boolean }
  → { total: number, alreadyImported: number, imported: number,
      skipped: Array<{ entryId: string, reason: string }>,
      failed: Array<{ entryId: string, error: string }>,
      completedAt?: string }
```
Validation: never runs implicitly at startup (FR-051). `dryRun: true` writes nothing.

### `undo_memory_migration`
```ts
{ confirm: true } → { deleted: string[] }
```
Validation: deletes only pages listed in `memory_migration_log`. Legacy data on disk is untouched
(FR-054).

---

## Events

Registered the same way `usage-updated` is.

| Event | Payload | When |
| --- | --- | --- |
| `memory-kernel-status` | `KernelStatus` | On state change only — never on every poll tick. |
| `memory-updated` | `{ scope: Scope, reason: "write"\|"delete"\|"migration" }` | After a successful mutation, so panels refresh without polling. |

---

## Shared enums (additions)

```ts
type KernelState = "notInstalled" | "probing" | "starting" | "ready"
                 | "attached" | "degraded" | "portConflict" | "failed";
type WiringKind   = "mcp" | "hooks";
type WiringStatus = "applied" | "stale" | "drifted" | "unmanaged";
type HandoffState = "open" | "accepted" | "expired";
type PageAuthor   = "terminal-ai" | "agent";
```

## Removed from the 001 catalog

Nothing. All five memory commands survive with their names and request shapes; only their
implementation moves behind `MemoryKernel`.
