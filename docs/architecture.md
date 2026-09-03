# Architecture

The React/TypeScript frontend communicates only through typed Tauri commands and events. It owns
the layout presentation, xterm instances, sidebar workflows, and transient UI state. Rust owns all
privileged work: PTYs, process signals, git operations, credential-backed usage requests, file
writes, and SQLite persistence.

The Rust workspace separates the pure domain model from runtime capabilities:

- `domain`: IDs, enums, layout validation, and the `SessionHost` and `MemoryKernel` contracts.
- `pty-runtime` and `provider-runtime`: native PTYs and provider command/resume adapters.
- `project-manager` and `worktree-manager`: libgit2-backed repository operations.
- `usage-core`: provider adapters and the single rate-limited poller.
- `skill-manager`: reversible skill artifacts.
- `memory-kernel`: the ai-memory kernel — supervisor, reads over `/api/v1`, writes through the
  binary, scope mapping, agent wiring, legacy import. Depends on `domain` alone, so a daemon can
  reuse it unchanged.
- `persistence`: SQLite migrations and DAOs.
- `platform-macos`: application paths, login-shell environment, logs, and notifications.
- `src-tauri`: orchestration and the IPC boundary.

Layouts are recursive `pane | split` trees. Pane bindings are persisted separately so a preset can
reproduce topology and provider choices without launching a process. PTY output is read on native
threads, coalesced in 8 ms / 64 KiB batches, sent over bounded channels, and rendered with xterm's
WebGL addon when available.

SQLite is the source of truth for metadata. Large/user-authored bodies stay as Markdown files.
Memory content is the exception: it left `app.db` entirely in feature 002 and now lives in the
ai-memory kernel's git-versioned wiki, in a store shared with whatever ai-memory the user runs
outside the app — see [ai-memory-kernel.md](./ai-memory-kernel.md). What SQLite keeps is records
*about* the kernel: what wiring was written where, what was imported, and which kernel project a
Terminal AI project last resolved to. The legacy `memory_entries`/`memory_fts` tables survive
read-only as the import's source and the way back. Provider secrets are read only from the
provider's files, environment, or macOS Keychain and never enter SQLite.
