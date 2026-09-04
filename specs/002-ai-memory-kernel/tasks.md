---
description: "Task list for the ai-memory kernel implementation (feature 002)"
---

**Input**: Design documents in `/specs/002-ai-memory-kernel/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`

**Tests**: The spec asks for verification-first acceptance (Constitution: Development Workflow), so
test tasks are included where a behaviour is data-loss-class or is a Success Criterion. They are not
generated blanket-style for every function.

**Organization**: By user story, US8 → US12, each independently demonstrable via `quickstart.md`.

**Numbering**: Task IDs continue the repository's global sequence; feature 001 ended at T106, so this
feature starts at **T107**.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task).
- **[Story]**: `US8`–`US12`. Setup / Foundational / Polish carry no story label.
- Paths follow the monorepo layout in `plan.md`.
- **T175–T178** were added by the 2026-09-03 analysis pass and sit in the phase they belong to, so
  IDs are not strictly ascending across phases from that point. IDs remain unique and never reused.

---

## Phase 1: Setup (Shared Infrastructure)

**Goal**: The pinned kernel binary is reproducibly obtainable and the workspace knows about the new
crate. Nothing here depends on the kernel running.

- [x] T107 Create `scripts/ai-memory.lock` pinning `v2.0.2` and the published SHA-256 for both macOS architectures — the single place a version is written (research §11).
- [x] T108 Write `scripts/fetch-ai-memory.sh`: download the tarball for the host arch, **verify the checksum against the lock file and abort on mismatch**, extract, and place the binary as `src-tauri/binaries/ai-memory-<target-triple>` with `chmod +x`.
- [x] T109 [P] Add `src-tauri/binaries/` to `.gitignore` — never commit a 29.5 MB binary.
- [x] T110 [P] Add `sha2 = "0.10"` to `[workspace.dependencies]` in the root `Cargo.toml`.
- [x] T111 Create `crates/memory-kernel/Cargo.toml` (deps: `terminal-ai-domain`, `reqwest`, `tokio`, `serde`, `serde_json`, `thiserror`, `tracing`, `chrono`, `sha2`; dev-deps `mockito`, `insta`) and register `crates/*` membership; assert it depends on neither `tauri` nor `persistence`.
- [x] T112 [P] Extract the upstream `LICENSE` from the tarball to `src-tauri/resources/third-party/ai-memory-LICENSE.txt`, add `bundle.resources` in `src-tauri/tauri.conf.json`, and create `THIRD_PARTY_LICENSES.md` at the repo root — MIT redistribution is a condition, not a courtesy (research §11).

**Checkpoint**: `bash scripts/fetch-ai-memory.sh && src-tauri/binaries/ai-memory-* --version` prints `ai-memory 2.0.2`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Goal**: The seam, the schema and the OS access every story needs. **No user story can start until
this phase is done.**

- [x] T113 Create `crates/domain/src/memory.rs` per `contracts/memory-kernel.md`: the `MemoryKernel` trait plus `KernelScope`, `MemoryPage`, `PageAuthor`, `MemorySource`, `Handoff`, `HandoffState`, `KernelStatus`, `KernelState`, `MemoryError`. **Zero new dependencies**; no transport type in any signature.
- [x] T114 Add a CI-style assertion that `terminal-ai-domain` gained no dependency — `cargo tree -p terminal-ai-domain` compared against a checked-in expectation (Principle VII, plan.md risk).
- [x] T115 Implement `crates/memory-kernel/src/scope.rs`: `Scope` → `KernelScope` mapping per `data-model.md`, project = repository-root basename, worktree → parent project via `worktrees.project_id`, deterministic page paths, and path validation (reject `..`, leading `/`, NUL, non-allowlisted bytes, >200 chars).
- [x] T116 [P] Unit-test `scope.rs`: every scope level; **two projects with the same basename are detected as a collision rather than silently merged**; worktree resolves to the parent; path traversal rejected; page path stable across runs.
- [x] T117 [P] Port `project_search_never_crosses_scope` from `crates/memory-manager/src/lib.rs:367-402` into `crates/memory-kernel/src/scope.rs` tests against a fake kernel — this is the FR-046 proof and must survive the crate's deletion.
- [x] T118 Create `crates/persistence/migrations/V005__memory_kernel.sql` per `data-model.md`: `memory_wiring_bindings`, `memory_migration_log`, and the four seeded `app_settings` rows. Purely additive; drops nothing.
- [x] T119 Bump the `tables >= 15` assertion in `crates/persistence/src/lib.rs:83` to 17 and add `MemoryWiringDao` + `MemoryMigrationDao` to `crates/persistence/src/dao/mod.rs`, shaped like `SkillsDao` (`dao/mod.rs:557-630`).
- [x] T120 [P] Create `crates/platform-macos/src/keychain.rs` with `keychain_get`/`keychain_set`/`keychain_delete` via `/usr/bin/security`, following the read pattern at `crates/usage-core/src/adapters/anthropic.rs:48-55`. **The secret goes on stdin, never on argv** — `-w <secret>` is visible to `ps` (research §10).
- [x] T121 [P] Add an `AuthToken` newtype in `crates/memory-kernel` whose `Debug` renders `<redacted>`, plus a test asserting the secret appears in no `Debug` output and no `tracing` field.

