//! macOS environment and app-data integration.
#![forbid(unsafe_code)]

use directories::BaseDirs;
use serde::Serialize;
use std::{
    collections::BTreeMap, ffi::OsString, path::PathBuf, process::Command, time::SystemTime,
};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub config: PathBuf,
    pub skills: PathBuf,
    pub memory: PathBuf,
    pub sessions: PathBuf,
    pub logs: PathBuf,
    pub cache: PathBuf,
}

impl AppPaths {
    pub fn bootstrap() -> Result<Self, PlatformError> {
        let base = BaseDirs::new().ok_or(PlatformError::NoHome)?;
        let root = base
            .home_dir()
            .join("Library/Application Support/AITerminal");
        let paths = Self {
            database: root.join("app.db"),
            config: root.join("config.toml"),
            skills: root.join("skills"),
            memory: root.join("memory"),
            sessions: root.join("sessions"),
            logs: root.join("logs"),
            cache: root.join("cache"),
            root,
        };
        for dir in [
            &paths.root,
            &paths.skills,
            &paths.memory,
            &paths.sessions,
            &paths.logs,
            &paths.cache,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(paths)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEnvironment {
    pub path: String,
    pub env: BTreeMap<String, String>,
    pub cached_at: String,
}

pub fn resolve_login_shell_env() -> Result<ResolvedEnvironment, PlatformError> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/zsh"));
    let output = Command::new(shell)
        .args(["-l", "-c", "/usr/bin/env -0"])
        .output()?;
    if !output.status.success() {
        return Err(PlatformError::Shell(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let allowed = [
        "PATH",
        "HOME",
        "USER",
        "SHELL",
        "LANG",
        "LC_ALL",
        "TERM",
        "COLORTERM",
        "TMPDIR",
    ];
    let mut env = BTreeMap::new();
    for entry in output.stdout.split(|byte| *byte == 0) {
        if let Some(index) = entry.iter().position(|byte| *byte == b'=') {
            let key = String::from_utf8_lossy(&entry[..index]);
            if allowed.contains(&key.as_ref()) {
                env.insert(
                    key.into_owned(),
                    String::from_utf8_lossy(&entry[index + 1..]).into_owned(),
                );
            }
        }
    }
    let mut path = env.get("PATH").cloned().unwrap_or_default();
    let home = BaseDirs::new()
        .ok_or(PlatformError::NoHome)?
        .home_dir()
        .to_path_buf();
    for candidate in [
        PathBuf::from("/opt/homebrew/bin"),
        home.join(".local/bin"),
        home.join(".cargo/bin"),
    ] {
        let value = candidate.to_string_lossy();
        if !path.split(':').any(|part| part == value) {
            if !path.is_empty() {
                path.push(':');
            }
            path.push_str(&value);
        }
    }
    env.insert("PATH".into(), path.clone());
    let cached_at = format!("{:?}", SystemTime::now());
    Ok(ResolvedEnvironment {
        path,
        env,
        cached_at,
    })
}

pub fn init_logging(
    paths: &AppPaths,
) -> Result<tracing_appender::non_blocking::WorkerGuard, PlatformError> {
    use tracing_subscriber::prelude::*;
    let file = tracing_appender::rolling::daily(&paths.logs, "terminal-ai.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(
                "terminal_ai=info"
                    .parse()
                    .map_err(|e| PlatformError::Logging(format!("{e}")))?,
            ),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .try_init()
        .map_err(|e| PlatformError::Logging(e.to_string()))?;
    Ok(guard)
}

pub fn notify(title: &str, body: &str) -> Result<(), PlatformError> {
    let status = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            body,
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Notification)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("home directory unavailable")]
    NoHome,
    #[error("login shell failed: {0}")]
    Shell(String),
    #[error("logging setup failed: {0}")]
    Logging(String),
    #[error("macOS notification delivery failed")]
    Notification,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
