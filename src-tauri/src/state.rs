use crate::host::{InProcessHost, SessionExit};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use terminal_ai_domain::invisible_mode::DockCooldown;
use terminal_ai_memory_kernel::runtime::Supervisor;
use terminal_ai_persistence::Database;
use terminal_ai_platform_macos::ResolvedEnvironment;
use terminal_ai_usage_core::{
    adapters::{AnthropicAdapter, CodexAdapter, OpenCodeAdapter},
    poller::UsagePoller,
    UsageAdapter,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct AppState {
    pub database: Database,
    pub environment: Arc<RwLock<Option<ResolvedEnvironment>>>,
    pub host: Arc<InProcessHost>,
    pub usage: Arc<UsagePoller>,
    pub skills_root: PathBuf,
    /// The legacy memory directory. Memory content now lives in the kernel's wiki; this stays
    /// because it is the migration's source and the rollback path — the legacy markdown files are
    /// deliberately not deleted (see specs/002-ai-memory-kernel/data-model.md).
    #[allow(dead_code, reason = "read by the legacy import in US9")]
    pub memory_root: PathBuf,
    /// The one memory-kernel supervisor. Every view reads its cached snapshot; nothing else polls
    /// the kernel (Constitution IV), and only this object may stop a process the app started
    /// (Constitution VII).
    pub kernel: Arc<Supervisor>,
    /// The app's data root. Wiring backups live under it, so a removal can restore a file the app
    /// merged into rather than clobbering the user's later edits.
    pub app_root: PathBuf,
    /// When the app last put itself back in the Dock. A dock hide that lands within a second of a
    /// show is silently dropped by tao (see `domain::invisible_mode`), so the invisible-mode
    /// adapter waits this out instead of reporting a hide that never happened.
    pub dock_cooldown: Mutex<DockCooldown>,
}
impl AppState {
    pub fn new(
        database: Database,
        cache_dir: PathBuf,
        skills_root: PathBuf,
        memory_root: PathBuf,
        exit_tx: UnboundedSender<SessionExit>,
        kernel: Arc<Supervisor>,
        app_root: PathBuf,
    ) -> Self {
        let environment = Arc::new(RwLock::new(None));
        let host = Arc::new(InProcessHost::new(Arc::clone(&environment), exit_tx));
        // Boot-time fail-fast: building the HTTP client only fails on a broken TLS backend,
        // which is unrecoverable at startup.
        #[allow(clippy::expect_used)]
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("usage HTTP client");
        let adapters: Vec<Arc<dyn UsageAdapter>> = vec![
            Arc::new(AnthropicAdapter::new(client.clone())),
            Arc::new(CodexAdapter::new(client.clone())),
            Arc::new(OpenCodeAdapter::new()),
        ];
        Self {
            database,
            environment,
            host,
            usage: Arc::new(UsagePoller::new(
                adapters,
                cache_dir.join("usage.json"),
                300,
            )),
            skills_root,
            memory_root,
            kernel,
            app_root,
            dock_cooldown: Mutex::new(DockCooldown::new()),
        }
    }
}
