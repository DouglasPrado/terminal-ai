# Implementation Plan: AI Terminal Workspace

**Branch**: `001-ai-terminal-workspace` | **Date**: 2026-07-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-ai-terminal-workspace/spec.md`

## Summary

A macOS desktop workspace where a developer runs and arranges multiple AI-agent terminals
(Claude, Codex, OpenCode, shell) in arbitrary split layouts, organized by cloned project and
git worktree, with a sidebar for projects, skills, memory and provider-usage cards. Technical
approach: a **Tauri 2** app with a **Rust** core exposing typed commands to a **React + TypeScript
+ Tailwind CSS 4** frontend; each session runs in a **real PTY** (`portable-pty`) rendered by
**xterm.js**; state persists in **SQLite** (`rusqlite`, FTS5); usage is polled by a single Rust
poller reimplementing the provider adapters. Sessions run **in-process** in v1 behind a
`SessionHost` trait so a persistent daemon can replace it later without UI changes. Design tokens
are ported verbatim from `github-visualize` into a single Tailwind `@theme` block.

## Technical Context

**Language/Version**: Rust (stable, latest via `rustup`; edition 2021) for the core; TypeScript
5.x on Node 22.23 / pnpm 10.9 for the frontend.

**Primary Dependencies**:
- Desktop shell: **Tauri 2** (`tauri`, `tauri-build`).
- Frontend: **React 18**, **Vite**, **Tailwind CSS 4**, **Zustand**, **@xterm/xterm** + addons
  (`addon-fit`, `addon-search`, `addon-web-links`, `addon-unicode11`, `addon-webgl`),
  **react-resizable-panels** (split tree), **@dnd-kit** (pane drag).
- Rust core: **tokio** (async), **portable-pty** (PTY), **rusqlite** (SQLite + `bundled` + FTS5),
  **reqwest 0.12** (usage HTTP), **serde/serde_json**, **git2** (status/worktrees), **keyring** /
  **security-framework** (Keychain), **directories** (paths), **tracing** (logs), **thiserror**.

**Storage**: SQLite `app.db` for structured state and FTS5 memory index; Markdown files for
portable skill/memory content; `config.toml` for provider profiles/preferences; **secrets never
stored** — read from Keychain and the provider CLIs' own files.

**Testing**: `cargo test` (unit + integration; `insta` snapshots for adapters, `mockito` for HTTP
adapter tests); **Vitest** + Testing Library for frontend logic (layout-tree reducers, stores);
end-to-end **quickstart** scenarios driving the running app for phase acceptance.

**Target Platform**: macOS on Apple Silicon (darwin arm64), Tauri 2 desktop app.

**Project Type**: desktop-app — monorepo (Cargo workspace + pnpm workspace) with a Rust core, a
Tauri shell, and a React frontend.

**Performance Goals**: UI boot < 2s; typing latency below one frame (~16ms); ≥ 12 simultaneous
PTYs without UI degradation; terminal output streamed in time-batched blocks; 60fps interactions.

**Constraints**: exactly one usage poll per provider per window with a ≥ 300s floor and ~60s
cache; zero layout loss across restarts; credentials never written to `app.db`/`config.toml`;
frontend has no arbitrary-command primitive; one terminal's failure never blocks the UI.

**Scale/Scope**: single-user desktop; on the order of dozens of projects/worktrees and dozens of
concurrent PTYs. Delivery order: core P1–P3 (terminals+layout, projects, usage) then P4–P7
(worktrees, presets, skills, memory).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| # | Principle | Gate | Status |
|---|-----------|------|--------|
| I | Typed Rust Boundary | Frontend calls only typed Tauri commands; every command validates path/provider/cwd/env; no `execute_any_command`. | ✅ PASS — contracts define a closed command set (`contracts/tauri-commands.md`). |
| II | Native PTY Fidelity | Every session is a real PTY; output streamed in batched blocks; no stdout-pipe wrapper. | ✅ PASS — `portable-pty` + Tauri Channel streaming (`research.md`). |
| III | Non-Destructive & Credential-Safe | Skills/memory apply via preview→diff→record→remove-only-created; secrets in Keychain/CLI files; output untrusted. | ✅ PASS — data-model separates state from secrets; skill/memory sync recorded in `data-model.md`. |
| IV | Single Source of Truth | One usage poller per provider (≥300s, 60s cache); one `@theme` token source. | ✅ PASS — single `UsagePoller`; tokens in `src/styles/theme.css`. |
| V | Layout as Persisted Tree | Arbitrary split tree persisted and restored losslessly. | ✅ PASS — `LayoutNode` schema + `workspace_layouts` table. |
| VI | Isolation & Resilience | One session's failure isolated; concurrent same-repo agents use worktrees. | ✅ PASS — per-session tasks; `worktree-manager`. |
| VII | Swappable Session Host | Command layer behind `SessionHost`; in-process now, daemon later, no UI change. | ✅ PASS — `contracts/session-host.md` trait; `InProcessHost` (v1). |

Post-Phase-1 re-check: **PASS** — the data model, contracts, and structure introduce no principle
violations. No entries required in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/001-ai-terminal-workspace/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 output — decisions & rationale
├── data-model.md        # Phase 1 output — SQLite schema & entities
├── quickstart.md        # Phase 1 output — runnable phase-acceptance scenarios
├── contracts/           # Phase 1 output — IPC & internal contracts
│   ├── tauri-commands.md
│   ├── daemon-events.md
│   ├── session-host.md
│   └── layout-node.schema.json
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
terminal-ai/
├── Cargo.toml                 # Cargo workspace
├── pnpm-workspace.yaml        # pnpm workspace
├── package.json
├── src/                       # React frontend
│   ├── components/            # design-system primitives (Card, Sidebar, Tabs, PaneHeader…)
│   ├── features/
│   │   ├── projects/          # sidebar project list, git status
│   │   ├── workspaces/        # workspace tabs + split-tree renderer (react-resizable-panels)
│   │   ├── terminals/         # xterm.js pane, session binding, resume-from-history
│   │   ├── skills/            # skill library + activation UI
│   │   ├── memory/            # scoped memory editor + search
│   │   └── usage/             # provider usage cards
│   ├── stores/                # Zustand stores (layout, sessions, projects, usage)
│   └── styles/theme.css       # single Tailwind @theme token source
├── src-tauri/                 # Tauri shell + typed command layer + SessionHost wiring
│   ├── Cargo.toml  tauri.conf.json
│   └── src/                   # commands.rs, events.rs, state.rs, host.rs (InProcessHost)
└── crates/
    ├── domain/                # pure entities & rules (LayoutNode, ids, enums)
    ├── pty-runtime/           # PTY spawn/resize/signal/output (portable-pty)
    ├── provider-runtime/      # AgentProvider trait + Claude/Codex/OpenCode/Shell/custom adapters
    ├── usage-core/            # reimplemented usage adapters + single poller + cache
    ├── project-manager/       # git discovery, clone, status (git2)
    ├── worktree-manager/      # git worktree create/list/remove
    ├── skill-manager/         # skill library, bindings, per-provider sync (non-destructive)
    ├── memory-manager/        # scoped memory + FTS5
    ├── persistence/           # rusqlite, migrations, DAOs
    └── platform-macos/        # Keychain, login-shell env resolution, notifications
# Deferred (Phase 10): crates/daemon/, crates/ipc/ — DaemonHost replaces InProcessHost.
```

**Structure Decision**: A single monorepo combining a Cargo workspace (Rust core split into
focused crates) and a pnpm-managed React frontend, orchestrated by Tauri 2. The Rust logic lives
in library crates (not only in `src-tauri`) so the Phase-10 daemon can reuse `pty-runtime`,
`provider-runtime`, and `usage-core` unchanged. The frontend is organized by feature to mirror the
user stories. This matches the approved root plan at
`/Users/douglasprado/.claude/plans/quero-que-me-ajude-warm-pnueli.md`.

## Complexity Tracking

No constitution violations — this section is intentionally empty. The one notable structural
choice (splitting the Rust core into many small crates rather than a single `src-tauri` crate) is
justified by Principle VII (Swappable Session Host): the crates are the seam that lets the daemon
be added later without a rewrite, so it reduces future complexity rather than adding present
complexity.
