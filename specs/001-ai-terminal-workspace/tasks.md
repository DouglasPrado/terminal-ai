---
description: "Task list for AI Terminal Workspace implementation"
---

# Tasks: AI Terminal Workspace

**Input**: Design documents from `/specs/001-ai-terminal-workspace/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, constitution.md

**Tests**: Included selectively (usage adapters, layout-tree reducer, PTY smoke) per the
constitution's "verification-first for runtime behavior" gate — not full TDD.

**Organization**: Grouped by user story (US1–US7 in priority order) so each is an independently
testable increment. Core MVP = Setup + Foundational + US1 (+US2/US3 to reach the daily-driver).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task).
- **[Story]**: US1–US7; Setup/Foundational/Polish carry no story label.
- Paths follow the monorepo in plan.md (`src/` frontend, `src-tauri/`, `crates/*`).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Toolchain, scaffold, workspace, design tokens.

- [X] T001 Install Rust via `rustup` and confirm `cargo`/`rustc`; ensure Xcode CLT (`xcode-select --install`).
- [X] T002 Scaffold the app with `create-tauri-app` (React + TypeScript + Vite) into `/Users/douglasprado/www/terminal-ai`.
- [X] T003 Define the Cargo workspace in `Cargo.toml` with member crates `crates/*` and `src-tauri`; create empty `crates/{domain,pty-runtime,provider-runtime,usage-core,project-manager,worktree-manager,skill-manager,memory-manager,persistence,platform-macos}` with `Cargo.toml` + `src/lib.rs`.
- [X] T004 Configure the pnpm workspace (`pnpm-workspace.yaml`, `package.json`) and install frontend deps: `@xterm/xterm` + addons (`addon-fit`, `addon-search`, `addon-web-links`, `addon-unicode11`, `addon-webgl`), `react-resizable-panels`, `@dnd-kit/core`, `zustand`.
- [X] T005 [P] Set up Tailwind CSS 4 and author the single design-token source in `src/styles/theme.css` (`@theme` block with the github-visualize tokens from research.md §11).
- [X] T006 [P] Configure lint/format: `rustfmt.toml` + `clippy` in CI script; ESLint + Prettier for `src/`.
- [X] T007 [P] Add `.gitignore` (include `.claude/`, `target/`, `node_modules/`, `dist/`, app data dir) and initialize git repo.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Persistence, domain types, command/event plumbing, app shell, macOS bootstrap.

**⚠️ CRITICAL**: No user-story work begins until this phase is complete.

- [X] T008 [P] Implement `crates/domain` pure types in `crates/domain/src/lib.rs`: `LayoutNode` (matches `contracts/layout-node.schema.json`), typed ids, and all enums from data-model.md (SessionState, scope, memory type, provider kind).
- [X] T009 Implement `crates/persistence` with `rusqlite` (`bundled`, FTS5) + `refinery`: migration `0001_init.sql` creating all tables from data-model.md, and `memory_fts` virtual table + sync triggers; open DB at `~/Library/Application Support/AITerminal/app.db` via `directories`.
- [X] T010 [P] Implement DAOs in `crates/persistence/src/` for projects, workspaces, layouts, panes, sessions, provider_profiles, usage_snapshots, app_settings (skills/memory DAOs added in their stories).
- [X] T011 [P] Define the `SessionHost` trait and support types in `crates/domain/src/host.rs` per `contracts/session-host.md` (trait only; `InProcessHost` impl lands in US1).
- [X] T012 Wire the Tauri command/event layer in `src-tauri/src/{commands.rs,events.rs,state.rs,lib.rs}`: app state, DB handle, typed-command registry, and the `ipc::Channel` plumbing for terminal output (per `contracts/`).
- [X] T013 [P] Implement `crates/platform-macos`: app-data-dir bootstrap (db/config/skills/memory/logs), login-shell env resolution + cache (research.md §9), and a `resolve_env` command.
- [X] T014 [P] Configure `tracing` logging with rotating file logs under the app data dir; define shared `thiserror` error types and map them to `HostError`.
- [X] T015 Build the app-shell UI: `src/components/` design-system primitives (Card, Button, SidebarFrame, WorkspaceTabs, PaneHeader) and `src/App.tsx` layout (resizable/collapsible sidebar 280–320px + top workspace tabs + main area), styled from `theme.css`.
- [X] T016 [P] Create Zustand stores skeleton in `src/stores/` (layout, sessions, projects, usage) and the typed Tauri command client in `src/lib/ipc.ts`.

**Checkpoint**: App boots to a themed shell, DB migrates, env resolves. `pnpm tauri dev` runs (quickstart Phase 0).

---

## Phase 3: User Story 1 - Compose a workspace of AI-agent terminals (Priority: P1) 🎯 MVP

**Goal**: Add Claude/Codex/OpenCode/shell terminals, split into an arbitrary tree, resize, maximize; layout persists and restores.

**Independent Test**: Build the four wireframe layouts, type into agents, restart → identical restore (SC-002, SC-003, SC-004).

### Tests for User Story 1

- [X] T017 [P] [US1] Vitest tests for the layout-tree reducer (split/resize/close/maximize, tree↔sizes invariants) in `src/features/workspaces/layoutTree.test.ts`.
- [X] T018 [P] [US1] `cargo test` PTY smoke test in `crates/pty-runtime/tests/` (spawn shell, write, read echo, resize, exit code).

### Implementation for User Story 1

- [X] T019 [P] [US1] Implement `crates/pty-runtime` with `portable-pty`: spawn, async read→batched chunks, write, resize, signal, exit in `crates/pty-runtime/src/lib.rs`.
- [X] T020 [P] [US1] Implement `crates/provider-runtime`: `AgentProvider` trait + builtin adapters (claude/codex/opencode/shell) producing `CommandSpec` via login-shell `exec` (research.md §9–10); `detect` executable/auth with a clear missing-CLI error (FR-013); plus **custom provider profiles** and the `list_providers`/`detect_provider`/`upsert_provider_profile` commands over `provider_profiles` (FR-015).
- [X] T021 [US1] Implement `InProcessHost` (SessionHost) in `src-tauri/src/host.rs` wrapping `pty-runtime` + `provider-runtime` (depends on T011, T019, T020).
- [X] T022 [US1] Implement session Tauri commands `create_session`, `write_input`, `resize_session`, `send_signal`, `close_session`, `restart_session`, `list_sessions`, `get_scrollback` + `TerminalOutput` Channel per `contracts/tauri-commands.md` (depends on T012, T021).
- [X] T023 [P] [US1] Build the xterm.js pane component in `src/features/terminals/TerminalPane.tsx` (addons fit/webgl/unicode11/search; web-links auto-open DISABLED); instance kept in a `ref`; bind input/output/resize to the session store.
- [X] T024 [US1] Implement the split-tree renderer in `src/features/workspaces/LayoutTree.tsx` with `react-resizable-panels` mapping `LayoutNode`; split right/down, drag-resize, maximize/restore, and the empty-pane `+` menu (new terminal / provider / project·worktree / recent session) (FR-003..FR-005).
- [X] T025 [US1] Implement provider picker + pane header wiring in `src/features/terminals/` (provider, state, activity dot, context menu: split/duplicate/restart/change-provider/rename/terminate) (FR-016).
- [X] T026 [US1] Persist and restore the layout tree and typed pane bindings: `save_layout`/`load_layout` commands + `workspace_layouts`/`panes` DAOs, restore on boot (FR-006, SC-002) (depends on T009, T024).
- [X] T027 [US1] Implement workspace tabs commands `list_workspaces`/`create_workspace`/`close_workspace` and wire `WorkspaceTabs` (FR-007).

**Checkpoint**: US1 fully functional — four wireframe layouts build, run agents interactively, and restore after restart (quickstart Phases 1–2).

---

## Phase 4: User Story 2 - Organize work by cloned project (Priority: P2)

**Goal**: Sidebar of projects with git status; open sessions in a project's cwd; background sessions across projects; per-project session history + resume.

**Independent Test**: albert/dashboard/genfoot listed with branch/dirty; shell in albert → `pwd` matches; second project's session coexists; reopen a past session resumes (SC-001, SC-011).

### Implementation for User Story 2

- [X] T028 [P] [US2] Implement `crates/project-manager` with `git2`: discover repos under configured roots (default `~/www`), add folder, clone (shell `git clone` for progress), status (branch, dirty, ahead/behind) in `crates/project-manager/src/lib.rs`.
- [X] T029 [US2] Implement project Tauri commands `list_projects`, `add_project_folder`, `clone_project`, `remove_project`, `get_git_status`, `set_project_trust` + `GitStatusChanged` event (depends on T028).
- [X] T030 [P] [US2] Build the sidebar PROJETOS section in `src/features/projects/ProjectList.tsx` (name/branch/status, add/clone, project selection, background-activity indicators) (FR-008..FR-011).
- [X] T031 [US2] Route session creation through a selected project's cwd and enforce the trust gate (Principle I) in `src-tauri/src/commands.rs` + `LaunchContext` (FR-010, FR-025).
- [X] T032 [P] [US2] Record per-project session history in `terminal_sessions` (with `resume_ref`), and implement `get_session_history` (FR-029) (depends on T009, T022).
- [X] T033 [US2] Add resume: `ResumeCapability` in `provider-runtime`, `resume_session` command, and history-click UI in `src/features/projects/SessionHistory.tsx` (new pane = fresh; click history = native resume) (FR-030, SC-011).

**Checkpoint**: US1 + US2 work independently; projects drive cwd and session history/resume.

---

## Phase 5: User Story 3 - Track AI usage and limits (Priority: P3)

**Goal**: Sidebar usage cards for Claude, Codex, OpenCode(→OpenRouter); one poll per provider; offline last-known; expired-auth state.

**Independent Test**: cards populate from real auth; one poll/provider/window via logs; network off → last snapshot (SC-006, SC-007).

### Tests for User Story 3

- [X] T034 [P] [US3] Adapter tests with `mockito` + `insta` snapshots in `crates/usage-core/tests/` (Anthropic, Codex, OpenRouter response parsing).

### Implementation for User Story 3

- [X] T035 [P] [US3] Implement `crates/usage-core` adapters: Anthropic (`~/.claude/.credentials.json` + Keychain fallback), Codex (`~/.codex/auth.json`), OpenRouter (API key env→config) in `crates/usage-core/src/adapters/`.
- [X] T036 [US3] Implement the single `UsagePoller` in `crates/usage-core/src/poller.rs`: ≥300s floor, ~60s cache, atomic write + `flock`, persist to `usage_snapshots`, mark stale (Principle IV) (depends on T035).
- [X] T037 [US3] Implement `get_usage`/`refresh_usage` commands + `UsageUpdated` and `ProviderAuthenticationExpired` events (depends on T036).
- [X] T038 [P] [US3] Build the USO sidebar cards in `src/features/usage/UsageCards.tsx` (Claude/Codex/OpenCode; reset timers; compact single-line; offline + expired-auth states) reading one shared snapshot (FR-017..FR-019).

**Checkpoint**: Daily-driver reached (US1–US3). Usage refreshes once per provider; offline safe.

---

## Phase 6: User Story 4 - Isolate concurrent agents with worktrees (Priority: P4)

**Goal**: Create/list/remove git worktrees; assign a pane to a worktree; two agents edit the same repo in isolation.

**Independent Test**: two worktrees of albert, an agent in each edits files → isolated (SC-008).

### Implementation for User Story 4

- [X] T039 [P] [US4] Implement `crates/worktree-manager` with `git2`: create (new/existing branch), list, remove worktrees in `crates/worktree-manager/src/lib.rs`.
- [X] T040 [US4] Implement `create_worktree`/`list_worktrees`/`remove_worktree` commands + `worktrees` DAO (depends on T009, T039).
- [X] T041 [US4] Add worktree UI under each project and the "new worktree" flow (branch → dir → open agent in cwd); allow a pane to target a specific worktree in `src/features/projects/` + `src/features/terminals/` (FR-012).

**Checkpoint**: Multi-agent isolation works via worktrees.

---

## Phase 7: User Story 5 - Save and restore layout presets (Priority: P5)

**Goal**: Save/duplicate a layout as a named preset; create a workspace from a preset.

**Independent Test**: save a 2×2 preset, create a workspace from it → reproduced (US5).

### Implementation for User Story 5

- [X] T042 [P] [US5] Implement `layout_presets` DAO and seed named presets (Review/Implementation/Debug/Multi-agent) in `crates/persistence/src/`.
- [X] T043 [US5] Implement `list_presets`, `save_preset`, `create_workspace_from_preset` commands (depends on T026, T042).
- [X] T044 [US5] Add preset UI: save/duplicate current layout and the top-bar `+` "from preset" in `src/features/workspaces/Presets.tsx` (FR-005, US5).

**Checkpoint**: Presets create and reproduce layouts.

---

## Phase 8: User Story 6 - Share skills across agents (Priority: P6)

**Goal**: Single skill library; scoped activation with precedence; non-destructive per-provider apply with preview/diff.

**Independent Test**: one global skill activated for Claude and Codex reaches both without manual duplication; removal reverts only app-created content (SC-009).

### Implementation for User Story 6

- [X] T045 [P] [US6] Implement `crates/skill-manager`: scan `~/Library/Application Support/AITerminal/skills/` (`skill.toml` + `instructions.md`), model bindings + precedence (session>workspace>worktree>project>global) in `crates/skill-manager/src/lib.rs`.
- [X] T046 [US6] Implement per-provider skill adapters (generate compiled version, compute diff, apply, record `applied_artifacts`, remove-only-created) per Principle III (depends on T045).
- [X] T047 [US6] Implement `list_skills`, `preview_skill_apply`, `apply_skill`, `remove_skill`, `set_skill_binding` commands + `skills`/`skill_bindings` DAO (depends on T009).
- [X] T048 [P] [US6] Build the SKILLS sidebar + activation UI with preview/diff modal in `src/features/skills/` (FR-020, FR-021).

**Checkpoint**: Skills shared across agents non-destructively.

---

## Phase 9: User Story 7 - Scoped project memory (Priority: P7)

**Goal**: Scoped memory (global/project/worktree/workspace/session), FTS search, opt-in selection capture, no cross-project leakage.

**Independent Test**: memory scoped to albert appears for albert agents, never for dashboard; keyword search returns it (SC-010).

### Implementation for User Story 7

- [X] T049 [P] [US7] Implement `crates/memory-manager`: scoped entries (Markdown + `memory_entries`), revisions, and FTS5 search over `memory_fts` in `crates/memory-manager/src/lib.rs`.
- [X] T050 [US7] Implement `list_memory`, `search_memory`, `add_memory`, `capture_selection_to_memory`, `preview_memory_context` commands (auto-capture OFF by default) (FR-022..FR-024) (depends on T009).
- [X] T051 [P] [US7] Build the MEMÓRIA sidebar: scoped editor, search, and terminal selection→"save as memory" (scope picker) with pre-injection preview in `src/features/memory/` (FR-022, FR-024).

**Checkpoint**: All user stories independently functional.

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Performance, security hardening, docs, and full acceptance run.

- [X] T052 [P] Performance pass: verify ≥12 terminals stay responsive under heavy output (webgl, batched chunks, bounded scrollback) (SC-003, SC-004).
- [X] T053 [P] Security hardening of terminal output (Principle III): disable auto link execution, confirm external URLs, sanitize titles, guard clipboard, cap scrollback in `src/features/terminals/`.
- [X] T054 [P] Customizable keybindings + macOS notifications in `src/features/` / `crates/platform-macos`.
- [X] T055 [P] Documentation: `README.md` and `docs/` (architecture, provider setup, design tokens).
- [X] T056 Run the full `quickstart.md` acceptance suite (Phases 0–9) and record results; `cargo clippy`, `cargo test`, `pnpm test`.
- [X] T057 Track deferred Phase-10 product work (daemon session persistence, Developer ID signing + notarization, auto-update) as follow-up — OUT of v1 scope.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup — **BLOCKS all user stories**.
- **User Stories (Phase 3+)**: all depend on Foundational. Recommended sequential by priority
  (P1→P7); US2/US4 build on US1's session machinery, US5 on US1's layout persistence.
- **Polish (Phase 10)**: depends on the desired stories being complete.

### User Story Dependencies

- **US1 (P1)**: after Foundational. Independent MVP.
- **US2 (P2)**: after Foundational; uses US1 session commands (cwd + history/resume).
- **US3 (P3)**: after Foundational; fully independent of US1/US2.
- **US4 (P4)**: after US2 (needs projects).
- **US5 (P5)**: after US1 (needs layout persistence).
- **US6 (P6)** / **US7 (P7)**: after Foundational; benefit from US2 scopes but are independently testable.

### Within Each User Story

Tests (where present) → crates (models/services) → Tauri commands → UI → integration → checkpoint.

### Parallel Opportunities

- Setup: T005, T006, T007 in parallel.
- Foundational: T008, T010, T011, T013, T014, T016 in parallel after T009/T012 land.
- Within US1: T017/T018 (tests) and T019/T020/T023 (different files) in parallel.
- US3 is independent of US1/US2 → can be built by a separate developer right after Foundational.

---

## Parallel Example: User Story 1

```bash
# Tests first (parallel):
Task: "Vitest layout-tree reducer in src/features/workspaces/layoutTree.test.ts"   # T017
Task: "cargo test PTY smoke in crates/pty-runtime/tests/"                            # T018

# Then core crates (parallel, different files):
Task: "pty-runtime in crates/pty-runtime/src/lib.rs"                                 # T019
Task: "provider-runtime builtin adapters in crates/provider-runtime/src/lib.rs"     # T020
Task: "TerminalPane.tsx (xterm) in src/features/terminals/TerminalPane.tsx"          # T023
```

---

## Implementation Strategy

### MVP First (Setup + Foundational + US1)

1. Phase 1 Setup → 2. Phase 2 Foundational (blocks all) → 3. Phase 3 US1 →
4. **STOP & VALIDATE**: four wireframe layouts + restore (quickstart Phases 0–2) → demo.

### Incremental Delivery to daily-driver

Add US2 (projects + history/resume) → test → demo; add US3 (usage) → test → demo. US1–US3 is the
usable daily driver. Then US4 (worktrees), US5 (presets), US6 (skills), US7 (memory) each ship as
an independent increment. Phase 10 hardens; daemon/signing are deferred beyond v1.

---

## Notes

- `[P]` = different files, no incomplete dependency; `[US#]` maps to spec.md user stories.
- Every command task must respect Principle I (typed, validated) and Principle VII (via `SessionHost`).
- Secrets never touch `app.db`/`config.toml` (Principle III).
- Commit after each task or logical group; stop at any checkpoint to validate a story independently.

---

## Phase 11: Convergence

> Appended by `/speckit-converge` on 2026-07-14 after the full implement pass. Assesses the code
> against spec/plan/tasks/constitution and lists the remaining work as new traceable tasks.
> **Append-only** — nothing above was modified. Complete these with `/speckit-implement`.
>
> Verified solid (no task needed): `ipc::Channel` output streaming with 8 ms / 64 KiB batching,
> real usage adapters (Anthropic/Codex/OpenRouter) with one poller + 300 s floor + 60 s lock-cache,
> non-destructive skill apply/remove, FTS5 memory with scope isolation, zero runtime `unwrap`.
> Out of scope (intentionally deferred, see docs/deferred.md): daemon session persistence, Developer
> ID signing/notarization, signed auto-update.

### Critical (constitution / baseline)

- [x] T058 [CRIT] Detect PTY self-exit: wire `PtyProcess::try_exit` (crates/pty-runtime/src/lib.rs:111) into the host, transition the session to `Exited`, finalize the DB row (`ended_at`/`exit_code`), and emit `ProcessExited` so `list_sessions` and activity indicators are honest — per daemon-events.md + Constitution VI + FR-028 (missing).
- [x] T059 [CRIT] Enforce the "allowed path" invariant end-to-end: validate `add_project_folder` accepts only paths under a configured root, and make `clone_project` honor `project_root_dirs` instead of the hardcoded `$HOME/www` (src-tauri/src/commands.rs:788-825) — per Constitution I (contradicts).
- [x] T060 [CRIT] Reattach live sessions after a workspace switch: rehydrate `sessionId` via `list_sessions` + `get_scrollback` on `load_layout` so switching workspaces never orphans running PTYs (src/App.tsx:98; `listSessions` unused) — per FR-011 (partial).

### High

- [x] T061 [HIGH] Build the custom-provider-profile UI (label/command/args/color) that calls `upsert_provider_profile` (src/lib/ipc.ts:219, never called today) — per FR-015 (missing).
- [x] T062 [HIGH] Make sidebar session-resume work with splits: drop the `layout.type !== "pane"` guard and open the resumed session into a chosen/new pane (src/App.tsx:180) — per FR-030 / SC-011 (contradicts).
- [x] T063 [HIGH] Capture and persist the real agent session id into `resume_ref` instead of the hardcoded `"continue"` (src-tauri/src/commands.rs:207) so `ResumeRef::ById` (crates/provider-runtime/src/lib.rs:74-90) can resume a specific past session — per FR-030 (partial).
- [x] T064 [HIGH] Replace the hand-typed-UUID "change worktree" prompt with a worktree picker (src/App.tsx:319) — per FR-012 (partial).
- [x] T065 [HIGH] De-duplicate and order terminal output using the `seq` field, attach the pane listener before session start, and gate the scrollback backfill so early/live chunks are neither double-written nor dropped (src/features/terminals/TerminalPane.tsx:90-99) — per Constitution II / FR-002 (partial).

### Medium

- [x] T066 [MED] Populate `PaneHeader` detail (project · branch · worktree) and render process state; `PaneBinding.detail` is never set today (src/App.tsx:132-145) — per FR-016 (partial).
- [x] T067 [MED] Fix `restart_session` DB drift: finalize the old session row and insert the new one (src-tauri/src/commands.rs:329-345; host.rs:213-224) — per session-history robustness (partial).
- [x] T068 [MED] Parse OSC title sequences from PTY output and emit `SessionTitleChanged`, wiring dynamic titles into headers — per daemon-events.md (missing).
- [x] T069 [MED] Subscribe to `GitStatusChanged` and refresh branch/dirty/ahead-behind and the activity dot live (src/features/projects/ProjectList.tsx:37,46 are static after first load) — per FR-009 / FR-011 (partial).
- [x] T070 [MED] Surface `HostErrorEvent` in the UI (session/host error toasts); the event type exists but nothing listens — per FR-028 (partial).
- [x] T071 [MED] On layout reload, re-offer each pane's saved provider as an actionable start instead of the generic picker (src/features/workspaces/WorkspaceLayout.tsx:185) — per SC-002 / US5-AS2 (partial).
- [x] T072 [MED] Track the focused pane so split/close/maximize shortcuts act on the active pane rather than always the first (src/App.tsx:63) — per US1 (partial).
- [x] T073 [MED] Expose skill scope/precedence via `set_skill_binding` in `SkillsPanel` (currently unused) — per FR-020 (partial).
- [x] T074 [MED] Add a compact single-line usage-card mode for a narrow sidebar (src/features/usage/UsageCards.tsx:55) — per FR-019 / AS3.4 (partial).
- [x] T075 [MED] Make an unscoped `search_memory` search across all scopes instead of defaulting to global-only (crates/memory-manager/src/lib.rs:153-157) — per FR-022 (partial).

### Low

- [x] T076 [LOW] Add an autonomous background usage poller loop (honoring the 300 s floor) and fix the shared 60 s cache so a never-polled provider isn't blocked (crates/usage-core/src/poller.rs:89-92) — per FR-017 / Constitution IV (partial).
- [x] T077 [LOW] Keep FTS `body` in sync on memory edits (the AFTER UPDATE trigger syncs only `title`) (crates/persistence/migrations/V001__init.sql:30) — per FR-022 (partial).
- [x] T078 [LOW] Quality hardening: add clippy `expect_used = "deny"` (test-allowed), replace the fragile `unreachable!()` (src-tauri/src/commands.rs:1523) with a typed error, and avoid persisting a null usage snapshot on serialize error (commands.rs:1163) — per plan quality gates (partial).
- [x] T079 [LOW] Remove dead code / wire orphans: unused Zustand stores (layout/sessions/projects) vs component-local state (violates CLAUDE.md "state in stores"), unused `Card.tsx`, unused ipc (`sendSignal`/`detectProvider`/`removeProject`), the unreachable `get_git_status`, the never-shown `detectProvider` install hint, the raw-JSON keybinding prompt, and the inert `memoryAutoCapture` toggle — cleanup (unrequested/partial).

---

## Phase 12: Convergence

> Appended by a second `/speckit-converge` on 2026-07-14 after Phase 11 was implemented and the
> full gate passed (cargo fmt/clippy -D/test, prettier/tsc/eslint/vitest/vite build). All 22
> Phase-11 tasks are satisfied; only two LOW residual partials remain (both were flagged as known
> limitations during implementation). No CRITICAL/HIGH/MEDIUM gaps remain. Out of scope (deferred,
> see docs/deferred.md): daemon session persistence, Developer ID signing/notarization, auto-update.

- [x] T080 [LOW] Thread the active worktree/workspace/session refIds into `SkillsPanel` so skills can be bound at all five scopes (currently only global/project are selectable; worktree/workspace/session are UI-disabled) — per FR-020 (partial).
- [x] T081 [LOW] Capture each agent's own native session id (scan `~/.claude/projects`, `~/.codex`, and the OpenCode session store) and persist it as `resume_ref` so a history entry resumes that *specific* past session via `--resume <id>` rather than "continue last" — per FR-030 (partial).

## Phase 13: Usage refresh responsiveness

> Appended 2026-07-15 after a bug report: clicking a usage card appeared inert. Root cause: the
> `refresh_usage` command applied the 300s autonomous floor to user-initiated clicks, so a click
> inside the window returned `scheduled:false` and emitted no `usage-updated` event. Clarified in
> spec §Clarifications (2026-07-15) / FR-018 and contracts/tauri-commands.md `refresh_usage`.

- [x] T082 [MED] Make an explicit user refresh (`refresh_usage`) honor the ~60s cache window instead of the 300s floor, while the single background poller keeps the 300s floor: add a `manual` flag to `UsagePoller::refresh` that gates a known provider on `CACHE_SECONDS` (crates/usage-core/src/poller.rs), pass `true` from the command and `false` from the autonomous loop, and remove the dead duplicate `spawn_loop` — per FR-018 / Constitution IV. Behavior locked by unit tests on the extracted `provider_due` helper.
- [x] T083 [MED] Make the usage card click responsive: show a per-card loading state (`animate-pulse` + `disabled`), and after `refreshUsage` always re-read `get_usage` → `setSnapshot` so a throttled click still shows the freshest cached values with feedback (src/features/usage/UsageCards.tsx) — per FR-018 / FR-019.

## Phase 14: Pane keyboard navigation + non-destructive sidebar refresh

> Appended 2026-07-15 from two user requests: navigate open terminals with the arrow keys, and
> stop a page reload from wiping the terminals. Decisions in spec §Clarifications (2026-07-15) /
> FR-031 / FR-032.

- [x] T084 [MED] Spatial pane focus navigation: added pure `paneRects`/`neighborPane` geometry helpers to `src/features/workspaces/layoutTree.ts` (+ vitest), wired `focusLeft/Right/Up/Down` keybindings (default `Meta+Arrow*`) into the `App.tsx` keydown handler to move `focusedPaneId` (and follow a maximized pane), and gave the newly-active pane real keyboard focus by passing `active` to `TerminalPane` and calling `term.focus()`. Added the four defaults to Rust `default_keybindings` — per FR-031 / Constitution II.
- [x] T085 [MED] Non-destructive sidebar refresh: intercept `Cmd/Ctrl+R` in `App.tsx` to `preventDefault` the WebView reload and instead re-fetch sidebar data in place (dispatch `projects-refresh` + a new `sidebar-refresh` event consumed by `UsageCards` and `SessionHistory`), with an info toast + a sidebar refresh button. Leaves the in-process PTYs/panes untouched — per FR-032. (Reattach-after-real-reload deferred; note: plain Cmd+R is a native dev reload the JS can't preempt — Option B/T088 makes reload non-destructive instead.)

## Phase 15: Session resume & reload survival

> Appended 2026-07-15. Two user reports: the pane's "Resume {provider}" button opened a blank
> session instead of continuing, and the panes' focus shortcut needed a chord (Cmd+Shift+Arrow,
> T084 updated). And the confirmed Option B reattach (T088).

- [x] T086 [MED] Pane keyboard-nav shortcut changed to `Cmd/Ctrl+Shift+Arrow` (plain Cmd+Arrow is consumed by macOS line-navigation before the app sees it); updated `App.tsx` + Rust `default_keybindings` + spec FR-031/Clarification — per FR-031.
- [x] T087 [MED] Terminal input: `macOptionIsMeta: false` so Option composes accented chars (ç/á/ã/ê) on a macOS/ABNT keyboard instead of acting as Meta (src/features/terminals/TerminalPane.tsx) — user-reported input bug.
- [x] T088 [MED] "Resume {provider}" picker button now resumes instead of starting blank: `ipc.createSession` forwards an optional `resume` ref; `ProviderPicker` primary button calls `onResumeProvider` → `startProvider(resume: Continue)` → `claude --continue` / `codex resume` / `opencode --continue` in the pane's own cwd, with a secondary "começar do zero" for a fresh start (src/lib/ipc.ts, App.tsx, WorkspaceLayout.tsx, ProviderPicker.tsx) — per FR-030. Backend already accepted `resume`.
- [ ] T089 [MED] Option B — reattach live sessions on any WebView reload (FR-032): persist `session_id` per pane (migration + save/load_layout + PaneBinding), reconcile restored panes against `list_sessions` on mount, add `attach_session` re-pointing a live session's output to a fresh Channel (host.rs swappable sink + SessionHost::attach, Principle VII), frontend reattaches instead of showing the picker. Un-defer from docs/deferred.md.

## Phase 16: Visual redesign, trust removal, and sidebar rework

> Appended 2026-09-03 from a run of user requests: the chrome looked misaligned and its buttons
> did not read as buttons; a cyberpunk direction; the SVGL brand marks; removal of project trust;
> a pinned usage footer; a reported reauthentication bug; per-workspace project folders; and
> archiving projects. Decisions in spec FR-033/FR-034/FR-035, research §11, constitution 2.0.0.

- [x] T090 [MED] Design system rebuilt on one token source (`src/styles/theme.css`): night-city duotone ramp (magenta leads, cyan answers), neon expressed as emission (`--shadow-glow`) reserved for live state, two type families with roles (`--font-ui` chrome / `--font-mono` data), a five-step type scale replacing ad-hoc 9–12px sizes, one radius family, and the `.scanlines` / `.hud-grid` textures. Docs: `docs/design-tokens.md`, research §11.
- [x] T091 [MED] Control primitives so every clickable thing reads as one: `Button` (default/ghost/accent/danger × sm/md, raised fill + inner highlight + press), `Menu`, `Modal`, `Field/Select/TextInput/TextArea`. Replaced four ad-hoc dropdowns, four ad-hoc modals and the bare-text buttons across pane actions, presets, settings, provider profiles, skills and memory.
- [x] T092 [MED] Official SVGL brand marks for Claude/Codex/OpenCode inlined in `src/lib/providers.tsx` (CSP forbids remote assets), drawn in `currentColor` so each takes its provider identity token.
- [x] T093 [MED] Chrome alignment: sidebar brand row and workspace tab bar share a 44px baseline; pane header lost its colored top strip in favour of the brand mark plus a neon hairline on the focused pane; UI copy unified to pt-BR.
- [x] T094 [HIGH] Project trust removed end to end at the user's direction: three `PROJECT_UNTRUSTED` gates, `set_project_trust`, the DAO writer, the `ProjectSummary` field and the sidebar affordance. Constitution amended to 2.0.0 (Principle I no longer lists project trust; the allowed-root check is the sole boundary on where a session launches). `projects.trusted` stays in V001 unused — see `docs/deferred.md`. Per FR-025.
- [x] T095 [MED] Usage readouts pinned to a non-scrolling sidebar footer (`SidebarFrame` gained a `footer` slot), alongside the open tab's context.
- [x] T096 [HIGH] Usage authentication reporting fixed (FR-035). Root cause: the stored Claude OAuth access token had passed `expiresAt`, and the poller's `or_insert_with` hardcoded `AuthState::Expired`, so a network failure and an expired token both rendered "reautentique" forever. `AuthState` gained `Unknown` and `Rejected`; only auth errors may move the state; `AnthropicAdapter::token` now reads `expiresAt`, prefers an unexplored-but-valid candidate, and no longer lets a malformed credentials file mask the Keychain. The app deliberately does not refresh the token itself (Principle III).
- [x] T097 [MED] Per-workspace project folder (FR-033): migration V003 `workspaces.root_path`, `set_workspace_root` with `~` expansion and directory validation, `list_projects(workspaceId)` scoping both discovery and the listing, pinned roots joined into `allowed_roots`, and a folder picker on the Projetos section.
- [x] T098 [MED] Project archiving (FR-034): migration V003 `projects.archived_at`, `set_project_archived`, an "Arquivados" toggle at the top of the sidebar and a per-project overflow menu to archive/restore. Rediscovery does not clear the flag.
- [x] T099 [MED] Project rows simplified to name + live-session dot at `text-title`; branch, clean/dirty, ahead/behind and worktree selection moved to `WorkspaceContext` in the pinned footer, where they describe the workspace actually open (FR-009).
- [x] T100 [LOW] "Recentes" removed from the sidebar (`SessionHistory` deleted). FR-030 is still met: the empty-pane picker lists recent sessions for the selected project and resumes them natively, so the capability lost a duplicate surface, not the feature.
- [x] T101 [MED] Project list reconciled against the filesystem (FR-008): `list_projects` now drops any stored row whose directory is gone or is no longer a git repository (`live_project_summary`), and the sidebar re-lists on window focus as well as on `projects-refresh`. The row is kept rather than pruned — `projects` cascades into `terminal_sessions` and `workspaces`, so deleting on a transient absence would destroy session history; `remove_project` remains the explicit way to forget one.
- [x] T102 [MED] Open-workspace context moved out of the sidebar and into the pane that opens the session (FR-016): the launcher's existing working-directory/worktree selectors gained the branch, clean/dirty and ahead/behind readout, and now seed from whatever the sidebar has selected (`LayoutTree` threads `defaultProjectId`/`defaultWorktreeId` into `ProviderPicker`). The sidebar footer is usage only; `WorkspaceContext` deleted.
- [x] T103 [MED] Native folder chooser (FR-033): `tauri-plugin-dialog` added but driven only from Rust behind a typed `pick_directory` command, so the WebView gains no file-dialog API. Creating a workspace (button or `newWorkspace` shortcut) now asks for its folder immediately; cancelling leaves it on the configured roots. The Projetos folder button opens the chooser directly, replacing the typed-path modal (`WorkspaceRootModal` deleted).
- [x] T104 [MED] Project rename (FR-036): migration V004 `projects.display_name`, `set_project_name`, and a Renomear entry in the per-project menu with a dialog that restores the directory name when emptied. Kept separate from `projects.name` because discovery rewrites that column on every scan.
- [x] T105 [HIGH] Fixed one workspace's projects leaking into another (FR-033). Two defects: (1) the listing scope for a workspace with no pinned folder used `allowed_roots`, which is the *security* union of every workspace root — split into `configured_roots` (display fallback) and `allowed_roots` (the Principle I boundary, still a union); (2) the display filter tested `path.starts_with(root)` while `project_manager::discover` scans one level with `read_dir`, so a workspace pinned to `~/www` listed repositories under `~/www/thayna` that its own scan can never find. The filter now matches on the project's parent directory. Verified against the live database: the two nested workspaces went from 8/35 with 8 overlapping to 8/27 with zero. `ProviderPicker` also listed every project because it called `listProjects()` with no workspace — now scoped, and archived projects excluded.
- [x] T106 [LOW] Workspace rename (FR-037): `rename_workspace` command + `WorkspacesDao::set_title`, driven by double-clicking the tab — inline edit committing on Enter or blur, cancelling on Escape. No extra chrome added to a bar whose job is to stay quiet.
- [x] T107 [LOW] `Cmd/Ctrl+Shift+R` performs a real `window.location.reload()` (FR-032). Plain `Cmd+R` still refreshes the sidebar in place, which left no way to reload the window at all — a problem while iterating on layout. The reload is issued explicitly rather than by letting the event through, because the WebView does not reliably bind the chord itself. Panes come back on the picker until the T089 reattach lands.
