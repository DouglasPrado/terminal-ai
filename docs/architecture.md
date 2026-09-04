# Architecture

The React/TypeScript frontend communicates only through typed Tauri commands and events. It owns
the layout presentation, xterm instances, sidebar workflows, and transient UI state. Rust owns all
privileged work: PTYs, process signals, git operations, credential-backed usage requests, file
writes, and SQLite persistence.

The Rust workspace separates the pure domain model from runtime capabilities:

- `domain`: IDs, enums, layout validation, and the `SessionHost` contract.
- `pty-runtime` and `provider-runtime`: native PTYs and provider command/resume adapters.
- `project-manager` and `worktree-manager`: libgit2-backed repository operations.
- `usage-core`: provider adapters and the single rate-limited poller.
- `skill-manager` and `memory-manager`: reversible skill artifacts and isolated Markdown memory.
- `persistence`: SQLite migrations and DAOs.
- `platform-macos`: application paths, login-shell environment, logs, and notifications.
- `src-tauri`: orchestration and the IPC boundary.

Layouts are recursive `pane | split` trees. Pane bindings are persisted separately so a preset can
reproduce topology and provider choices without launching a process. PTY output is read on native
threads, coalesced in 8 ms / 64 KiB batches, sent over bounded channels, and rendered with xterm's
WebGL addon when available.

SQLite is the source of truth for metadata. Large/user-authored bodies stay as Markdown files;
memory entries maintain append-only revision files and an FTS5 index. Provider secrets are read
only from the provider's files, environment, or macOS Keychain and never enter SQLite.
