//! The memory-host seam (Constitution VII).
//!
//! `src-tauri` depends on [`MemoryKernel`] and never on a concrete HTTP client, sub-process or
//! JSON-RPC envelope. v1 wires an ai-memory-backed implementation; a daemon can replace it without
//! touching the UI or the command contracts.
//!
//! **Rule**: nothing in this module may name a transport type. Only `&str`, `String` and owned
//! domain structs cross this boundary — that is what keeps `domain` IO-free, and it is why this
//! module adds no dependency to the crate.

use crate::{MemoryType, Scope};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Where a [`Scope`] resolves inside the kernel.
///
/// Only the scope mapper constructs this. That is deliberate: an unscoped kernel query returns
/// pages from *every* project, so making the resolved scope unforgeable is how cross-project
/// leakage is prevented by the type system rather than by remembering to pass a parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelScope {
    pub workspace: String,
    pub project: String,
    pub path_prefix: String,
}

/// Who wrote a page: Terminal AI, or an agent working in the same shared store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageAuthor {
    TerminalAi,
    Agent,
}

/// One memory page.
///
/// `id` is the kernel page path, so it stays a stable string key for the frontend — the wire type
/// is still called `MemoryEntry` there precisely so no component has to change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPage {
    pub id: String,
    pub scope: Scope,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub body: String,
    pub author: PageAuthor,
    pub created_at: String,
    pub updated_at: String,
}

/// A page that contributed to a composed context, shown in the pre-injection preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySource {
    pub entry_id: String,
    pub scope: Scope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffState {
    Open,
    Accepted,
    Expired,
}

/// A typed continuity record between agent sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KernelState {
    /// No binary could be resolved anywhere.
    NotInstalled,
    Probing,
    Starting,
    /// Running, and started by this app — the only state in which it may be stopped or restarted.
    Ready,
    /// Running, but started by someone else. Never terminated or restarted by this app.
    Attached,
    Degraded,
    /// Something is on the port, and it is not the kernel. Neither attach nor spawn over it.
    PortConflict,
    Failed,
}

impl KernelState {
    /// Whether memory operations can be attempted at all.
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Ready | Self::Attached)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelStatus {
    pub state: KernelState,
    /// The single gate on terminate and restart. False for a server we merely found running.
    pub owned: bool,
    pub server_url: String,
    pub data_dir: Option<String>,
    pub version: Option<String>,
    /// False when the running server is not the version this build pinned — surfaced instead of
    /// letting a moved response shape fail obscurely somewhere else.
    pub version_matches_pin: bool,
    pub has_token: bool,
    pub pages: Option<u64>,
    pub pending_migration: u64,
    pub hybrid_search: bool,
    pub last_checked_at: String,
    pub last_error: Option<String>,
    /// What the user can actually do about the current state.
    pub guidance: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum MemoryError {
    /// The kernel is not usable. Returned immediately, without attempting IO, so a dead or
    /// missing kernel can never make a command hang (Constitution VI).
    #[error("memory kernel unavailable ({0:?})")]
    Unavailable(KernelState),
    #[error("memory kernel rejected our credentials")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("invalid page path: {0}")]
    InvalidPath(String),
    /// The kernel answered, and said no.
    #[error("memory kernel error: {message}")]
    Upstream {
        code: Option<String>,
        message: String,
    },
    /// The kernel answered something we could not parse — usually a version moved a shape.
    #[error("unexpected response from memory kernel: {0}")]
    Protocol(String),
    #[error("cannot reach memory kernel: {0}")]
    Transport(String),
}

/// The memory host seam.
///
/// Note the absent method: there is no `begin_handoff`. Creating a handoff is an agent action at
/// the end of its own session; the app doing it on the user's behalf would fabricate a record of
/// work it did not do.
#[async_trait]
pub trait MemoryKernel: Send + Sync {
    /// Reads a cached snapshot. MUST NOT perform IO and cannot fail — this is what lets the UI
    /// render a kernel's absence instead of waiting on it.
    async fn status(&self) -> KernelStatus;

    async fn list(&self, scope: &Scope, limit: usize) -> Result<Vec<MemoryPage>, MemoryError>;

    /// `scope` is not optional, and that is the point: an unscoped query against the kernel
    /// returns pages from every project.
    async fn search(
        &self,
        query: &str,
        scope: &Scope,
        limit: usize,
    ) -> Result<Vec<MemoryPage>, MemoryError>;

    async fn read(&self, scope: &Scope, path: &str) -> Result<MemoryPage, MemoryError>;

    async fn write(
        &self,
        scope: &Scope,
        memory_type: MemoryType,
        title: &str,
        body: &str,
    ) -> Result<String, MemoryError>;

    async fn update(
        &self,
        scope: &Scope,
        path: &str,
        title: Option<&str>,
        body: &str,
    ) -> Result<(), MemoryError>;

    async fn delete(&self, scope: &Scope, path: &str) -> Result<(), MemoryError>;

    async fn compose_context(
        &self,
        scope: &Scope,
        max_bytes: usize,
    ) -> Result<(String, Vec<MemorySource>), MemoryError>;

    async fn briefing(&self, scope: &Scope) -> Result<String, MemoryError>;

    async fn handoffs(
        &self,
        scope: &Scope,
        state: Option<HandoffState>,
    ) -> Result<Vec<Handoff>, MemoryError>;

    /// Clear handoffs that have gone stale.
    ///
    /// There is no `accept_handoff` on purpose: a handoff is consumed by the next agent at session
    /// start, so an app that accepted one would silently take the context away from the agent that
    /// was about to receive it.
    async fn expire_handoffs(
        &self,
        scope: &Scope,
        older_than_days: u32,
    ) -> Result<u32, MemoryError>;
}
