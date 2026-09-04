# Terminal AI

**A native macOS workspace for running AI coding agents, shells and Git worktrees side by side.**

Terminal AI brings **Claude Code, Codex, OpenCode, shells and custom CLI providers** into one persistent desktop workspace.

Instead of managing multiple terminal windows, projects and agent sessions manually, Terminal AI gives each project a structured environment with split layouts, provider-aware panes, Git worktrees, usage visibility, shared skills and scoped memory.

Built with **Tauri 2, Rust, React and TypeScript**.

---

## Why Terminal AI?

Modern AI development rarely happens in a single terminal.

A typical workflow may involve:

- Claude working on one task
- Codex reviewing another
- OpenCode running in a separate project
- a shell for tests or Git operations
- isolated worktrees for parallel changes
- shared skills across multiple agents
- memory that needs to survive individual sessions

Terminal AI treats that workflow as a **desktop workspace**, not a collection of disconnected terminal windows.

```text
Project
├── Worktree A
│   ├── Claude
│   └── Shell
│
├── Worktree B
│   └── Codex
│
└── Main workspace
    ├── OpenCode
    └── Shell
```

The goal is simple:

> **Keep the terminal-native workflow, while making multi-agent development easier to organize, isolate and operate.**

---

## Features

### Multiple AI CLI providers

Terminal AI currently supports built-in integrations for:

- Claude Code
- Codex
- OpenCode
- login shell (`zsh`)
- custom CLI providers

Each provider runs as its real CLI inside a native PTY.

Terminal AI does not replace the provider's terminal interface with a custom chat implementation.

---

### Persistent split layouts

Build workspaces using recursive horizontal and vertical splits.

Layouts can represent:

```text
Single pane

┌─────────────────────────────┐
│           Claude            │
└─────────────────────────────┘
```

```text
Two columns

┌──────────────┬──────────────┐
│    Claude    │    Codex     │
│              │              │
└──────────────┴──────────────┘
```

```text
Multi-agent workspace

┌──────────────┬──────────────┐
│    Claude    │   OpenCode   │
├──────────────┼──────────────┤
│    Codex     │    Shell     │
└──────────────┴──────────────┘
```

The split tree, pane sizes and provider bindings can be persisted and restored.

---

### Layout presets

Save a workspace layout and reuse it later.

A preset stores the topology and provider choices without automatically starting processes.

This makes it possible to create reusable environments such as:

```text
Feature Development
├── Claude
├── Codex
├── Test Shell
└── Git Shell
```

or:

```text
Review Workspace
├── Codex
├── Claude
└── Shell
```

---

### Git project awareness

Projects are first-class objects.

Terminal AI tracks project information such as:

- repository path
- active branch
- clean / dirty state
- worktrees
- project-specific sessions

Agent processes always start inside a configured project root or one of its registered worktrees.

---

### Git worktrees

Create isolated Git worktrees for parallel agent work.

```text
repository/
│
├── main
│
├── worktree-feature-a
│   └── Claude
│
└── worktree-feature-b
    └── Codex
```

Different agents can work on separate branches without sharing the same working copy.

Worktree operations are handled natively through the Rust backend.

---

### Native terminal runtime

Terminal panes are backed by native PTYs.

Terminal AI uses `portable-pty` and renders terminal output with xterm.js.

The runtime is designed for interactive applications such as:

- Claude
- Codex
- OpenCode
- shells
- Vim
- long-running commands
- high-volume output

PTY output is coalesced before crossing the frontend boundary to reduce rendering overhead.

---

### Provider usage

Terminal AI can surface usage information for supported providers without duplicating provider credentials into its own database.

Usage polling is:

- centralized
- rate-limited
- cached
- independent from the number of open panes

If a provider usage endpoint temporarily fails, Terminal AI can retain the last known snapshot rather than treating the entire provider as unavailable.

---

### Native session resume

Where supported by the underlying provider, Terminal AI can resume existing CLI sessions using the provider's own mechanism.

Current built-in resume strategies include:

- Claude: `--continue` / `--resume`
- Codex: `resume`
- OpenCode: `--continue`

A new pane still starts a new session unless a resume action is explicitly selected.

---

### Custom providers

Terminal AI is not limited to the built-in AI CLIs.

Custom provider profiles can define:

- display label
- executable
- arguments
- display color
- non-secret environment values

Executables are resolved using the cached macOS login-shell environment.

Custom provider profiles are not intended to store API tokens or other secrets.

---

## Shared Skills

Terminal AI can manage reusable skills across supported agents.

Instead of manually duplicating the same skill into several provider-specific configuration locations, the application can:

