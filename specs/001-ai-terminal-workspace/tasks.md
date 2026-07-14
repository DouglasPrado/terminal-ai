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

- [ ] T001 Install Rust via `rustup` and confirm `cargo`/`rustc`; ensure Xcode CLT (`xcode-select --install`).
- [ ] T002 Scaffold the app with `create-tauri-app` (React + TypeScript + Vite) into `/Users/douglasprado/www/terminal-ai`.
- [ ] T003 Define the Cargo workspace in `Cargo.toml` with member crates `crates/*` and `src-tauri`; create empty `crates/{domain,pty-runtime,provider-runtime,usage-core,project-manager,worktree-manager,skill-manager,memory-manager,persistence,platform-macos}` with `Cargo.toml` + `src/lib.rs`.
- [ ] T004 Configure the pnpm workspace (`pnpm-workspace.yaml`, `package.json`) and install frontend deps: `@xterm/xterm` + addons (`addon-fit`, `addon-search`, `addon-web-links`, `addon-unicode11`, `addon-webgl`), `react-resizable-panels`, `@dnd-kit/core`, `zustand`.
- [ ] T005 [P] Set up Tailwind CSS 4 and author the single design-token source in `src/styles/theme.css` (`@theme` block with the github-visualize tokens from research.md §11).
- [ ] T006 [P] Configure lint/format: `rustfmt.toml` + `clippy` in CI script; ESLint + Prettier for `src/`.
- [ ] T007 [P] Add `.gitignore` (include `.claude/`, `target/`, `node_modules/`, `dist/`, app data dir) and initialize git repo.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Persistence, domain types, command/event plumbing, app shell, macOS bootstrap.

**⚠️ CRITICAL**: No user-story work begins until this phase is complete.

- [ ] T008 [P] Implement `crates/domain` pure types in `crates/domain/src/lib.rs`: `LayoutNode` (matches `contracts/layout-node.schema.json`), typed ids, and all enums from data-model.md (SessionState, scope, memory type, provider kind).
- [ ] T009 Implement `crates/persistence` with `rusqlite` (`bundled`, FTS5) + `refinery`: migration `0001_init.sql` creating all tables from data-model.md, and `memory_fts` virtual table + sync triggers; open DB at `~/Library/Application Support/AITerminal/app.db` via `directories`.
- [ ] T010 [P] Implement DAOs in `crates/persistence/src/` for projects, workspaces, layouts, panes, sessions, provider_profiles, usage_snapshots, app_settings (skills/memory DAOs added in their stories).
- [ ] T011 [P] Define the `SessionHost` trait and support types in `crates/domain/src/host.rs` per `contracts/session-host.md` (trait only; `InProcessHost` impl lands in US1).
- [ ] T012 Wire the Tauri command/event layer in `src-tauri/src/{commands.rs,events.rs,state.rs,lib.rs}`: app state, DB handle, typed-command registry, and the `ipc::Channel` plumbing for terminal output (per `contracts/`).
- [ ] T013 [P] Implement `crates/platform-macos`: app-data-dir bootstrap (db/config/skills/memory/logs), login-shell env resolution + cache (research.md §9), and a `resolve_env` command.
- [ ] T014 [P] Configure `tracing` logging with rotating file logs under the app data dir; define shared `thiserror` error types and map them to `HostError`.
- [ ] T015 Build the app-shell UI: `src/components/` design-system primitives (Card, Button, SidebarFrame, WorkspaceTabs, PaneHeader) and `src/App.tsx` layout (resizable/collapsible sidebar 280–320px + top workspace tabs + main area), styled from `theme.css`.
- [ ] T016 [P] Create Zustand stores skeleton in `src/stores/` (layout, sessions, projects, usage) and the typed Tauri command client in `src/lib/ipc.ts`.

**Checkpoint**: App boots to a themed shell, DB migrates, env resolves. `pnpm tauri dev` runs (quickstart Phase 0).

---

