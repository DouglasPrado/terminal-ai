//! The supervisor's IO half: process lifetime, the health loop, and the cached status snapshot.
//!
//! All the *decisions* live in [`crate::supervisor`] as a pure function. This module only performs
//! what that function asks for, which is why the ownership rule ("never terminate a process we did
//! not start") is enforced in one tested place instead of at every call site.

use crate::cli::{KernelCli, KernelConfig, PINNED_VERSION};
use crate::probe::{probe, ProbeOutcome};
use crate::supervisor::{backoff_for, transition, Action, Event, Machine, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use terminal_ai_domain::memory::{KernelState, KernelStatus};

/// How often liveness is checked.
///
/// Constitution IV's ≥300s floor is scoped by its own wording to *usage* polling, where the remote
/// endpoints rate-limit aggressively. This is a loopback check against a child process, with no
/// quota and no remote party; at 300s the UI would keep claiming a dead kernel was healthy for five
/// minutes, which is exactly what SC-013 forbids. What Principle IV actually requires — one poller,
/// one cached snapshot, never per-view — is honoured.
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(15);

const STARTUP_BUDGET: Duration = Duration::from_secs(15);

/// Written when we spawn, read on the next boot. Without it, an app crash orphans a kernel and the
/// next launch starts a second one on a port the first still holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PidFile {
    pid: u32,
    server_url: String,
    started_at: String,
}

/// Everything the supervisor needs that it cannot work out for itself.
pub struct SupervisorOptions {
    pub config: Option<KernelConfig>,
    pub runtime_dir: PathBuf,
    /// Legacy entries not yet imported. Shown so the panel can offer the migration; the app never
    /// runs it on its own.
    pub pending_migration: u64,
}

pub struct Supervisor {
    machine: RwLock<Machine>,
    status: RwLock<KernelStatus>,
    cli: Option<Arc<KernelCli>>,
    runtime_dir: PathBuf,
    child: tokio::sync::Mutex<Option<tokio::process::Child>>,
}

impl Supervisor {
    #[must_use]
    pub fn new(options: SupervisorOptions) -> Self {
        let server_url = options.config.as_ref().map_or_else(
            || "http://127.0.0.1:49374".to_owned(),
            |c| c.server_url.clone(),
        );
        let installed = options.config.is_some();
        let hybrid = options.config.as_ref().is_some_and(|c| c.hybrid_search);
        let has_token = options.config.as_ref().is_some_and(|c| c.token.is_some());

        let machine = Machine {
            state: if installed {
                State::Idle
            } else {
                State::NotInstalled
            },
            ..Machine::default()
        };

        Self {
            machine: RwLock::new(machine),
            status: RwLock::new(KernelStatus {
                state: if installed {
                    KernelState::Probing
                } else {
                    KernelState::NotInstalled
                },
                owned: false,
                server_url,
                data_dir: None,
                version: None,
                version_matches_pin: true,
                has_token,
                pages: None,
                pending_migration: options.pending_migration,
                hybrid_search: hybrid,
                last_checked_at: Utc::now().to_rfc3339(),
                last_error: None,
                guidance: if installed {
                    None
                } else {
                    Some(NOT_INSTALLED_GUIDANCE.to_owned())
                },
            }),
            cli: options.config.map(|c| Arc::new(KernelCli::new(c))),
            runtime_dir: options.runtime_dir,
            child: tokio::sync::Mutex::new(None),
        }
    }

