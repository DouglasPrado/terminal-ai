# CLAUDE.md — Terminal AI

Operating manual for any Claude/agent session working in this repository. Read this first.

**Terminal AI** is a macOS desktop workspace (Tauri 2 + Rust + React) for running and arranging
multiple AI-agent terminals (Claude, Codex, OpenCode, shell) side by side, organized by cloned
project and git worktree, with a sidebar for projects, skills, memory and provider-usage cards.

> **Current state (2026-07-14):** Specs are complete; **implementation has not started**. The
> code tree described in *Architecture* does not exist yet — the next step is Phase 1 (Setup) of
> `specs/001-ai-terminal-workspace/tasks.md`. Keep this note updated as phases land.

---

## 0. The one rule: everything goes through Spec Kit (SDD)

This project uses **Spec-Driven Development** with GitHub **Spec Kit**. **Do not write or change
product code without a spec, plan, and tasks for it.** Specs are the source of truth; code
implements them.

- Feature artifacts live in `specs/NNN-feature-name/`.
- Project principles live in `.specify/memory/constitution.md` — **non-negotiable** (see §1).
- Templates/scripts live in `.specify/`; the workflow skills live in `.claude/skills/speckit-*`.

### The workflow (in order)

| Step | Skill | When / what it produces |
|------|-------|-------------------------|
| 1. Constitution | `/speckit-constitution` | Establish/amend project principles → `.specify/memory/constitution.md`. Rare (only when principles change). |
| 2. Specify | `/speckit-specify` | The **what & why** (user stories, FRs, SCs) → `specs/NNN-*/spec.md`. No tech details. |
| 3. Clarify | `/speckit-clarify` | Resolve ambiguities with ≤5 targeted questions → `## Clarifications` in the spec. **Run before plan.** |
| 4. Plan | `/speckit-plan` | The **how** (stack, architecture) → `plan.md` + `research.md`, `data-model.md`, `contracts/`, `quickstart.md`. Must pass the Constitution Check gate. |
| 5. Tasks | `/speckit-tasks` | Dependency-ordered, story-grouped `tasks.md`. |
| 6. Analyze | `/speckit-analyze` | Read-only consistency check across spec/plan/tasks/constitution. **Resolve CRITICAL findings before implementing.** |
| 7. Implement | `/speckit-implement` | Execute tasks. Work story-by-story; validate each checkpoint via `quickstart.md`. |
| — Converge | `/speckit-converge` | Re-assess the codebase vs spec/plan/tasks and append remaining work. |

### Rules of the flow

- **New capability** → start at Specify (new `specs/NNN-*/`). **Changing scope of an existing
  feature** → update that feature's `spec.md`, re-run Clarify/Plan/Tasks as needed, re-Analyze.
- **Never skip Analyze before Implement.** A change that can't trace back to an FR/SC/task means
  the spec is incomplete — fix the spec first.
- Keep artifacts in sync: if implementation reveals a design change, update `plan.md`/`data-model.md`/
  `contracts/` **and** `tasks.md`, then re-Analyze. Code and specs must not drift.
- The active feature directory is recorded in `.specify/feature.json`.
- Extension hooks: none registered (`.specify/extensions.yml` absent) — skip hook steps.

---

## 1. The constitution is law