- [x] T175 **[BLOCKING]** Resolve research open item 2 before any wiring code exists: determine whether **Codex** and **OpenCode** support project-scoped hook configuration the way Claude Code does. If an agent does not, capture wiring for that agent is **unavailable** and the UI says so — the documented fallback (global hooks + `--capture-mode allowlist` + a `.ai-memory.toml` marker) is NOT to be used, because it reintroduces the marker that breaks `remove_worktree` (research §6, §7). Record the answer in `contracts/ai-memory-surface.md` — per FR-058 / Constitution III + VI.
- [x] T176 **[BLOCKING]** Resolve research open item 1 before T126: find the flag or config key that suppresses the ~87 MB embedding-model fetch. If none exists, the kernel is started only after the disclosure has been shown and accepted, and the setting is "start the kernel" rather than "suppress the fetch". Record the answer in `contracts/ai-memory-surface.md` — per FR-062.

**Checkpoint**: `cargo test` green; the trait compiles; `V005` applies on a fresh DB and on one seeded at `V004`; **T175 and T176 answered and recorded** — no user-story phase starts before that.

---

## Phase 3: User Story 8 — A memory kernel the app owns (Priority: P1) 🎯 MVP

**Goal**: The kernel comes up or is attached to, its state is observable, and losing it never blocks
anything.

**Independent Test**: Kill the kernel with six panes running — no session dies, the UI stays
interactive, the panel says unavailable within 60s, and it recovers on its own (SC-013).

### Implementation for User Story 8