1. prepare the target changes
2. show a preview / diff
3. apply the skill
4. record what Terminal AI created
5. remove only Terminal AI-managed artifacts later

The design favors **reversible writes**.

Removing a skill should not delete unrelated provider configuration.

---

## Memory

Terminal AI integrates with an external **ai-memory kernel** instead of owning a proprietary memory database.

The kernel stores memory as a Git-versioned Markdown wiki and maintains a rebuildable search index.

```text
~/Library/Application Support/ai-memory/
├── wiki/          # Git-versioned Markdown memory
├── db/            # Rebuildable search index
└── config.toml
```

Terminal AI acts as a client and supervisor of that memory system.

This means the same memory store can be shared between Terminal AI and agents running outside the application.

### Memory principles

- memory is not locked inside `app.db`
- the Markdown wiki remains user-readable
- the search index can be rebuilt
- Terminal AI only manages artifacts it owns
- destructive whole-store operations are not exposed through the app
- agent configuration changes require preview and confirmation

### Agent access

Memory access can be wired into supported agents through MCP.

Automatic capture is treated separately because it has a larger blast radius.

Terminal AI only offers automatic capture where it can respect the requested project scope.

---

## Architecture

Terminal AI separates the unprivileged UI from privileged native capabilities.

```text
┌───────────────────────────────────────────────┐
│              React / TypeScript               │
│                                               │
│  Layouts                                      │
│  Sidebar                                      │
│  xterm.js                                     │
│  Provider UI                                  │
│  Memory UI                                    │
│  Transient presentation state                │
└──────────────────────┬────────────────────────┘
                       │
                       │ Typed Tauri commands
                       │ and events
                       ▼
┌───────────────────────────────────────────────┐
│                    Rust                       │
│                                               │
│  PTYs                                         │
│  Process signals                              │
│  Git operations                               │
│  Worktrees                                    │
│  Provider execution                           │
│  Usage requests                               │
│  File writes                                  │
│  SQLite persistence                           │
│  Platform integration                         │
└──────────────────────┬────────────────────────┘
                       │
              ┌────────┼─────────┐
              │        │         │
              ▼        ▼         ▼
          Providers    Git     ai-memory
              │                 kernel
              ▼
      Claude / Codex /
        OpenCode / CLI
```

The frontend never performs privileged native work directly.

Privileged operations cross a typed Tauri IPC boundary.

---

## Rust workspace

The Rust backend is split into focused crates.

```text
crates/
├── domain/
├── memory-kernel/
├── persistence/
├── platform-macos/
├── project-manager/
├── provider-runtime/
├── pty-runtime/
├── skill-manager/
├── usage-core/
└── worktree-manager/
```

### `domain`

Pure domain types and contracts.

Contains:

- IDs
- enums
- layout validation
- session contracts
- memory contracts

The domain crate is intentionally kept independent from runtime infrastructure.

### `pty-runtime`

Owns native pseudo-terminal execution and terminal process interaction.

### `provider-runtime`

Maps provider definitions to commands and native resume behavior.

### `project-manager`

Handles project-level Git operations.

### `worktree-manager`

Handles Git worktree lifecycle and isolation.

### `usage-core`

Contains provider usage adapters and centralized rate-limited polling.

### `skill-manager`

Manages reversible agent skill artifacts.

### `memory-kernel`

Integrates Terminal AI with the external ai-memory service.

It handles:

- kernel supervision
- API communication
- project scope mapping
- agent wiring
- legacy-memory import

### `persistence`

Owns SQLite migrations and data access.

### `platform-macos`

Contains macOS-specific capabilities including:

- application paths
- login-shell environment
- logs
- notifications

### `src-tauri`

Application orchestration and the Tauri IPC boundary.

---

## Data model

SQLite is the source of truth for Terminal AI application metadata.

The database stores application state such as projects, layouts and runtime metadata.

Provider secrets do **not** belong in the Terminal AI database.

Large or user-authored content is kept outside SQLite when appropriate.

Memory content specifically lives in the ai-memory kernel's Git-versioned Markdown store.

Application data is stored under:

```text
~/Library/Application Support/AITerminal/
```

---

## Provider authentication

Terminal AI uses credentials already managed by each provider.

It does not create a second credential store for AI accounts.

Current provider integrations can read authentication from the provider's own configuration or macOS Keychain where appropriate.

Credentials are read in memory and are not copied into:

- `app.db`
- application logs
- custom provider profiles

For provider-specific details, see [`docs/providers.md`](./docs/providers.md).

---

## Security model

Terminal AI runs AI agents capable of executing commands, so process boundaries and filesystem scope are treated as product concerns.

### Allowed project roots

