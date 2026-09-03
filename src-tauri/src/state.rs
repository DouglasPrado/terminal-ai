use crate::host::{InProcessHost, SessionExit};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use terminal_ai_persistence::Database;
use terminal_ai_platform_macos::ResolvedEnvironment;
use terminal_ai_usage_core::{
    adapters::{AnthropicAdapter, CodexAdapter, OpenRouterAdapter},
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
    pub memory_root: PathBuf,
}
impl AppState {
    pub fn new(
        database: Database,
        cache_dir: PathBuf,
        skills_root: PathBuf,
        memory_root: PathBuf,
        exit_tx: UnboundedSender<SessionExit>,
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
            Arc::new(OpenRouterAdapter::new(client)),
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
        }
    }
}