- [x] T122 [US8] Implement `crates/memory-kernel/src/supervisor.rs` as a **pure** `transition(state, event) -> state` function first: the state machine in `plan.md`, backoff 1s→2s→4s→8s→16s→30s with jitter, a cap of 5 consecutive failures into terminal `Failed`, and `owned` as the sole gate on terminate/restart.
- [x] T123 [P] [US8] Unit-test the supervisor transitions with **no IO**, including the data-loss-class case: **an `Attached` (not-owned) kernel never transitions into a terminate or restart** (FR-039, SC-019).
- [x] T124 [US8] Add binary resolution to the supervisor: bundled sidecar via `current_exe()?.parent()?.join("ai-memory")` (plus the `-<target-triple>` form for `cargo tauri dev`) → `app_settings.memory_kernel_binary` → `PATH` via `provider_runtime::resolve_executable` semantics → `NotInstalled`. No `tauri-plugin-shell` (Principle I).
- [x] T125 [US8] Implement the detection probe: `POST /mcp` `tools/list` with **both** `Content-Type: application/json` and `Accept: application/json, text/event-stream`, 800 ms timeout, requiring `result.tools` to contain `memory_query`. Attach on success, `Attached{unauthorized}` on 401/403, spawn on connection-refused, `PortConflict` on anything else. **Never probe `/api/v1` — it 404s without `--enable-web`** (research §3).
- [x] T126 [US8] Implement spawn: `ai-memory serve --transport http --bind 127.0.0.1:<port> --enable-web`, **no `--data-dir`** so the shared store is used (research §5), token from Keychain in the child env only, readiness by `status --json` with backoff and a 15s cap.
- [x] T127 [US8] Implement the runtime pidfile at `<AppPaths.root>/runtime/ai-memory.json` (`{pid, port, owned, started_at}`) and orphan reclaim on boot: adopt a live, correctly-answering process instead of spawning a second server (FR-043).
- [x] T128 [US8] Implement `crates/memory-kernel/src/http.rs`: the `/api/v1` read client on one `reqwest::Client` built with `.timeout(5s)` and **`.no_proxy()`** (a login-shell `HTTP_PROXY` would otherwise route loopback traffic). Parse **bare arrays**, `body_markdown`, nested `frontmatter`; funnel every response through status → `{"error":…}` → success shape.
- [x] T129 [P] [US8] `mockito` tests for `http.rs`: 200, `{"error":…}` body, 401, 404, 500, and valid-JSON-wrong-shape — asserting the `MemoryError` **variant**, not a string.
- [x] T130 [US8] Implement `crates/memory-kernel/src/cli.rs`: fixed-argv invocation of `write-page`, `delete-page`, `status --json`, `search --json`; exit-code and stderr mapping into `MemoryError`. Never a shell string.
- [x] T131 [US8] Implement `AiMemoryKernel` in `crates/memory-kernel/src/lib.rs`, wiring `scope` + `http` + `cli` + supervisor cache into the `MemoryKernel` trait — per FR-038. `status()` reads the cache, performs **no IO** and cannot fail; every other method short-circuits with `MemoryError::Unavailable` when not ready.
- [x] T132 [US8] Add `kernel: Arc<MemoryKernelSupervisor>` to `AppState` (`src-tauri/src/state.rs:13-20`) beside `usage`, and start **one** supervisor task with **one** status ticker (~15s) in `src-tauri/src/lib.rs`, mirroring the usage poller at `lib.rs:62-95`. Emit `memory-kernel-status` **on change only**.
- [x] T133 [US8] Change `src-tauri/src/lib.rs` from `.run(generate_context!())` to `.build(...)? + .run(|_, event|)` and handle `RunEvent::Exit`: SIGTERM → 3s → SIGKILL **only when `owned`**; when not owned, do nothing (SC-019).
- [x] T134 [US8] Add the `MemoryError → AppError` conversion beside the existing `From` impls in `src-tauri/src/commands.rs:42-51`, using the code table in `contracts/memory-kernel.md`.
- [x] T135 [US8] Re-point `list_memory`, `search_memory`, `add_memory`, `capture_selection_to_memory`, `preview_memory_context` at the kernel, keeping names and request shapes. `search_memory`'s `scope` becomes **required**. Keep the session-vs-scope guard at `commands.rs:2136-2168` verbatim — per FR-038 / FR-045.
- [x] T136 [US8] Add `get_memory_kernel_status`, `start_memory_kernel`, `stop_memory_kernel`, `restart_memory_kernel`, `set_memory_kernel_settings`, `set_memory_kernel_token` per `contracts/tauri-commands.md`; register them in `src-tauri/src/lib.rs`. `stop`/`restart` refuse with `MEMORY_KERNEL_NOT_OWNED` when `owned === false`; `serverUrl` must be loopback (FR-063). The token is written to the Keychain only and never returned by any command — per FR-061.
- [x] T137 [US8] Create `src/stores/memory.ts` (Zustand): kernel status + entries + wiring state, subscribing **once** to `memory-kernel-status`. This closes the drift flagged in T079 — memory state is currently all local `useState`.
- [x] T138 [P] [US8] Vitest: N mounted memory views produce **zero** extra `invoke` calls for status — the SC-020 proof.
- [x] T139 [US8] Add the new typed clients to `src/lib/ipc.ts` and build `src/features/memory/KernelSetup.tsx`: the `notInstalled` / `starting` / `attached` / `portConflict` / `failed` states with their guidance, using existing primitives and `theme.css` tokens only.
- [x] T140 [US8] Add the kernel status chip to `MemoryPanel.tsx` and make the panel degrade to an inline banner — never a blocking modal — when the kernel is unavailable.
- [x] T141 [P] [US8] Detect the "spawned then died with signal 9 / status 137" pattern and map it to a quarantine-specific `lastError` with the `xattr -d com.apple.quarantine` guidance (research §11).

