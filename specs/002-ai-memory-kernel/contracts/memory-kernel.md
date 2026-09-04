# Contract: `MemoryKernel` (Swappable Memory Host Seam — Principle VII)

The command layer in `src-tauri` depends **only** on the `MemoryKernel` trait, never on a concrete
HTTP client, sub-process or JSON-RPC envelope. v1 wires `AiMemoryKernel` over a supervised or
attached `ai-memory` server; a future daemon swaps in with **zero** changes to the UI, the Tauri
commands, or the event contracts.

**The rule that keeps `domain` IO-free**: no signature below may carry `reqwest::Url`,
`serde_json::Value`, a JSON-RPC type, or any other transport type. Only `&str`, `String` and owned
domain structs. `crates/domain` gains **no new dependency** for this trait — `async-trait` and
`tokio` are already there.

```rust
use async_trait::async_trait;

/// Reused verbatim from feature 001 — `crates/domain/src/lib.rs`.
pub struct Scope { pub level: ScopeLevel, pub ref_id: Option<String> }
pub enum ScopeLevel { Global, Project, Worktree, Workspace, Session }
pub enum MemoryType { Fact, Decision, Constraint, Preference, Glossary, KnownIssue, Command, Todo }

/// Where a scope resolves inside the kernel. Constructed only by the scope mapper, never by a
/// caller — which is what makes an unscoped (leaking) query unrepresentable.
pub struct KernelScope { pub workspace: String, pub project: String, pub path_prefix: String }

/// One memory page. `id` is the kernel page path, so it stays a stable string key for the UI.
pub struct MemoryPage {
    pub id: String,
    pub scope: Scope,
    pub memory_type: MemoryType,
    pub title: String,
    pub body: String,
    pub author: PageAuthor,
    pub created_at: String,
    pub updated_at: String,
}

/// Whether Terminal AI wrote this page or an agent did (FR-049).
pub enum PageAuthor { TerminalAi, Agent }

pub struct MemorySource { pub entry_id: String, pub scope: Scope }

pub struct Handoff {
    pub id: String,
    pub agent: String,
    pub state: HandoffState,
    pub summary: String,
    pub open_questions: Vec<String>,
    pub next_steps: Vec<String>,
    pub created_at: String,
    pub accepted_at: Option<String>,
}
pub enum HandoffState { Open, Accepted, Expired }

pub struct KernelStatus {
    pub state: KernelState,
    /// The single gate on terminate/restart. False for a server the app merely found (FR-039).
    pub owned: bool,
    pub server_url: String,
    pub data_dir: Option<String>,
    pub version: Option<String>,
    pub version_matches_pin: bool,
    pub has_token: bool,
    pub pages: Option<u64>,
    pub pending_migration: u64,
    pub hybrid_search: bool,
    pub last_checked_at: String,
    pub last_error: Option<KernelError>,
    /// Actionable text for the state — install steps, quarantine fix, port conflict (FR-044).
    pub guidance: Option<String>,
}

pub enum KernelState {
    NotInstalled, Probing, Starting, Ready, Attached, Degraded, PortConflict, Failed,
}

#[async_trait]
pub trait MemoryKernel: Send + Sync {
    /// Reads the supervisor's cached snapshot. MUST NOT perform IO and MUST NOT fail —
    /// this is what keeps a dead kernel from blocking the UI (FR-041, FR-042).
    async fn status(&self) -> KernelStatus;

    async fn list(&self, scope: &Scope, limit: usize) -> Result<Vec<MemoryPage>, MemoryError>;
    async fn search(&self, query: &str, scope: &Scope, limit: usize)
        -> Result<Vec<MemoryPage>, MemoryError>;
    async fn read(&self, scope: &Scope, path: &str) -> Result<MemoryPage, MemoryError>;
    async fn write(&self, scope: &Scope, kind: MemoryType, title: &str, body: &str)
        -> Result<String, MemoryError>;
    async fn update(&self, scope: &Scope, path: &str, title: Option<&str>, body: &str)
        -> Result<(), MemoryError>;
    async fn delete(&self, scope: &Scope, path: &str) -> Result<(), MemoryError>;

    async fn compose_context(&self, scope: &Scope, max_bytes: usize)
        -> Result<(String, Vec<MemorySource>), MemoryError>;
    async fn briefing(&self, scope: &Scope) -> Result<String, MemoryError>;

    async fn handoffs(&self, scope: &Scope, state: HandoffState)
        -> Result<Vec<Handoff>, MemoryError>;
    async fn accept_handoff(&self, scope: &Scope, id: &str) -> Result<(), MemoryError>;
    async fn cancel_handoff(&self, scope: &Scope, id: &str) -> Result<(), MemoryError>;
}
```

**Note the absent method.** There is no `begin_handoff`. Creating a handoff is an agent action at the
end of its own session; the app offering to create one on the user's behalf would fabricate a record
of work it did not do (FR-060).

**Note `search`'s scope is not `Option`.** An unscoped query against the kernel returns pages from
every project — verified. Making the parameter required is how FR-046 is enforced by the type system
rather than by discipline.

```rust
pub enum MemoryError {
    /// The kernel is not ready. Returned immediately, without attempting IO (Principle VI).
    Unavailable(KernelState),
    Unauthorized,
    NotFound,
    InvalidScope(String),
    InvalidPath(String),
    /// The kernel answered, and said no.
    Upstream { code: Option<String>, message: String },
    /// The kernel answered something we could not parse — an upgrade probably moved a shape.
    Protocol(String),
    Transport(String),
}
```

Mapped at the command boundary into the existing `AppError { code, message }`, alongside the
`PersistenceError` and `rusqlite::Error` conversions already in `src-tauri/src/commands.rs`:

| `MemoryError` | `AppError.code` |
| --- | --- |
| `Unavailable(_)` | `MEMORY_KERNEL_UNAVAILABLE` |
| `Unauthorized` | `MEMORY_KERNEL_UNAUTHORIZED` |
| `NotFound` | `MEMORY_NOT_FOUND` |
| `InvalidScope(_)` | `INVALID_SCOPE` |
| `InvalidPath(_)` | `MEMORY_INVALID_PATH` |
| `Upstream{..}` | `MEMORY_KERNEL_UPSTREAM` |
| `Protocol(_)` | `MEMORY_KERNEL_PROTOCOL` |
| `Transport(_)` | `MEMORY_KERNEL_TRANSPORT` |

## Implementors

- **`AiMemoryKernel`** (v1, `crates/memory-kernel`) — reads over `/api/v1` with `reqwest`, writes by
  invoking the `ai-memory` binary with a fixed argv, status from a supervisor-owned cache.
- **`DaemonMemoryKernel`** (deferred) — the same trait over the Phase 10 daemon socket.
- **`FakeKernel`** (tests) — records calls; the migration and command tests run against it with no
  network and no sub-process.

**Rule**: a new capability is added to this trait **before** it is added to a Tauri command, and a
command never reaches around the trait to the kernel. If a feature cannot be expressed here, it does
not belong in the memory subsystem.