## Phase 3: User Story 1 - Compose a workspace of AI-agent terminals (Priority: P1) 🎯 MVP

**Goal**: Add Claude/Codex/OpenCode/shell terminals, split into an arbitrary tree, resize, maximize; layout persists and restores.

**Independent Test**: Build the four wireframe layouts, type into agents, restart → identical restore (SC-002, SC-003, SC-004).

### Tests for User Story 1

- [ ] T017 [P] [US1] Vitest tests for the layout-tree reducer (split/resize/close/maximize, tree↔sizes invariants) in `src/features/workspaces/layoutTree.test.ts`.
- [ ] T018 [P] [US1] `cargo test` PTY smoke test in `crates/pty-runtime/tests/` (spawn shell, write, read echo, resize, exit code).

### Implementation for User Story 1

- [ ] T019 [P] [US1] Implement `crates/pty-runtime` with `portable-pty`: spawn, async read→batched chunks, write, resize, signal, exit in `crates/pty-runtime/src/lib.rs`.
- [ ] T020 [P] [US1] Implement `crates/provider-runtime`: `AgentProvider` trait + builtin adapters (claude/codex/opencode/shell) producing `CommandSpec` via login-shell `exec` (research.md §9–10); `detect` executable/auth with a clear missing-CLI error (FR-013); plus **custom provider profiles** and the `list_providers`/`detect_provider`/`upsert_provider_profile` commands over `provider_profiles` (FR-015).
- [ ] T021 [US1] Implement `InProcessHost` (SessionHost) in `src-tauri/src/host.rs` wrapping `pty-runtime` + `provider-runtime` (depends on T011, T019, T020).
- [ ] T022 [US1] Implement session Tauri commands `create_session`, `write_input`, `resize_session`, `send_signal`, `close_session`, `restart_session`, `list_sessions`, `get_scrollback` + `TerminalOutput` Channel per `contracts/tauri-commands.md` (depends on T012, T021).
- [ ] T023 [P] [US1] Build the xterm.js pane component in `src/features/terminals/TerminalPane.tsx` (addons fit/webgl/unicode11/search; web-links auto-open DISABLED); instance kept in a `ref`; bind input/output/resize to the session store.
- [ ] T024 [US1] Implement the split-tree renderer in `src/features/workspaces/LayoutTree.tsx` with `react-resizable-panels` mapping `LayoutNode`; split right/down, drag-resize, maximize/restore, and the empty-pane `+` menu (new terminal / provider / project·worktree / recent session) (FR-003..FR-005).
- [ ] T025 [US1] Implement provider picker + pane header wiring in `src/features/terminals/` (provider, state, activity dot, context menu: split/duplicate/restart/change-provider/rename/terminate) (FR-016).
- [ ] T026 [US1] Persist and restore the layout tree: `save_layout`/`load_layout` commands + `workspace_layouts` DAO, restore on boot (FR-006, SC-002) (depends on T009, T024).
- [ ] T027 [US1] Implement workspace tabs commands `list_workspaces`/`create_workspace`/`close_workspace` and wire `WorkspaceTabs` (FR-007).

**Checkpoint**: US1 fully functional — four wireframe layouts build, run agents interactively, and restore after restart (quickstart Phases 1–2).

---

## Phase 4: User Story 2 - Organize work by cloned project (Priority: P2)

**Goal**: Sidebar of projects with git status; open sessions in a project's cwd; background sessions across projects; per-project session history + resume.

**Independent Test**: albert/dashboard/genfoot listed with branch/dirty; shell in albert → `pwd` matches; second project's session coexists; reopen a past session resumes (SC-001, SC-011).

### Implementation for User Story 2