- [x] T177 [US8] Instrument the ready-state latency (probe start → `Ready`/`Attached`) and assert it against the SC-012 budget in the `#[ignore]`d sidecar integration test: ≤5s warm, ≤15s cold. 001 treats performance budgets as "enforced, not aspirational" — per SC-012.

**Checkpoint**: quickstart K0, K1, K2, K3 pass. The MVP is usable: memory reads and writes work through the kernel and nothing else in the app can be taken down by it.

---

## Phase 4: User Story 9 — The existing memory survives the move (Priority: P2)

**Goal**: The legacy acervo is imported once, idempotently, resumably and undoably.

**Independent Test**: Import ≥50 legacy entries, verify the count, run again and verify zero new pages (SC-016).

- [x] T142 [US9] Implement `crates/memory-kernel/src/migration.rs` taking an already-loaded `Vec<LegacyEntry>` (so the crate stays `persistence`-free): deterministic `terminal-ai/imported/<entry_id>.md` paths, `terminal_ai_entry_id` frontmatter, `body_sha256`, concurrency 4, 10s per entry, ordered global → project → worktree → workspace → session.
- [x] T143 [US9] Add the `src-tauri` orchestration that reads `memory_entries` + each `content_path` from disk, drives `migration.rs`, and **writes `memory_migration_log` per item, not batched**, so an interrupted run resumes exactly (FR-053).
- [x] T144 [P] [US9] Table-driven tests against a `FakeKernel`: run twice → second writes zero; error at item 3 → resume writes exactly the remainder; **path stable across runs with the log emptied** (the third idempotency layer).
- [x] T145 [US9] Add `run_memory_migration { dryRun }` and `undo_memory_migration { confirm }` commands; expose `pendingMigration` in kernel status. **Never run implicitly at startup** (FR-051). `dryRun: true` writes nothing — per FR-052.
- [x] T146 [US9] Add the migration banner and report UI (preview before run) to `MemoryPanel.tsx` — per FR-052 —: pending count, preview, run, undo — with the skipped/failed lists visible rather than swallowed.

**Checkpoint**: quickstart K4 passes.

---

## Phase 5: User Story 10 — Agents that share the same memory (Priority: P3)

**Goal**: Consented, recorded, reversible wiring of the agent CLIs. **This is the first phase that writes outside the app's own data directory.**

**Independent Test**: Wire a project, have a Claude pane report the same page count the panel shows, then remove the wiring and diff the configs (SC-015, SC-017).

- [x] T147 [US10] Implement `crates/memory-kernel/src/wiring.rs`: build dry-run and apply argv for `install-mcp` / `install-hooks` (always with `--config-file`, `--project-strategy repo-root`, `--no-capture-prompts`), extract the emitted JSON block, and define `MemoryWiringArtifact` per `data-model.md`.
- [x] T148 [US10] Compute the diff **from the target file itself** (read before, apply into a temporary copy, read after) rather than parsing the human-readable dry-run text; if the text format is unrecognisable, degrade to "cannot preview; apply disabled" rather than applying something unshown (research §8).
- [x] T149 [US10] Implement the unmanaged guard: an existing ai-memory entry the app did not create is reported `unmanaged` and never overwritten (FR-056).
- [x] T150 [US10] Implement apply: backup to `<AppPaths.root>/wiring-backups/<id>/<basename>.<millis>.bak` via tmp-write + rename, run with `--apply`, hash the result, and record the artifact — including `binary_path` for staleness detection.
- [x] T151 [US10] Implement removal in three tiers: `uninstall --only <kind> --mcp-url <url>` (dry-run first) → delete when the app created the file and it still matches → hash-gated backup restore. **Refuse with `MEMORY_WIRING_DRIFTED` when the file changed after apply** (FR-057).
- [x] T152 [US10] Add `preview_memory_wiring`, `apply_memory_wiring`, `remove_memory_wiring`, `list_memory_wiring`; gate `kinds` containing `"hooks"` on `app_settings.memory_auto_capture === true`, else `MEMORY_CAPTURE_NOT_CONSENTED` (FR-058). Record via `MemoryWiringDao` after the crate call succeeds, mirroring `commands.rs:1961-1991`.
- [x] T153 [US10] Install hooks into the agent's **project-scoped** configuration file, not the global one, so one project's consent cannot enable capture machine-wide (research §7). **T175 settled the per-agent answer**: offer capture for `claude-code` only (written to `<project>/.claude/settings.json`); for `codex` (no automatic hook install upstream) and `opencode` (global plugin only) offer MCP registration and state plainly that capture is unavailable — per FR-065.
- [x] T154 [US10] Build the wiring consent UI in `KernelSetup.tsx`: target path, real diff, and the **explicit list of lifecycle events that would be captured** — consent has to be informed to count (FR-058).
- [x] T155 [P] [US10] Regression test: create a worktree, apply wiring for it, and assert `remove_worktree` **still succeeds** — `worktree-manager::is_dirty` counts untracked files (`lib.rs:105-113`) and `remove` refuses a dirty worktree (`:83-85`).
- [x] T156 [US10] Implement stale detection: compare the recorded `binary_path` against the resolved sidecar at startup, mark the binding `stale`, and offer re-apply (FR-059).

