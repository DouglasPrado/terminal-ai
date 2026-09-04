# Terminal AI

Terminal AI is a native macOS workspace for running Claude, Codex, OpenCode, shells, and custom
CLI providers side by side. It combines persistent split layouts, git projects and worktrees,
provider usage, shared skills, and scoped memory in a Tauri 2 desktop app.

## Run locally

Requirements: macOS, Xcode Command Line Tools, Rust stable, Node 22+, and pnpm 10+.

```bash
pnpm install
pnpm tauri dev
```

Application state is stored in `~/Library/Application Support/AITerminal/`. SQLite contains app
state only; provider credentials remain in each provider's own credential store.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm lint
pnpm test
pnpm build
```

See [architecture](docs/architecture.md), [provider setup](docs/providers.md), [design tokens](docs/design-tokens.md), and the [acceptance guide](specs/001-ai-terminal-workspace/quickstart.md).

## Security model

- Agents only ever launch inside a configured project root or one of its worktrees; any
  path escaping those roots is rejected at the command boundary.
- Terminal links never auto-open; output titles are sanitized and scrollback is bounded.
- Usage polling is centralized, rate-limited, cached, and never persists credentials.
- Skill writes are previewed and marked; removal only touches app-managed artifacts.
- Terminal-to-memory capture is explicit and automatic capture defaults to off.

Daemon-backed sessions, release signing/notarization, and auto-update are intentionally deferred
from v1; see [deferred work](docs/deferred.md).
