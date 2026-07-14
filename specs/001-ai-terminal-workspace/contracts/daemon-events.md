# Contract: Events & Output Stream

These are the messages that flow **from** the Rust core **to** the frontend. In v1 they are
Tauri events plus one `ipc::Channel` per session for output. In Phase 10 the identical payload
shapes cross the Unix-socket boundary to the `ai-terminal-daemon` (Principle VII): the frontend
contract does not change when the host is swapped.

## Output stream (per session)

Terminal output is delivered on the `ipc::Channel<TerminalChunk>` passed to `create_session` /
`resume_session`, **not** as an event and **not** as a command result (Principle II).

```ts
// TerminalChunk — a batched block of raw PTY bytes
{ sessionId: string, seq: number, bytes: string /* base64 or utf-8 */ }
```

**Batching**: the core coalesces PTY reads for ~4–16ms (or up to a size threshold) before
emitting a chunk, so a flood of output produces tens of messages per second, not thousands
(Principle II / performance budget). `seq` is monotonic per session for gap detection.

## Events (broadcast)

```ts
// SessionStarted
{ sessionId: string, providerId: string, projectId?: string, worktreeId?: string,
  pid: number, title: string }

// ProcessExited
{ sessionId: string, exitCode: number | null, signal?: string, at: string }

// SessionTitleChanged  (from OSC title sequences, sanitized)
{ sessionId: string, title: string }

// GitStatusChanged  (project or worktree status shifted)
{ projectId: string, worktreeId?: string, branch: string, dirty: boolean,
  ahead: number, behind: number }

// UsageUpdated  (single poller published a new shared snapshot)
{ updatedAt: string, offline: boolean, providers: string[] /* ids that changed */ }

// ProviderAuthenticationExpired
{ providerId: string, detail: string }

// HostError  (recoverable host/daemon problem surfaced to UI)
{ scope: "session" | "usage" | "git" | "host", sessionId?: string,
  code: string, message: string }
```

## Delivery semantics

- Events are **advisory**: the frontend reconciles authoritative state via the corresponding
  query command (`list_sessions`, `get_usage`, `get_git_status`) — an event tells the UI *when*
  to refresh, not the whole truth.
- `TerminalChunk` ordering is guaranteed per session via `seq`; on reconnect the UI calls
  `get_scrollback` and resumes from the channel.
- `UsageUpdated` never carries per-terminal data — there is exactly one snapshot (Principle IV).
- Window titles in `SessionTitleChanged` are sanitized (control chars stripped) before emission
  (Principle III — untrusted output).