- [ ] T028 [P] [US2] Implement `crates/project-manager` with `git2`: discover repos under configured roots (default `~/www`), add folder, clone (shell `git clone` for progress), status (branch, dirty, ahead/behind) in `crates/project-manager/src/lib.rs`.
- [ ] T029 [US2] Implement project Tauri commands `list_projects`, `add_project_folder`, `clone_project`, `remove_project`, `get_git_status`, `set_project_trust` + `GitStatusChanged` event (depends on T028).
- [ ] T030 [P] [US2] Build the sidebar PROJETOS section in `src/features/projects/ProjectList.tsx` (name/branch/status, add/clone, project selection, background-activity indicators) (FR-008..FR-011).
- [ ] T031 [US2] Route session creation through a selected project's cwd and enforce the trust gate (Principle I) in `src-tauri/src/commands.rs` + `LaunchContext` (FR-010, FR-025).
- [ ] T032 [P] [US2] Record per-project session history in `terminal_sessions` (with `resume_ref`), and implement `get_session_history` (FR-029) (depends on T009, T022).
- [ ] T033 [US2] Add resume: `ResumeCapability` in `provider-runtime`, `resume_session` command, and history-click UI in `src/features/projects/SessionHistory.tsx` (new pane = fresh; click history = native resume) (FR-030, SC-011).

**Checkpoint**: US1 + US2 work independently; projects drive cwd and session history/resume.

---

## Phase 5: User Story 3 - Track AI usage and limits (Priority: P3)

**Goal**: Sidebar usage cards for Claude, Codex, OpenCode(→OpenRouter); one poll per provider; offline last-known; expired-auth state.

**Independent Test**: cards populate from real auth; one poll/provider/window via logs; network off → last snapshot (SC-006, SC-007).

### Tests for User Story 3

- [ ] T034 [P] [US3] Adapter tests with `mockito` + `insta` snapshots in `crates/usage-core/tests/` (Anthropic, Codex, OpenRouter response parsing).

### Implementation for User Story 3

- [ ] T035 [P] [US3] Implement `crates/usage-core` adapters: Anthropic (`~/.claude/.credentials.json` + Keychain fallback), Codex (`~/.codex/auth.json`), OpenRouter (API key env→config) in `crates/usage-core/src/adapters/`.
- [ ] T036 [US3] Implement the single `UsagePoller` in `crates/usage-core/src/poller.rs`: ≥300s floor, ~60s cache, atomic write + `flock`, persist to `usage_snapshots`, mark stale (Principle IV) (depends on T035).
- [ ] T037 [US3] Implement `get_usage`/`refresh_usage` commands + `UsageUpdated` and `ProviderAuthenticationExpired` events (depends on T036).
- [ ] T038 [P] [US3] Build the USO sidebar cards in `src/features/usage/UsageCards.tsx` (Claude/Codex/OpenCode; reset timers; compact single-line; offline + expired-auth states) reading one shared snapshot (FR-017..FR-019).

**Checkpoint**: Daily-driver reached (US1–US3). Usage refreshes once per provider; offline safe.

---

## Phase 6: User Story 4 - Isolate concurrent agents with worktrees (Priority: P4)

**Goal**: Create/list/remove git worktrees; assign a pane to a worktree; two agents edit the same repo in isolation.

**Independent Test**: two worktrees of albert, an agent in each edits files → isolated (SC-008).

### Implementation for User Story 4

- [ ] T039 [P] [US4] Implement `crates/worktree-manager` with `git2`: create (new/existing branch), list, remove worktrees in `crates/worktree-manager/src/lib.rs`.
- [ ] T040 [US4] Implement `create_worktree`/`list_worktrees`/`remove_worktree` commands + `worktrees` DAO (depends on T009, T039).
- [ ] T041 [US4] Add worktree UI under each project and the "new worktree" flow (branch → dir → open agent in cwd); allow a pane to target a specific worktree in `src/features/projects/` + `src/features/terminals/` (FR-012).

**Checkpoint**: Multi-agent isolation works via worktrees.

---

## Phase 7: User Story 5 - Save and restore layout presets (Priority: P5)

**Goal**: Save/duplicate a layout as a named preset; create a workspace from a preset.