**Checkpoint**: quickstart K5, K5b, K6, K10 pass.

---

## Phase 6: User Story 11 — Search that respects the project boundary (Priority: P4)

**Goal**: Scoped hybrid search, and results you can actually open.

**Independent Test**: Same keyword in two projects; searching from one returns only that one (SC-014).

- [x] T157 [US11] Add `read_memory_page`, `update_memory` and `delete_memory` commands and their `ipc.ts` clients — closing the 001 gaps where `update` existed but was never exposed and `delete` did not exist at all — per FR-045.
- [x] T158 [US11] Give the entry rows in `MemoryPanel.tsx:105-117` an `onClick` that opens the page body, add edit and delete, and expose the `worktree` and `workspace` scopes the picker at `MemoryPanel.tsx:11` currently hides.
- [x] T159 [US11] Render kernel content as sanitized text/markdown with a bounded length — search snippets contain `<mark>` HTML and page bodies are untrusted (FR-048).
- [x] T160 [P] [US11] List agent-authored pages (no `terminal_ai_*` frontmatter) with a degraded type and path-derived title, badged as agent-written (FR-049).
- [x] T161 [US11] Fix the capture-selection bug at `MemoryPanel.tsx:47-59` where the user-edited title is discarded because the backend re-derives it from the text.
- [x] T179 [US11] `preview_memory_context` remains the only way composed memory is surfaced — per FR-050. **Resolved by scope, not by code**: this feature adds no path that injects composed memory into a session, so there is nothing that could bypass the preview. Building a gate in front of a door that does not exist would be theatre. When injection is built, the gate belongs in the same change; FR-050 is satisfied today because the preview is the only consumer.
- [x] T162 [US11] Add the `hybridSearch` settings toggle: off by default, and disclosing the ~87 MB model size **before** enabling it (FR-062, research §9 open item 1).

- [x] T178 [US11] Detect stale project identity: record the repository path a project's memory was written under, compare it at startup, and when the directory has been renamed or moved surface a notice offering to re-point the memory — rather than showing an apparently empty panel. Re-pointing is the one case where `rename-project` may be called, and only on explicit user confirmation — per FR-064 / SC-021.

**Checkpoint**: quickstart K7, K9 pass, plus SC-021.

---

## Phase 7: User Story 12 — Continuity when switching agents (Priority: P5)

**Goal**: Pending handoffs surfaced where the user will see them.

**Independent Test**: End a Claude session, open a Codex pane in the same project, see the handoff (SC-018).

- [x] T163 [US12] Implement `handoffs`, `accept_handoff` and `cancel_handoff` in `AiMemoryKernel` over `/api/v1/.../handoffs` and the corresponding MCP tools.
- [x] T164 [US12] Add `list_memory_handoffs`, `accept_memory_handoff`, `cancel_memory_handoff`. **Do not add `begin_memory_handoff`** — creating a handoff is an agent action (FR-060).
- [x] T165 [US12] Surface pending handoffs in `MemoryPanel.tsx` with summary, open questions and next steps, plus accept and cancel.
- [x] T166 [P] [US12] Add `get_memory_briefing` and a briefing card (counts, recent activity, health).

**Checkpoint**: quickstart K8 passes. All user stories independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