Agents can only launch inside:

- a configured project root
- a registered worktree belonging to that project

Paths escaping the allowed roots are rejected at the command boundary.

### Secrets

Provider credentials remain in provider-owned credential stores.

Terminal AI does not persist provider tokens in SQLite or logs.

### Terminal links

Links printed by terminal applications are not automatically opened.

### Terminal output

Output titles are sanitized and scrollback is bounded.

### Skills

Skill writes are previewed and tracked.

Removal targets only application-managed artifacts.

### Memory

Terminal-to-memory capture is explicit.

Automatic capture is disabled by default.

### Rust safety policy

The Rust workspace denies:

```rust
unsafe_code
```

Clippy also denies:

```text
unwrap_used
expect_used
```

The intent is to make failure paths explicit rather than relying on unchecked panics.

---

## Terminal runtime

Terminal layouts and terminal sessions are intentionally modeled separately.

A pane can exist without an active process.

This allows saved presets to reproduce:

- topology
- dimensions
- provider assignments

without unexpectedly launching agents.

PTY output is read natively and batched before it is sent to the WebView.

xterm.js uses WebGL acceleration when available.

---

## Project isolation

Project and worktree isolation is an important invariant.

Conceptually:

```text
Project A
├── main
│   └── Claude
│
└── feature-a
    └── Codex


Project B
└── main
    └── OpenCode
```

A provider launched for `Project A / feature-a` should not silently start in `Project A / main` or `Project B`.

The working directory is part of the execution boundary.

---

## Tech stack

### Desktop

- Tauri 2
- Rust
- React 19
- TypeScript
- Vite
- Tailwind CSS

### Terminal

- xterm.js
- xterm WebGL addon
- `portable-pty`

### State and UI

- Zustand
- react-resizable-panels
- dnd-kit
- Lucide

### Native backend

- Tokio
- libgit2 through `git2`
- SQLite through `rusqlite`
- Reqwest
- Serde
- Tracing

### Tooling

- pnpm
- Vitest
- ESLint
- Prettier
- Cargo
- Clippy
- rustfmt

---

## Repository structure

```text
terminal-ai/
├── .github/
│   └── workflows/
│
├── crates/
│   ├── domain/
│   ├── memory-kernel/
│   ├── persistence/
│   ├── platform-macos/
│   ├── project-manager/
│   ├── provider-runtime/
│   ├── pty-runtime/
│   ├── skill-manager/
│   ├── usage-core/
│   └── worktree-manager/
│
├── docs/
│
├── scripts/
│
├── specs/
│
├── src/                      # React / TypeScript frontend
│
├── src-tauri/                # Tauri app + native orchestration
│
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-lock.yaml
├── pnpm-workspace.yaml
├── vite.config.ts
└── README.md
```

---

## Requirements

Terminal AI currently targets macOS.

Development requires:

- macOS
- Xcode Command Line Tools
- Rust stable
- Node.js 22+
- pnpm 10+

Install Xcode Command Line Tools if needed:

```bash
xcode-select --install
```

Install Rust using rustup if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

---

## Run locally

Clone the repository:

```bash
git clone https://github.com/DouglasPrado/terminal-ai.git
cd terminal-ai
```

Install JavaScript dependencies:

```bash
pnpm install
```

Run the Tauri application in development:

```bash
pnpm tauri dev
```

---

## Build

Build the frontend:

```bash
pnpm build
```

Build the Tauri application:

```bash
pnpm tauri build
```

> Signing, notarization and automated updates are not part of the current v1 release process.

---

## Verification

### Rust

Check formatting:

```bash
cargo fmt --all --check
```

Run Clippy:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Run Rust tests:

```bash
cargo test --workspace
```

### Frontend

Lint:

```bash
pnpm lint
```

Run tests:

```bash
pnpm test
```

Build:

```bash
pnpm build
```

A complete local verification pass is:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm test
pnpm build
```

---

## Provider setup

Terminal AI resolves the macOS login-shell environment and augments `PATH` with common Homebrew, local and Cargo locations.

Built-in executable detection currently covers:

```text
claude
codex
opencode
/bin/zsh -l
```

If a provider executable is unavailable, the provider should be disabled with an actionable detection message rather than failing silently.

See [`docs/providers.md`](./docs/providers.md) for authentication, usage and custom-provider details.

---

## Design decisions

### Use the real CLI

Terminal AI does not reimplement Claude, Codex or OpenCode as custom chat clients.

The real CLI remains the provider runtime.

Benefits include:

- native provider behavior
- provider-owned authentication
- provider-specific commands
- native resume support
- lower coupling to undocumented APIs

---

### Tauri instead of an Electron-only architecture

The UI remains a familiar React application while privileged capabilities live in Rust.

This creates a clear boundary between:

```text
Presentation
     │
     ▼
