use async_trait::async_trait;
use portable_pty::PtySize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
};
use terminal_ai_domain::{
    host::{
        HostError, HostSessionState, LaunchContext, OutputChunk, SessionHost, SessionInfo, Signal,
    },
    SessionId,
};
use terminal_ai_platform_macos::ResolvedEnvironment;
use terminal_ai_provider_runtime::{builtin_providers, AgentProvider, ProviderAdapter};
use terminal_ai_pty_runtime::PtyProcess;
use tokio::sync::mpsc;

/// Emitted when a PTY process exits on its own (i.e. not via an explicit `close`/`restart`).
/// Consumed in `lib.rs` to emit `ProcessExited` and finalize the session's DB row.
#[derive(Debug, Clone)]
pub struct SessionExit {
    pub id: String,
    pub exit_code: Option<i32>,
}
struct LiveSession {
    process: Arc<PtyProcess>,
    info: SessionInfo,
    context: LaunchContext,
    output: mpsc::Sender<OutputChunk>,
    scrollback: Arc<Mutex<VecDeque<u8>>>,
}
pub struct InProcessHost {
    sessions: Arc<Mutex<HashMap<String, LiveSession>>>,
    providers: RwLock<BTreeMap<String, ProviderAdapter>>,
    environment: Arc<RwLock<Option<ResolvedEnvironment>>>,
    exit_tx: mpsc::UnboundedSender<SessionExit>,
}

impl InProcessHost {
    pub fn new(
        environment: Arc<RwLock<Option<ResolvedEnvironment>>>,
        exit_tx: mpsc::UnboundedSender<SessionExit>,
    ) -> Self {
        let providers = builtin_providers()
            .into_iter()
            .map(|provider| (provider.profile().id.0.clone(), provider))
            .collect();
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            providers: RwLock::new(providers),
            environment,
            exit_tx,
        }
    }
    fn env(&self) -> Result<BTreeMap<String, String>, HostError> {
        let mut env = if let Some(resolved) = self
            .environment
            .read()
            .map_err(|_| HostError::Transport("environment cache poisoned".into()))?
            .as_ref()
        {
            resolved.env.clone()
        } else {
            std::env::vars().collect()
        };
        // The front-end terminal is xterm.js (256-color / truecolor). Force TERM/COLORTERM so agent
        // CLIs and shell programs emit ANSI colors even when the resolved login-shell env — captured
        // from a GUI launch with no controlling terminal — lacks or mis-sets them.
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        Ok(env)
    }
    async fn spawn(
        &self,
        context: LaunchContext,
        output: mpsc::Sender<OutputChunk>,
    ) -> Result<SessionInfo, HostError> {
        if !(1..=1000).contains(&context.cols) || !(1..=1000).contains(&context.rows) {
            return Err(HostError::Validation(
                "terminal dimensions must be 1..1000".into(),
            ));
        }
        let provider = self
            .providers
            .read()
            .map_err(|_| HostError::Transport("provider registry poisoned".into()))?
            .get(&context.provider_id.0)
            .cloned()
            .ok_or_else(|| HostError::ProviderMissing(context.provider_id.0.clone()))?;
        let spec = provider
            .command(&context, &self.env()?)
            .await
            .map_err(|error| HostError::Spawn(error.to_string()))?;
        let id = SessionId::new();
        let scrollback = Arc::new(Mutex::new(VecDeque::with_capacity(1_000_000)));
        let scrollback_writer = Arc::clone(&scrollback);
        let out = output.clone();
        let seq = Arc::new(AtomicU64::new(0));
        let sequence = Arc::clone(&seq);
        let session_id = id.0.clone();
        let sessions_for_exit = Arc::clone(&self.sessions);
        let exit_tx = self.exit_tx.clone();
        let exit_id = id.0.clone();
        let process = PtyProcess::spawn(
            &spec,
            PtySize {
                rows: context.rows,
                cols: context.cols,
                pixel_width: 0,
                pixel_height: 0,
            },
            move |bytes| {
                if let Ok(mut buffer) = scrollback_writer.lock() {
                    buffer.extend(&bytes);
                    while buffer.len() > 1_000_000 {
                        buffer.pop_front();
                    }
                }
                let _ = out.blocking_send(OutputChunk {
                    seq: sequence.fetch_add(1, Ordering::Relaxed),
                    bytes,
                });
            },
            move |code| {
                match sessions_for_exit.lock() {
                    Ok(mut map) => match map.get_mut(&exit_id) {
                        Some(session) => session.info.state = HostSessionState::Exited(code),
                        // Already removed by an explicit close/restart — do not double-report.
                        None => return,
                    },
                    Err(_) => return,
                }
                let _ = exit_tx.send(SessionExit {
                    id: exit_id.clone(),
                    exit_code: code,
                });
            },
        )
        .map_err(|error| HostError::Spawn(error.to_string()))?;
        let info = SessionInfo {
            id: id.clone(),
            provider_id: context.provider_id.clone(),
            project_id: context.project_id.clone(),
            worktree_id: context.worktree_id.clone(),
            pid: process.pid(),
            title: provider.profile().label.clone(),
            state: HostSessionState::Running,
        };
        self.sessions
            .lock()
            .map_err(|_| HostError::Transport("session map poisoned".into()))?
            .insert(
                session_id,
                LiveSession {
                    process: Arc::new(process),
                    info: info.clone(),
                    context,
                    output,
                    scrollback,
                },
            );
        Ok(info)
    }

    pub fn register_provider(&self, provider: ProviderAdapter) -> Result<(), HostError> {
        self.providers
            .write()
            .map_err(|_| HostError::Transport("provider registry poisoned".into()))?
            .insert(provider.profile().id.0.clone(), provider);
        Ok(())
    }
}

