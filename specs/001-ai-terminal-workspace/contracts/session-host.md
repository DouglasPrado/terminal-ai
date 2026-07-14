# Contract: `SessionHost` (Swappable Host Seam — Principle VII)

The command layer in `src-tauri` depends **only** on the `SessionHost` trait, never on a
concrete PTY or daemon. v1 wires `InProcessHost`; Phase 10 swaps in `DaemonHost` with **zero**
changes to the UI, the Tauri commands, or the event/channel contracts.

```rust
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Stable identifiers (newtypes over String) live in `crates/domain`.
pub struct SessionId(pub String);
pub struct ProjectId(pub String);
pub struct WorktreeId(pub String);
pub struct ProviderId(pub String);

/// Everything needed to launch (or resume) one session.
pub struct LaunchContext {
    pub project_id: Option<ProjectId>,
    pub worktree_id: Option<WorktreeId>,
    pub provider_id: ProviderId,
    pub cwd: std::path::PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub resume: Option<ResumeRef>,
}

/// How a past session is resumed, if the provider supports it.
pub enum ResumeRef {
    /// e.g. `claude --continue` — resume the most recent session in this cwd.
    Continue,
    /// e.g. `codex resume <id>` / `claude --resume <id>` — resume a specific session.
    ById(String),
}

/// The concrete process to spawn, produced by a provider adapter.
pub struct CommandSpec {
    pub program: std::path::PathBuf,      // absolute, resolved against login-shell PATH
    pub args: Vec<String>,
    pub cwd: std::path::PathBuf,
    pub env: Vec<(String, String)>,       // resolved login-shell env (+ overrides)
}

/// What a provider can do when asked to resume.
pub enum ResumeCapability {
    Continue,     // supports "continue last"
    ResumeById,   // supports "resume <id>"
    None,         // no resume; reopening starts fresh
}

pub enum SessionState { Starting, Running, Exited(Option<i32>), Error(String) }

pub struct SessionInfo {
    pub id: SessionId,
    pub provider_id: ProviderId,
    pub project_id: Option<ProjectId>,
    pub worktree_id: Option<WorktreeId>,
    pub pid: u32,
    pub title: String,
    pub state: SessionState,
}

/// One batched block of PTY output (mirrors `TerminalChunk` on the wire).
pub struct OutputChunk { pub seq: u64, pub bytes: Vec<u8> }

pub enum Signal { Int, Term, Kill, Hup }

#[async_trait]
pub trait SessionHost: Send + Sync {
    /// Spawn a session; output is pushed to `out`. Returns immediately with info.
    async fn create(&self, ctx: LaunchContext, out: mpsc::Sender<OutputChunk>)
        -> Result<SessionInfo, HostError>;

    async fn write(&self, id: &SessionId, data: &[u8]) -> Result<(), HostError>;
    async fn resize(&self, id: &SessionId, cols: u16, rows: u16) -> Result<(), HostError>;
    async fn signal(&self, id: &SessionId, sig: Signal) -> Result<(), HostError>;
    async fn close(&self, id: &SessionId) -> Result<Option<i32>, HostError>;

    /// Kill + respawn fresh (drops any `resume`).
    async fn restart(&self, id: &SessionId) -> Result<SessionInfo, HostError>;

    async fn list(&self) -> Result<Vec<SessionInfo>, HostError>;
    async fn scrollback(&self, id: &SessionId, max_bytes: usize)
        -> Result<(Vec<u8>, bool), HostError>;

    /// Resume from history; falls back to a fresh session if capability is `None`.
    async fn resume(&self, ctx: LaunchContext, out: mpsc::Sender<OutputChunk>)
        -> Result<(SessionInfo, bool /* resumed */), HostError>;
}

#[derive(thiserror::Error, Debug)]
pub enum HostError {
    #[error("session not found")] NotFound,
    #[error("provider not detected: {0}")] ProviderMissing(String),
    #[error("spawn failed: {0}")] Spawn(String),
    #[error("host transport error: {0}")] Transport(String),
}
```

## Implementors

### `InProcessHost` (v1)
Lives in `src-tauri`/`pty-runtime`. Holds a map `SessionId → PtyHandle`. `create` asks the
`provider-runtime` for a `CommandSpec` (which resolves the executable and login-shell env, and
applies the `resume` flags when present), spawns it through `portable-pty`, and starts a Tokio
task that reads the PTY, batches bytes (~4–16ms), and pushes `OutputChunk`s to `out`. `write`,
`resize`, and `signal` act directly on the `portable-pty` master. There is no cross-process
persistence: closing the app drops the map, but each session's metadata + `ResumeRef` were
already written to the project's session history, so `resume` works on next launch.

### `DaemonHost` (Phase 10)
Lives in `crates/daemon` + `crates/ipc`. Implements the same trait by serializing each call
across a Unix domain socket (`~/Library/Application Support/AITerminal/runtime/daemon.sock`) to
the long-lived `ai-terminal-daemon`, which owns the PTYs. Output chunks arrive over the socket
and are forwarded to the same `mpsc::Sender`. Because the trait, the Tauri commands, and the
`TerminalChunk`/event shapes are unchanged, swapping `InProcessHost` for `DaemonHost` is a
one-line wiring change in app startup — the UI cannot tell the difference except that sessions
now survive window close.

**Rule**: no code outside the host implementations may reference `portable-pty` or the socket.
Everything else programs against `dyn SessionHost`.