    /// The cached snapshot. Never performs IO and never fails — that is what lets a command answer
    /// instantly when the kernel is gone, instead of the UI waiting on a dead socket.
    #[must_use]
    pub fn status(&self) -> KernelStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    #[must_use]
    pub fn cli(&self) -> Option<Arc<KernelCli>> {
        self.cli.clone()
    }

    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.status().state.is_usable()
    }

    fn machine(&self) -> Machine {
        self.machine
            .read()
            .map(|m| *m)
            .unwrap_or_else(|p| *p.into_inner())
    }

    fn set_machine(&self, machine: Machine) {
        match self.machine.write() {
            Ok(mut guard) => *guard = machine,
            Err(poisoned) => *poisoned.into_inner() = machine,
        }
    }

    fn update_status(&self, apply: impl FnOnce(&mut KernelStatus)) {
        let mut guard = match self.status.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        apply(&mut guard);
        guard.last_checked_at = Utc::now().to_rfc3339();
    }

    /// One turn of the supervisor: feed an event to the machine, perform what it asks for.
    pub async fn handle(&self, event: Event) {
        let (next, actions) = transition(self.machine(), event);
        self.set_machine(next);
        self.sync_status(next);

        for action in actions {
            match action {
                Action::Probe => Box::pin(self.do_probe()).await,
                Action::Spawn => Box::pin(self.do_spawn()).await,
                Action::Terminate => self.do_terminate().await,
                Action::ScheduleBackoff(_) => {}
                Action::ReportQuarantine => {
                    self.update_status(|s| {
                        s.guidance = Some(QUARANTINE_GUIDANCE.to_owned());
                    });
                }
            }
        }
    }

    fn sync_status(&self, machine: Machine) {
        let state = match machine.state {
            State::Idle | State::Probing => KernelState::Probing,
            State::NotInstalled => KernelState::NotInstalled,
            State::Starting => KernelState::Starting,
            State::Ready => KernelState::Ready,
            State::Attached => KernelState::Attached,
            State::Degraded => KernelState::Degraded,
            State::PortConflict => KernelState::PortConflict,
            State::Failed => KernelState::Failed,
        };
        self.update_status(|s| {
            s.state = state;
            s.owned = machine.owned;
            if state.is_usable() {
                s.last_error = None;
            }
            s.guidance = guidance_for(state);
        });
    }

    async fn do_probe(&self) {
        let Some(cli) = self.cli.as_ref() else {
            Box::pin(self.handle(Event::BinaryMissing)).await;
            return;
        };
        let config = cli.config();
        let outcome = probe(&config.server_url, config.token.as_ref()).await;

        let event = match outcome {
            ProbeOutcome::Kernel => {
                self.refresh_details().await;
                // Ours if we hold the child handle, or if the pidfile says we started it and
                // that process is still alive (an app crash left it orphaned). Anything else
                // belongs to the user and is never managed.
                let ours = self.child.lock().await.is_some() || self.adopt_orphan().await;
                if ours {
                    Event::ProbeFoundOwnKernel
                } else {
                    Event::ProbeFoundForeignKernel
                }
            }
            ProbeOutcome::Unauthorized => {
                self.update_status(|s| {
                    s.last_error = Some("the memory kernel rejected our credentials".into());
                });
                Event::ProbeFoundForeignKernel
            }
            ProbeOutcome::Stranger(why) => {
                self.update_status(|s| {
                    s.last_error = Some(format!("something else is on this port: it {why}"));
                });
                Event::ProbeFoundStranger
            }
            ProbeOutcome::Refused => Event::ProbeRefused,
        };
        Box::pin(self.handle(event)).await;
    }

    /// A kernel we started before but lost the handle to — after an app crash, say. Adopting it
    /// beats starting a second server against the same store.
    async fn adopt_orphan(&self) -> bool {
        let Some(pidfile) = self.read_pidfile() else {
            return false;
        };
        let Some(cli) = self.cli.as_ref() else {
            return false;
        };
        if pidfile.server_url != cli.config().server_url {
            return false;
        }
        process_is_alive(pidfile.pid)
    }

    async fn refresh_details(&self) {
        let Some(cli) = self.cli.as_ref() else { return };
        if let Ok(report) = cli.status().await {
            let matches_pin = report.version == PINNED_VERSION;
            self.update_status(|s| {
                s.version = Some(report.version.clone());
                s.version_matches_pin = matches_pin;
                s.data_dir = report.data_dir.clone();
                s.pages = report.counts.as_ref().map(|c| c.pages_latest);
            });
        }
    }

    async fn do_spawn(&self) {
        let Some(cli) = self.cli.as_ref() else {
            Box::pin(self.handle(Event::BinaryMissing)).await;
            return;
        };
        let mut command = cli.serve_command();
        command.kill_on_drop(false);

        match command.spawn() {
            Ok(child) => {
                if let Some(pid) = child.id() {
                    self.write_pidfile(pid, &cli.config().server_url);
                }
                *self.child.lock().await = Some(child);
                Box::pin(self.handle(Event::SpawnSucceeded)).await;
                Box::pin(self.await_ready()).await;
            }
            Err(err) => {
                self.update_status(|s| s.last_error = Some(format!("could not start: {err}")));
                Box::pin(self.handle(Event::SpawnFailed)).await;
            }
        }
    }

    /// Poll until the kernel answers or the budget runs out.
    async fn await_ready(&self) {
        let Some(cli) = self.cli.as_ref() else { return };
        let config = cli.config();
        let deadline = std::time::Instant::now() + STARTUP_BUDGET;
        let mut attempt = 0u32;

        loop {
            // A child that died immediately is almost always Gatekeeper quarantine, which no
            // amount of retrying fixes — say so now rather than after five backoffs.
            if let Some(child) = self.child.lock().await.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let killed = status.code().is_none() || status.code() == Some(137);
                    Box::pin(self.handle(if killed {
                        Event::SpawnKilledBySignal
                    } else {
                        Event::SpawnFailed
                    }))
                    .await;
                    return;
                }
            }

            if probe(&config.server_url, config.token.as_ref()).await == ProbeOutcome::Kernel {
                self.refresh_details().await;
                Box::pin(self.handle(Event::ReadyConfirmed)).await;
                return;
            }

            if std::time::Instant::now() >= deadline {
                Box::pin(self.handle(Event::StartupTimedOut)).await;
                return;
            }
            attempt = attempt.saturating_add(1);
            tokio::time::sleep(Duration::from_millis(200).max(backoff_for(attempt) / 8)).await;
        }
    }

    /// Only ever reached through [`Action::Terminate`], which the pure machine emits exclusively
    /// for a process we started.
    async fn do_terminate(&self) {
        let mut guard = self.child.lock().await;
        let Some(mut child) = guard.take() else {
            return;
        };
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
        drop(guard);
        self.clear_pidfile();
    }

    fn pidfile_path(&self) -> PathBuf {
        self.runtime_dir.join("ai-memory.json")
    }

    fn write_pidfile(&self, pid: u32, server_url: &str) {
        let _ = std::fs::create_dir_all(&self.runtime_dir);
        let record = PidFile {
            pid,
            server_url: server_url.to_owned(),
            started_at: Utc::now().to_rfc3339(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&record) {
            let _ = std::fs::write(self.pidfile_path(), json);
        }
    }

    fn read_pidfile(&self) -> Option<PidFile> {
        let raw = std::fs::read_to_string(self.pidfile_path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn clear_pidfile(&self) {
        let _ = std::fs::remove_file(self.pidfile_path());
    }

    /// The one health loop. Every view reads [`Supervisor::status`]; nothing else polls.
    pub async fn run_health_loop(self: Arc<Self>) {
        loop {
            tokio::time::sleep(HEALTH_INTERVAL).await;
            let machine = self.machine();
            match machine.state {
                State::Ready | State::Attached => {
                    let Some(cli) = self.cli.as_ref() else {
                        continue;
                    };
                    let config = cli.config();
                    if probe(&config.server_url, config.token.as_ref()).await
                        == ProbeOutcome::Kernel
                    {
                        self.refresh_details().await;
                    } else {
                        self.handle(Event::HealthCheckFailed).await;
                    }
                }
                State::Degraded => self.handle(Event::BackoffElapsed).await,
                // NotInstalled, PortConflict and Failed are terminal until the user acts: retrying
                // them on a timer would just churn.
                _ => {}
            }
        }
    }
}

fn process_is_alive(pid: u32) -> bool {
    // Signal 0 checks for existence without delivering anything.
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .output()
        .is_ok_and(|o| o.status.success())
}

const NOT_INSTALLED_GUIDANCE: &str =
    "The memory kernel binary was not found. Reinstall Terminal AI, or point Settings at an \
     existing ai-memory binary.";

const QUARANTINE_GUIDANCE: &str =
    "The memory kernel was blocked from starting, which usually means macOS quarantined it. \
     Clear it with: xattr -d com.apple.quarantine <path to ai-memory>";

fn guidance_for(state: KernelState) -> Option<String> {
    match state {
        KernelState::NotInstalled => Some(NOT_INSTALLED_GUIDANCE.to_owned()),
        KernelState::PortConflict => Some(
            "Another program is using the memory kernel's port. Stop it, or choose a different \
             port in Settings."
                .to_owned(),
        ),
        KernelState::Failed => Some(
            "The memory kernel could not be started. Check the logs, then try again from Settings."
                .to_owned(),
        ),
        KernelState::Attached => Some(
            "Using a memory server that was already running. Terminal AI will not stop or restart \
             it."
            .to_owned(),
        ),
        _ => None,
    }
}

/// Resolve the path where the kernel's runtime bookkeeping lives.
#[must_use]
pub fn runtime_dir(app_root: &Path) -> PathBuf {
    app_root.join("runtime")
}