#[async_trait]
impl SessionHost for InProcessHost {
    async fn create(
        &self,
        ctx: LaunchContext,
        out: mpsc::Sender<OutputChunk>,
    ) -> Result<SessionInfo, HostError> {
        self.spawn(ctx, out).await
    }
    async fn write(&self, id: &SessionId, data: &[u8]) -> Result<(), HostError> {
        if data.len() > 1_048_576 {
            return Err(HostError::Validation("input exceeds 1 MiB".into()));
        }
        let process = {
            Arc::clone(
                &self
                    .sessions
                    .lock()
                    .map_err(|_| HostError::Transport("session map poisoned".into()))?
                    .get(&id.0)
                    .ok_or(HostError::NotFound)?
                    .process,
            )
        };
        process
            .write(data)
            .map_err(|e| HostError::Transport(e.to_string()))
    }
    async fn resize(&self, id: &SessionId, cols: u16, rows: u16) -> Result<(), HostError> {
        let process = {
            Arc::clone(
                &self
                    .sessions
                    .lock()
                    .map_err(|_| HostError::Transport("session map poisoned".into()))?
                    .get(&id.0)
                    .ok_or(HostError::NotFound)?
                    .process,
            )
        };
        process
            .resize(cols, rows)
            .map_err(|e| HostError::Transport(e.to_string()))
    }
    async fn signal(&self, id: &SessionId, sig: Signal) -> Result<(), HostError> {
        let process = {
            Arc::clone(
                &self
                    .sessions
                    .lock()
                    .map_err(|_| HostError::Transport("session map poisoned".into()))?
                    .get(&id.0)
                    .ok_or(HostError::NotFound)?
                    .process,
            )
        };
        process
            .signal(sig)
            .map_err(|e| HostError::Transport(e.to_string()))
    }
    async fn close(&self, id: &SessionId) -> Result<Option<i32>, HostError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| HostError::Transport("session map poisoned".into()))?
            .remove(&id.0)
            .ok_or(HostError::NotFound)?;
        session
            .process
            .close()
            .map_err(|e| HostError::Transport(e.to_string()))
    }
    async fn restart(&self, id: &SessionId) -> Result<SessionInfo, HostError> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| HostError::Transport("session map poisoned".into()))?
            .remove(&id.0)
            .ok_or(HostError::NotFound)?;
        let _ = session.process.close();
        let mut context = session.context;
        context.resume = None;
        self.spawn(context, session.output).await
    }
    async fn list(&self) -> Result<Vec<SessionInfo>, HostError> {
        Ok(self
            .sessions
            .lock()
            .map_err(|_| HostError::Transport("session map poisoned".into()))?
            .values()
            .map(|session| session.info.clone())
            .collect())
    }
    async fn scrollback(
        &self,
        id: &SessionId,
        max_bytes: usize,
    ) -> Result<(Vec<u8>, bool), HostError> {
        let buffer = Arc::clone(
            &self
                .sessions
                .lock()
                .map_err(|_| HostError::Transport("session map poisoned".into()))?
                .get(&id.0)
                .ok_or(HostError::NotFound)?
                .scrollback,
        );
        let buffer = buffer
            .lock()
            .map_err(|_| HostError::Transport("scrollback poisoned".into()))?;
        let count = max_bytes.min(1_000_000).min(buffer.len());
        Ok((
            buffer.iter().skip(buffer.len() - count).copied().collect(),
            count < buffer.len(),
        ))
    }
    async fn resume(
        &self,
        ctx: LaunchContext,
        out: mpsc::Sender<OutputChunk>,
    ) -> Result<(SessionInfo, bool), HostError> {
        let resumed = ctx.resume.is_some();
        self.spawn(ctx, out).await.map(|info| (info, resumed))
    }
}