`.specify/memory/constitution.md` defines 7 principles. They are hard constraints on every plan,
task, and line of code. Summary + practical guardrails (**MUST**/**NEVER**):

1. **Typed Rust Boundary** — the frontend calls only typed Tauri commands; each validates
   trust/path/provider/cwd/env. **NEVER** expose a generic `execute_any_command(string)`.
2. **Native PTY Fidelity** — every session is a real PTY (`portable-pty`); output streamed in
   time-batched blocks over an `ipc::Channel`. **NEVER** capture agent stdout through a non-PTY pipe.
3. **Non-Destructive & Credential-Safe** — skills/memory apply via preview→diff→apply→record→
   remove-only-created. **NEVER** put secrets in `app.db`/`config.toml` (Keychain / CLI files only).
   Treat terminal output as untrusted (no auto-link exec, sanitize titles, bound scrollback).
4. **Single Source of Truth** — one usage poller per provider (≥300s floor, ~60s cache).
   **NEVER** poll per-terminal/per-card. One design-token source: `src/styles/theme.css` `@theme`.
5. **Layout as a Persisted Tree** — arbitrary split tree, persisted and restored losslessly.
6. **Isolation & Resilience** — one session's failure never blocks the UI; concurrent same-repo
   agents use separate git worktrees.
7. **Swappable Session Host** — all session ops go through the `SessionHost` trait so the v1
   in-process runtime can be replaced by a daemon later **without UI/command changes**.

If a requirement conflicts with a principle, change the requirement — or amend the constitution
explicitly via `/speckit-constitution`. Do not silently dilute a principle.

---

## 2. Architecture (clean, layered, maintainable)

Monorepo: a **Cargo workspace** (Rust core split into focused crates) + a **pnpm** React frontend,
orchestrated by **Tauri 2**.

```
terminal-ai/
├── src/                    # React + TS frontend (feature-organized)
│   ├── components/         # design-system primitives (Card, Sidebar, Tabs, PaneHeader…)
│   ├── features/{projects,workspaces,terminals,skills,memory,usage}/
│   ├── stores/             # Zustand stores (layout, sessions, projects, usage)
│   ├── lib/ipc.ts          # the ONLY channel to the backend (typed command client)
│   └── styles/theme.css    # single Tailwind @theme token source
├── src-tauri/              # composition root: wires commands/events, implements InProcessHost
│   └── src/{commands,events,state,host}.rs
└── crates/
    ├── domain/             # pure types & rules (LayoutNode, ids, enums, SessionHost trait). NO IO.
    ├── pty-runtime/        # PTY spawn/resize/signal/output (portable-pty)
    ├── provider-runtime/   # AgentProvider trait + Claude/Codex/OpenCode/Shell/custom adapters
    ├── usage-core/         # usage adapters + the single UsagePoller + cache
    ├── project-manager/    # git discovery/clone/status (git2)
    ├── worktree-manager/   # git worktree create/list/remove
    ├── skill-manager/      # skill library, bindings, non-destructive per-provider sync
    ├── memory-manager/     # scoped memory + FTS5
    ├── persistence/        # rusqlite + refinery migrations + DAOs
    └── platform-macos/     # Keychain, login-shell env resolution, notifications
# Deferred (Phase 10): crates/{daemon,ipc}/ — DaemonHost replaces InProcessHost, UI unchanged.
```

### Dependency direction (enforce this)

```
React UI ──(typed commands only)──▶ src-tauri ──▶ feature crates ──▶ domain
                                        │                    ▲
                                        └────────────────────┘ (all depend inward on `domain`)
```

- **`domain`** depends on nothing internal and does no IO. All shared types/enums/traits live here.
- **Feature/infra crates** depend only on `domain` (+ external crates). They do **not** depend on
  each other sideways unless truly necessary, and **never** depend "up" on `src-tauri`.
- **`src-tauri`** is the composition root: it wires DAOs, adapters, and `InProcessHost`, and exposes
  the typed command surface. Business logic does **not** live here — it lives in the crates.
- **The frontend** talks to the backend **only** through `src/lib/ipc.ts` (typed commands) and the
  output `Channel`. No `fs`/`shell`/network from the WebView.
- **Reusability seam:** `pty-runtime`, `provider-runtime`, `usage-core` must stay free of Tauri so
  the future `daemon` crate can reuse them unchanged (Principle VII).

### Where do I put new code?

- New agent/CLI → an adapter in `provider-runtime` (+ optional `provider_profiles` row). Never
  special-case a provider in the UI or `src-tauri`.
- New usage source → an adapter in `usage-core`; register it with the single poller. Never add a
  second poller.
- New persisted data → a migration + DAO in `persistence`; a type/enum in `domain`.
- New backend capability → a typed command in `src-tauri` (validated) delegating to a crate;
  document it in `specs/NNN-*/contracts/tauri-commands.md`.
- New UI → a `src/features/<area>/` component reading a Zustand store; style from `theme.css` tokens.

### Contracts (authoritative interfaces)

See `specs/001-ai-terminal-workspace/contracts/`:
`tauri-commands.md` (closed command set), `daemon-events.md` (event/stream shapes),
`session-host.md` (the `SessionHost` trait), `layout-node.schema.json` (the layout tree).
Change a contract in the spec **before** changing its implementation.

---

## 3. Stack conventions

**Rust**
- Async on **tokio**; run blocking SQLite (`rusqlite`, `bundled`, FTS5) via `spawn_blocking`.
- Errors: `thiserror` per crate; map to `HostError` at the command boundary. No `unwrap()` in
  non-test code paths that can fail at runtime.
- Keep crates small and single-purpose; put pure logic in `domain`, IO at the edges.
- Migrations: `refinery`, sequential SQL in `persistence`. Never mutate a shipped migration.

**Frontend (React + TS)**
- One xterm.js instance per pane, kept in a `ref` — **never** in React state (perf).
- Global state in **Zustand** stores under `src/stores/`; keep components thin.
- All backend calls go through `src/lib/ipc.ts`. No ad-hoc `invoke` scattered in components.
- Styling: Tailwind 4 utilities + the `@theme` tokens only. Per-agent color is an accent detail
  (top strip / status dot / active border), never a pane fill.

**Design tokens** (single source, from `github-visualize`): app `#0b0a10`, panel `neutral-950/60`,
border `neutral-800`→hover `neutral-700`→active `fuchsia-700`, accent `fuchsia-400`, data pink
`#f472b6` / cyan `#22d3ee`, mono font stack. Full set in `src/styles/theme.css` / `research.md §11`.

---

## 4. Commands

```bash
# Dev / run
pnpm install                 # install frontend deps
pnpm tauri dev               # run the app (hot reload)
pnpm tauri build             # production bundle

# Quality gates (run before committing)
cargo fmt --all && cargo clippy --all-targets -- -D warnings
cargo test                   # Rust unit + integration (adapters use mockito/insta)
pnpm test                    # Vitest (layout-tree reducer, stores)

# Acceptance
# Drive scenarios in specs/001-ai-terminal-workspace/quickstart.md (per phase).
# Prefer the /verify and /run skills to exercise the app end-to-end.
```

App data lives at `~/Library/Application Support/AITerminal/` (`app.db`, `config.toml`, `skills/`,
`memory/`, `logs/`). Secrets are **not** stored there.

---

## 5. Maintainability guardrails (quick DON'T list)

- ❌ No generic command execution exposed to the frontend (Principle I).
- ❌ No secrets/API keys in `app.db` or `config.toml` (Principle III).
- ❌ No second usage poller; no per-terminal/per-card polling (Principle IV).
- ❌ No second design-token source; don't hardcode hex in components (Principle IV).
- ❌ No business logic in `src-tauri` or in React components — it belongs in crates/stores.
- ❌ No sideways/upward crate dependencies; keep `domain` IO-free.
- ❌ No code changes without a corresponding spec/plan/task; re-run `/speckit-analyze` after edits.
- ✅ Stream terminal output via `Channel` in batched blocks; ✅ persist the layout tree losslessly;
  ✅ route sessions through `SessionHost`; ✅ isolate concurrent same-repo agents with worktrees.

---

## 6. `.gitignore` essentials

`.claude/` (may hold agent credentials), `target/`, `node_modules/`, `dist/`, and the app data dir
must be git-ignored.
