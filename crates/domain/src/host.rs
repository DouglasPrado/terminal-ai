use crate::{ProjectId, ProviderId, SessionId, WorktreeId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchContext {
    pub project_id: Option<ProjectId>,
    pub worktree_id: Option<WorktreeId>,
    pub provider_id: ProviderId,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub resume: Option<ResumeRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "ref", rename_all = "camelCase")]
pub enum ResumeRef {
    Continue,
    ById(String),
}
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeCapability {
    Continue,
    ResumeById,
    None,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum HostSessionState {
    Starting,
    Running,
    Exited(Option<i32>),
    Error(String),
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub provider_id: ProviderId,
    pub project_id: Option<ProjectId>,
    pub worktree_id: Option<WorktreeId>,
    pub pid: u32,
    pub title: String,
    pub state: HostSessionState,
}
#[derive(Debug, Clone)]
pub struct OutputChunk {
    pub seq: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Copy)]
pub enum Signal {
    Int,
    Term,
    Kill,
    Hup,
}

#[async_trait]
pub trait SessionHost: Send + Sync {
    async fn create(
        &self,
        ctx: LaunchContext,
        out: mpsc::Sender<OutputChunk>,
    ) -> Result<SessionInfo, HostError>;
    async fn write(&self, id: &SessionId, data: &[u8]) -> Result<(), HostError>;
    async fn resize(&self, id: &SessionId, cols: u16, rows: u16) -> Result<(), HostError>;
    async fn signal(&self, id: &SessionId, sig: Signal) -> Result<(), HostError>;
    async fn close(&self, id: &SessionId) -> Result<Option<i32>, HostError>;
    async fn restart(&self, id: &SessionId) -> Result<SessionInfo, HostError>;
    async fn list(&self) -> Result<Vec<SessionInfo>, HostError>;
    async fn scrollback(
        &self,
        id: &SessionId,
        max_bytes: usize,
    ) -> Result<(Vec<u8>, bool), HostError>;
    async fn resume(
        &self,
        ctx: LaunchContext,
        out: mpsc::Sender<OutputChunk>,
    ) -> Result<(SessionInfo, bool), HostError>;
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("session not found")]
    NotFound,
    #[error("provider not detected: {0}")]
    ProviderMissing(String),
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("host transport error: {0}")]
    Transport(String),
    #[error("validation failed: {0}")]
    Validation(String),
}