Typed IPC
     │
     ▼
Native capabilities
```

---

### Domain separated from runtime

Core domain contracts do not need to know about PTYs, Git implementations, SQLite or Tauri.

This allows runtime components to evolve around a smaller stable model.

---

### Metadata in SQLite, user-owned content outside it

SQLite is appropriate for application metadata.

User-authored or externally shared data should remain portable when possible.

The memory system follows this principle by keeping memory in a Git-versioned Markdown wiki rather than trapping it inside the desktop application's database.

---

### Worktrees for parallel agents

Running several coding agents against the same working tree creates unnecessary contention.

Git worktrees provide independent working copies while keeping branch history inside the same repository.

---

### Reversible configuration writes

Integrating skills, hooks or memory into external AI tools changes files Terminal AI does not fully own.

The application therefore prefers:

```text
preview
   ↓
explicit confirmation
   ↓
tracked write
   ↓
safe removal / restore
```

over invisible configuration mutation.

---

### Local-first integration

Terminal AI is a desktop workspace.

Local project state, local terminals and local Git repositories remain useful without requiring a Terminal AI cloud backend.

---

## Current v1 boundaries

Some capabilities are intentionally deferred rather than partially hidden behind incomplete feature flags.

### Sessions do not survive application shutdown

PTY ownership currently lives inside the application process.

Closing the application terminates those hosted sessions.

A future daemon-based architecture may allow sessions to survive GUI restarts and upgrades.

### Signing and notarization

Apple Developer ID signing, hardened-runtime configuration and notarization are deferred.

Current local development builds are unsigned.

### Automatic updates

A signed auto-update feed, staged rollout and rollback are deferred.

Updates are currently manual.

### Full WebView reload

Normal workspace persistence is supported, but a real WebView reload can currently lose frontend session attachments while native PTYs remain alive.

A future reattachment mechanism is planned.

See [`docs/deferred.md`](./docs/deferred.md) for the engineering rationale and remaining work.

---

## Documentation

More detailed engineering documentation lives in [`docs/`](./docs).

Useful starting points:

- [`docs/architecture.md`](./docs/architecture.md) — system architecture and crate boundaries
- [`docs/providers.md`](./docs/providers.md) — provider detection, authentication and custom profiles
- [`docs/ai-memory-kernel.md`](./docs/ai-memory-kernel.md) — memory architecture
- [`docs/deferred.md`](./docs/deferred.md) — intentionally deferred product work
- [`specs/`](./specs) — product specifications and acceptance scenarios

---

## Engineering principles

Terminal AI is built around a few recurring principles.

### Explicit boundaries

Native capabilities belong in Rust and cross the frontend boundary through typed commands.

### Isolation

Agents operate inside explicitly configured project roots or worktrees.

### Reproducible workspaces

Layout topology and provider choices are persisted independently from process lifetime.

### Reversible mutations

When Terminal AI modifies external agent configuration, the change should be inspectable and removable.

### Provider ownership

Providers continue owning their own authentication and CLI behavior.

### User-owned memory

Memory should remain portable and readable outside the desktop application.

### Failure containment

A failure in an optional subsystem such as memory should not take down terminals, projects or the rest of the workspace.

---

## Project status

Terminal AI is under active development.

The current implementation already includes the foundations for:

- native multi-pane terminal execution
- Claude, Codex and OpenCode providers
- custom CLI providers
- persistent layout trees
- layout presets
- Git project awareness
- Git worktrees
- centralized usage polling
- reversible skills
- scoped memory integration
- provider-native session resume
- SQLite-backed application state
- macOS-native platform integration

Public APIs, persistence models and implementation details may evolve while the project is being developed.

---

## Contributing

Contributions and technical discussions are welcome.

Before submitting a change, run the complete verification suite:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm test
pnpm build
```

When changing architecture, preserve the main boundary:

```text
React / TypeScript
       │
       │ typed Tauri IPC
       ▼
     Rust
       │
       ▼
Native capabilities
```

Avoid moving privileged filesystem, process, Git or credential behavior into the WebView.

---

## Philosophy

AI coding tools are still strongest in the terminal.

The problem is no longer access to an agent.

The problem is managing **multiple agents, multiple projects, multiple worktrees and the context around them** without turning the desktop into a pile of disconnected terminal windows.

Terminal AI is an exploration of a more structured workflow:

> **Keep the agents terminal-native. Make the workspace intentional.**

---

## License

Released under the [MIT License](./LICENSE).
