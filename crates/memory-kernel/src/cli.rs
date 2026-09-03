//! The write side, and process control: invoking the `ai-memory` binary.
//!
//! Writes do not go over `/api/v1` because that surface is read-only by construction upstream.
//! They go through the binary, which is itself a full HTTP client of the same server. Every
//! invocation is built as a fixed argv — never a shell string — which is the same discipline
//! `project-manager` already uses for `git clone`, and is what keeps Constitution I intact: the
//! frontend never composes an argument.

use crate::token::AuthToken;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use terminal_ai_domain::memory::{KernelScope, MemoryError};
use tokio::process::Command;

/// The version this build was pinned to. A running server that disagrees is reported rather than
/// trusted, because response shapes are an external contract we observe, not one we own.
pub const PINNED_VERSION: &str = "2.0.2";

/// Startup environment for the kernel.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub binary: PathBuf,
    pub server_url: String,
    pub bind: String,
    /// `None` means "use ai-memory's own default location", which is what makes the store shared
    /// with whatever ai-memory the user runs outside the app. That sharing is the point of the
    /// feature, so this is `None` in production and `Some` only in tests.
    pub data_dir: Option<PathBuf>,
    pub token: Option<AuthToken>,
    /// When false the kernel starts with embeddings disabled, so its first run performs no
    /// unannounced ~87 MB model download.
    pub hybrid_search: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusCounts {
    #[serde(default)]
    pub pages_latest: u64,
    #[serde(default)]
    pub sessions: u64,
    #[serde(default)]
    pub observations: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusReport {
    pub version: String,
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub counts: Option<StatusCounts>,
    #[serde(default)]
    pub capture_mode: Option<String>,
}

/// A `write-page` result, parsed from the CLI's confirmation line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    pub path: String,
}

pub struct KernelCli {
    config: KernelConfig,
}

impl KernelCli {
    #[must_use]
    pub const fn new(config: KernelConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub const fn config(&self) -> &KernelConfig {
        &self.config
    }

    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.config.binary);
        if let Some(dir) = &self.config.data_dir {
            cmd.arg("--data-dir").arg(dir);
        }
        cmd.env("AI_MEMORY_SERVER_URL", &self.config.server_url);
        if let Some(token) = &self.config.token {
            // Environment, never argv: a command line is visible to any `ps` on the machine.
            cmd.env("AI_MEMORY_AUTH_TOKEN", token.expose());
        }
        cmd.stdin(Stdio::null());
        cmd
    }

    /// The argv for `serve`. Returned rather than run so the supervisor owns process lifetime.
    #[must_use]
    pub fn serve_command(&self) -> Command {
        let mut cmd = self.base_command();
        cmd.args([
            "serve",
            "--transport",
            "http",
            "--bind",
            &self.config.bind,
            // Required: without it every /api/v1 route answers 404, and the read client is the
            // whole read path.
            "--enable-web",
        ]);
        if !self.config.hybrid_search {
            // Verified opt-out: with this set the kernel fetches no model and `models/` stays
            // empty, while full-text, entity and graph ranking keep working.
            cmd.env("AI_MEMORY_EMBEDDING_PROVIDER", "none");
        }
        cmd
    }

    /// Run an arbitrary kernel subcommand, built from a fixed argv the caller assembled.
    ///
    /// Public so the wiring layer can reuse the same error mapping and environment; still never a
    /// shell string, and never anything the frontend composed.
    pub async fn run_raw(&self, args: &[&str]) -> Result<String, MemoryError> {
        self.run(args).await
    }

    async fn run(&self, args: &[&str]) -> Result<String, MemoryError> {
        let output = self
            .base_command()
            .args(args)
            .output()
            .await
            .map_err(|e| MemoryError::Transport(format!("could not run ai-memory: {e}")))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        // The binary prefixes real failures with `Error:`; anything else is log noise.
        let message = stderr
            .lines()
            .find_map(|line| line.strip_prefix("Error: "))
            .map_or_else(|| stderr.trim().to_owned(), str::to_owned);
        Err(MemoryError::Upstream {
            code: output.status.code().map(|c| c.to_string()),
            message: if message.is_empty() {
                "ai-memory failed with no message".to_owned()
            } else {
                message
            },
        })
    }

    /// Health and identity in one call. Used by the supervisor's readiness check.
    pub async fn status(&self) -> Result<StatusReport, MemoryError> {
        let out = self.run(&["status", "--json"]).await?;
        // `status` writes tracing lines to stderr, so stdout is clean JSON — but be defensive and
        // start at the first brace anyway.
        let json = out.find('{').map_or(out.as_str(), |i| &out[i..]);
        serde_json::from_str(json).map_err(|e| {
            MemoryError::Protocol(format!("could not read `ai-memory status --json`: {e}"))
        })
    }

    /// Create or replace a page. Idempotent for a given path: the kernel versions pages, so a
    /// repeat write supersedes rather than duplicating.
    pub async fn write_page(
        &self,
        scope: &KernelScope,
        path: &str,
        title: &str,
        kind: &str,
        body: &str,
    ) -> Result<WriteOutcome, MemoryError> {
        crate::scope::validate_path(path)?;
        self.run(&[
            "write-page",
            "--path",
            path,
            "--body",
            body,
            "--title",
            title,
            "--kind",
            kind,
            "--tag",
            "terminal-ai",
            "--workspace",
            &scope.workspace,
            "--project",
            &scope.project,
        ])
        .await?;
        Ok(WriteOutcome {
            path: path.to_owned(),
        })
    }

    /// Expire handoffs older than a number of days.
    ///
    /// The kernel only offers bulk expiry, not per-id cancellation, so this is what "clear the
    /// stale ones" actually means.
    pub async fn expire_handoffs(
        &self,
        scope: &KernelScope,
        older_than_days: u32,
    ) -> Result<(), MemoryError> {
        let days = older_than_days.max(1).to_string();
        self.run(&[
            "handoffs",
            "--workspace",
            &scope.workspace,
            "--project",
            &scope.project,
            "--older-than-days",
            &days,
            "--confirm",
        ])
        .await
        .map(|_| ())
    }

    pub async fn delete_page(&self, scope: &KernelScope, path: &str) -> Result<(), MemoryError> {
        crate::scope::validate_path(path)?;
        // Note: the kernel reports `deleted: true` even for a page that never existed, so the
        // response cannot be used to tell "removed" from "was not there".
        self.run(&[
            "delete-page",
            "--path",
            path,
            "--workspace",
            &scope.workspace,
            "--project",
            &scope.project,
        ])
        .await
        .map(|_| ())
    }
}

/// Resolve the kernel binary: the bundled sidecar first, then an explicit override, then `PATH`.
///
/// Tauri copies an `externalBin` next to the app executable, so the sidecar is found without the
/// shell plugin — which matters, because that plugin is exactly what Constitution I keeps away
/// from the WebView.
#[must_use]
pub fn resolve_binary(configured: Option<&Path>, path_lookup: Option<&Path>) -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in [
                "ai-memory",
                concat!("ai-memory-", env!("TARGET_TRIPLE_HINT")),
            ] {
                let candidate = dir.join(name);
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    if let Some(configured) = configured.filter(|p| is_executable(p)) {
        return Some(configured.to_path_buf());
    }
    path_lookup
        .filter(|p| is_executable(p))
        .map(Path::to_path_buf)
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
}