**Independent Test**: save a 2×2 preset, create a workspace from it → reproduced (US5).

### Implementation for User Story 5

- [ ] T042 [P] [US5] Implement `layout_presets` DAO and seed named presets (Review/Implementation/Debug/Multi-agent) in `crates/persistence/src/`.
- [ ] T043 [US5] Implement `list_presets`, `save_preset`, `create_workspace_from_preset` commands (depends on T026, T042).
- [ ] T044 [US5] Add preset UI: save/duplicate current layout and the top-bar `+` "from preset" in `src/features/workspaces/Presets.tsx` (FR-005, US5).

**Checkpoint**: Presets create and reproduce layouts.

---

## Phase 8: User Story 6 - Share skills across agents (Priority: P6)

**Goal**: Single skill library; scoped activation with precedence; non-destructive per-provider apply with preview/diff.

**Independent Test**: one global skill activated for Claude and Codex reaches both without manual duplication; removal reverts only app-created content (SC-009).

### Implementation for User Story 6

- [ ] T045 [P] [US6] Implement `crates/skill-manager`: scan `~/Library/Application Support/AITerminal/skills/` (`skill.toml` + `instructions.md`), model bindings + precedence (session>workspace>worktree>project>global) in `crates/skill-manager/src/lib.rs`.
- [ ] T046 [US6] Implement per-provider skill adapters (generate compiled version, compute diff, apply, record `applied_artifacts`, remove-only-created) per Principle III (depends on T045).
- [ ] T047 [US6] Implement `list_skills`, `preview_skill_apply`, `apply_skill`, `remove_skill`, `set_skill_binding` commands + `skills`/`skill_bindings` DAO (depends on T009).
- [ ] T048 [P] [US6] Build the SKILLS sidebar + activation UI with preview/diff modal in `src/features/skills/` (FR-020, FR-021).

**Checkpoint**: Skills shared across agents non-destructively.

---

## Phase 9: User Story 7 - Scoped project memory (Priority: P7)

**Goal**: Scoped memory (global/project/worktree/workspace/session), FTS search, opt-in selection capture, no cross-project leakage.

**Independent Test**: memory scoped to albert appears for albert agents, never for dashboard; keyword search returns it (SC-010).

### Implementation for User Story 7

- [ ] T049 [P] [US7] Implement `crates/memory-manager`: scoped entries (Markdown + `memory_entries`), revisions, and FTS5 search over `memory_fts` in `crates/memory-manager/src/lib.rs`.
- [ ] T050 [US7] Implement `list_memory`, `search_memory`, `add_memory`, `capture_selection_to_memory`, `preview_memory_context` commands (auto-capture OFF by default) (FR-022..FR-024) (depends on T009).
- [ ] T051 [P] [US7] Build the MEMÓRIA sidebar: scoped editor, search, and terminal selection→"save as memory" (scope picker) with pre-injection preview in `src/features/memory/` (FR-022, FR-024).

**Checkpoint**: All user stories independently functional.

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Performance, security hardening, docs, and full acceptance run.

- [ ] T052 [P] Performance pass: verify ≥12 terminals stay responsive under heavy output (webgl, batched chunks, bounded scrollback) (SC-003, SC-004).
- [ ] T053 [P] Security hardening of terminal output (Principle III): disable auto link execution, confirm external URLs, sanitize titles, guard clipboard, cap scrollback in `src/features/terminals/`.
- [ ] T054 [P] Customizable keybindings + macOS notifications in `src/features/` / `crates/platform-macos`.
- [ ] T055 [P] Documentation: `README.md` and `docs/` (architecture, provider setup, design tokens).
- [ ] T056 Run the full `quickstart.md` acceptance suite (Phases 0–9) and record results; `cargo clippy`, `cargo test`, `pnpm test`.
- [ ] T057 Track deferred Phase-10 product work (daemon session persistence, Developer ID signing + notarization, auto-update) as follow-up — OUT of v1 scope.

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