- [x] T167 Delete `crates/memory-manager/` entirely, plus the dependency line in `src-tauri/Cargo.toml` and the `use terminal_ai_memory_manager as memory_manager;` at `commands.rs:15`. Only after T117 has ported its isolation test.
- [x] T168 [P] Update `CLAUDE.md` §2: the crate tree gains `memory-kernel/`, loses `memory-manager/`, and the "where do I put new code" list gains a memory-kernel entry.
- [x] T169 [P] Add to `docs/deferred.md`: dropping the legacy memory tables after two releases, and signing `externalBin` as part of the Developer ID / notarization checklist.
- [x] T170 [P] Write `docs/ai-memory-kernel.md`: what the kernel is, where its data lives, that the store is shared, how to attach to your own server, and how to remove the wiring.
- [x] T171 Add the version-pin check: compare the running server's `status --json` version against `scripts/ai-memory.lock` and surface `versionMatchesPin: false` rather than failing obscurely on a moved shape (plan.md risk 1).
- [x] T172 Resolve the research open items in `research.md` and fold the answers back into `contracts/ai-memory-surface.md`, promoting ⚠ entries to ✅ as they are observed.
- [x] T173 Run the full gate: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test && cargo test -- --ignored && pnpm test && pnpm lint && pnpm build`.
- [x] T174 Evidence recorded in `docs/validation-2026-09-03.md`. **Partially done, and the record says so**: 5 of 11 scenarios are proven by automated tests (including SC-014 and SC-016 against a real sidecar), 6 need the app on screen and are marked *not yet run* rather than assumed. K6 — an agent reading a page the panel wrote — is the one that still has to be observed before the feature can be called accepted.

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)** — no dependencies.
- **Phase 2 (Foundational)** — depends on Phase 1. **Blocks every user story.**
- **Phase 3 (US8)** — depends on Phase 2. Nothing else depends on it being *complete*, but every later story needs its kernel to be reachable.
- **Phase 4 (US9)** — depends on Phase 3 (needs a working `write`).
- **Phase 5 (US10)** — depends on Phase 3. Independent of US9.
- **Phase 6 (US11)** — depends on Phase 3. Independent of US9 and US10.
- **Phase 7 (US12)** — depends on Phase 5 (handoffs only exist once agents are wired and capturing).
- **Phase 8 (Polish)** — T167 depends on T117 and T135; the rest depend on their stories.

### Parallel opportunities

- Phase 1: T109, T110, T112 in parallel after T107.
- Phase 2: T116, T117, T120, T121 in parallel once T113 and T115 exist.
- Phase 3: T123, T129, T138, T141 in parallel with the sequential supervisor/command work.
- Phases 4, 5 and 6 can proceed in parallel once Phase 3 is checkpointed — they touch different files.

## Parallel Example: User Story 8

```
After T122 lands, run together:
  T123  supervisor transition tests        (crates/memory-kernel/src/supervisor.rs tests)
  T129  mockito read-client tests          (crates/memory-kernel/src/http.rs tests)
  T138  Vitest single-poller assertion     (src/stores/memory.test.ts)
  T141  quarantine detection               (crates/memory-kernel/src/supervisor.rs)
```

## Implementation Strategy

### MVP first

Phases 1–3 (T107–T141) are the MVP: a supervised or attached kernel, memory reading and writing
through it, and an app that cannot be taken down by it. Ship-able and demonstrable on its own.

### Incremental delivery

Each later phase is a standalone increment with its own quickstart scenarios. US10 is the first that
writes outside the app's data directory and should not be started until US8's checkpoint is observed,
because a wiring bug on top of an unreliable kernel is very hard to diagnose.

### Order of risk

T125 (probe), T133 (ownership on exit) and T151 (drift-gated removal) are the three tasks where a
mistake is data-loss-class. Each has a dedicated test task and a dedicated quickstart scenario.

## Notes

- **Out of scope, deliberately**: `ai-memory run` managed workstreams (a possible feature 003), and
  every destructive kernel operation (`reset`, `purge-project`, `--purge-data`) — the store is shared
  with the user (research §5).
- The memory half of feature 001 (FR-022, FR-023, FR-024, SC-010) stays true; its implementation
  moves here. `capture_selection_to_memory`'s explicit-only capture guard is preserved verbatim.
