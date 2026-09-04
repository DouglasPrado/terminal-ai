//! Composition root for the memory kernel: resolving scopes against the database, building the
//! supervisor, and the error mapping the command layer needs.
//!
//! The kernel crate deliberately knows nothing about `persistence`, so the database lookups a
//! scope needs live here — the same split the usage poller already uses, where the crate computes
//! and `lib.rs` writes the rows.

use crate::commands::AppError;
use std::path::{Path, PathBuf};
use terminal_ai_domain::memory::MemoryError;
use terminal_ai_domain::{Scope, ScopeLevel};
use terminal_ai_memory_kernel::cli::KernelConfig;
use terminal_ai_memory_kernel::kernel::ScopeDirectory;
use terminal_ai_memory_kernel::scope::{ProjectRef, ScopeInput};
use terminal_ai_persistence::Database;

impl From<MemoryError> for AppError {
    fn from(error: MemoryError) -> Self {
        let code = match &error {
            MemoryError::Unavailable(_) => "MEMORY_KERNEL_UNAVAILABLE",
            MemoryError::Unauthorized => "MEMORY_KERNEL_UNAUTHORIZED",
            MemoryError::NotFound => "MEMORY_NOT_FOUND",
            MemoryError::InvalidScope(_) => "INVALID_SCOPE",
            MemoryError::InvalidPath(_) => "MEMORY_INVALID_PATH",
            MemoryError::Upstream { .. } => "MEMORY_KERNEL_UPSTREAM",
            MemoryError::Protocol(_) => "MEMORY_KERNEL_PROTOCOL",
            MemoryError::Transport(_) => "MEMORY_KERNEL_TRANSPORT",
        };
        Self {
            code: code.into(),
            message: error.to_string(),
        }
    }
}

/// Resolves a scope to the project that owns it.
///
/// A worktree resolves to its **parent** project, which is what makes memory written from a
/// worktree visible to the project and its sibling worktrees (FR-047) — and it is the same join
/// `skill_target_root` already relies on.
pub struct DbScopeDirectory {
    database: Database,
}

impl DbScopeDirectory {
    #[must_use]
    pub const fn new(database: Database) -> Self {
        Self { database }
    }

    fn lookup(&self, sql: &str, id: &str) -> Result<Option<(String, String)>, MemoryError> {
        let conn = self
            .database
            .connection()
            .map_err(|e| MemoryError::InvalidScope(e.to_string()))?;
        conn.query_row(sql, [id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(MemoryError::InvalidScope(other.to_string())),
            })
    }
}

impl ScopeDirectory for DbScopeDirectory {
    fn resolve(&self, scope: &Scope) -> Result<ScopeInput, MemoryError> {
        if scope.level == ScopeLevel::Global {
            return Ok(ScopeInput::global());
        }
        let Some(ref_id) = scope.ref_id.as_deref().filter(|s| !s.is_empty()) else {
            return Err(MemoryError::InvalidScope(
                "this scope requires a reference id".into(),
            ));
        };

        // Each arm yields (project_id, project_path) plus an optional human label for the page
        // path — a worktree's branch, for instance.
        let (found, label) = match scope.level {
            ScopeLevel::Project => (
                self.lookup("SELECT id, path FROM projects WHERE id = ?1", ref_id)?,
                None,
            ),
            ScopeLevel::Worktree => {
                let branch = self
                    .lookup("SELECT branch, branch FROM worktrees WHERE id = ?1", ref_id)?
                    .map(|(branch, _)| branch);
                (
                    self.lookup(
                        "SELECT p.id, p.path FROM worktrees w JOIN projects p ON p.id = w.project_id WHERE w.id = ?1",
                        ref_id,
                    )?,
                    branch,
                )
            }
            ScopeLevel::Workspace => (
                self.lookup(
                    "SELECT p.id, p.path FROM workspaces ws \
                     LEFT JOIN worktrees w ON w.id = ws.worktree_id \
                     JOIN projects p ON p.id = COALESCE(w.project_id, ws.project_id) \
                     WHERE ws.id = ?1",
                    ref_id,
                )?,
                None,
            ),
            ScopeLevel::Session => (
                self.lookup(
                    "SELECT p.id, p.path FROM terminal_sessions s JOIN projects p ON p.id = s.project_id WHERE s.id = ?1",
                    ref_id,
                )?,
                None,
            ),
            ScopeLevel::Global => unreachable!("handled above"),
        };

        let Some((project_id, project_path)) = found else {
            return Err(MemoryError::InvalidScope(
                "this scope no longer points at a project".into(),
            ));
        };

        Ok(ScopeInput::for_project(
            scope,
            ProjectRef {
                id: project_id,
                path: PathBuf::from(project_path),
            },
            label,
        ))
    }
}

/// Settings the kernel needs, read from `app_settings`. Never a secret: the bearer token, when
/// there is one, comes from the Keychain.
pub struct KernelSettings {
    pub server_url: String,
    pub auto_start: bool,
    pub binary: Option<PathBuf>,
    pub hybrid_search: bool,
}

impl KernelSettings {
    pub fn load(database: &Database) -> Self {
        let read = |key: &str| -> Option<serde_json::Value> {
            terminal_ai_persistence::dao::SettingsDao(database)
                .get(key)
                .ok()
                .flatten()
        };
        Self {
            server_url: read("memory_kernel_server_url")
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "http://127.0.0.1:49374".into()),
            auto_start: read("memory_kernel_auto_start").and_then(|v| v.as_bool()) != Some(false),
            binary: read("memory_kernel_binary").and_then(|v| v.as_str().map(PathBuf::from)),
            hybrid_search: read("memory_kernel_hybrid_search").and_then(|v| v.as_bool())
                == Some(true),
        }
    }
}

/// The Keychain account holding an optional bearer token.
pub const TOKEN_ACCOUNT: &str = "ai-memory-auth-token";

/// Build the kernel configuration, or `None` when no binary can be found anywhere.
///
/// Note the absent `data_dir`: the kernel uses its own default location, which is what makes the
/// store shared with any ai-memory the user runs outside Terminal AI. That sharing is the point of
/// the feature, not an oversight.
pub fn build_config(
    settings: &KernelSettings,
    path_lookup: Option<PathBuf>,
) -> Option<KernelConfig> {
    let binary = terminal_ai_memory_kernel::cli::resolve_binary(
        settings.binary.as_deref(),
        path_lookup.as_deref(),
    )?;
    let bind = settings
        .server_url
        .rsplit('/')
        .next()
        .filter(|s| s.contains(':'))
        .unwrap_or("127.0.0.1:49374")
        .to_owned();
    let token = terminal_ai_platform_macos::keychain::get(
        terminal_ai_platform_macos::keychain::SERVICE,
        TOKEN_ACCOUNT,
    )
    .ok()
    .flatten()
    .map(terminal_ai_memory_kernel::token::AuthToken::new);

    let hooks_dir = resolve_hooks_dir(&binary);
    if hooks_dir.is_none() {
        tracing::warn!(
            "no ai-memory hooks bundle found next to the kernel binary; capture wiring will fail \
             until scripts/fetch-ai-memory.sh is run"
        );
    }

    Some(KernelConfig {
        binary,
        server_url: settings.server_url.clone(),
        bind,
        data_dir: None,
        token,
        hybrid_search: settings.hybrid_search,
        hooks_dir,
    })
}

/// Find the `hooks/<agent>/` bundle that ships with the kernel release.
///
/// The kernel searches for this itself, but none of the places it looks is where a Tauri sidecar
/// ends up — which is how the wiring flow shipped broken. So the app finds it and passes
/// `--hooks-dir` explicitly. `fetch-ai-memory.sh` puts it beside the binary, and Tauri's bundler
/// puts declared resources in `Contents/Resources`.
fn resolve_hooks_dir(binary: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = binary.parent() {
        candidates.push(dir.join("hooks"));
        // macOS bundle: the executable is in Contents/MacOS, resources in Contents/Resources.
        candidates.push(dir.join("../Resources/hooks"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("hooks"));
            candidates.push(dir.join("../Resources/hooks"));
        }
    }
    candidates
        .into_iter()
        // Require a known agent subdirectory: a stray empty `hooks/` would satisfy a bare
        // `is_dir()` and send us back to the same failure with a more confusing message.
        .find(|path| path.join("claude-code").is_dir())
        .and_then(|path| path.canonicalize().ok())
}

/// Reject anything that is not loopback (FR-063).
pub fn validate_loopback(url: &str) -> Result<(), AppError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| AppError {
        code: "INVALID_URL".into(),
        message: format!("Not a valid URL: {e}"),
    })?;
    let host = parsed.host_str().unwrap_or_default();
    if matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        Ok(())
    } else {
        Err(AppError {
            code: "MEMORY_KERNEL_NOT_LOOPBACK".into(),
            message: "The memory kernel may only be reached over loopback.".into(),
        })
    }
}

// -------------------------------------------------------------------------------------------
// Legacy import
// -------------------------------------------------------------------------------------------

use async_trait::async_trait;
use terminal_ai_domain::memory::KernelScope;
use terminal_ai_memory_kernel::cli::KernelCli;
use terminal_ai_memory_kernel::migration::{
    ImportedIndex, LegacyEntry, MigrationRecorder, PageWriter, PlannedImport,
};
use terminal_ai_persistence::dao::{MemoryMigrationDao, MemoryMigrationRecord};

/// Writes imported pages through the kernel binary.
pub struct CliPageWriter(pub std::sync::Arc<KernelCli>);

#[async_trait]
impl PageWriter for CliPageWriter {
    async fn write_page(
        &self,
        scope: &KernelScope,
        path: &str,
        title: &str,
        kind: &str,
        document: &str,
    ) -> Result<(), MemoryError> {
        self.0
            .write_page(scope, path, title, kind, document)
            .await
            .map(|_| ())
    }

    async fn delete_page(&self, scope: &KernelScope, path: &str) -> Result<(), MemoryError> {
        self.0.delete_page(scope, path).await
    }
}

/// Records each import in `memory_migration_log`, one item at a time.
pub struct DbMigrationRecorder(pub Database);

#[async_trait]
impl MigrationRecorder for DbMigrationRecorder {
    async fn record(&self, item: &PlannedImport) -> Result<(), MemoryError> {
        MemoryMigrationDao(&self.0)
            .record(&MemoryMigrationRecord {
                entry_id: item.entry_id.clone(),
                workspace: item.scope.workspace.clone(),
                project: item.scope.project.clone(),
                page_path: item.page_path.clone(),
                body_sha256: item.body_sha256.clone(),
            })
            .map_err(|e| MemoryError::Upstream {
                code: None,
                message: e.to_string(),
            })
    }
}

/// Read every legacy entry, pulling each body off disk.
///
/// A body that cannot be read is *not* an error for the whole run: it becomes an entry with an
/// empty body, which the planner then reports as skipped with a reason. One unreadable file must
/// not stop a user's other fifty entries from being imported.
pub fn load_legacy_entries(database: &Database) -> Result<Vec<LegacyEntry>, AppError> {
    let conn = database.connection()?;
    let mut stmt = conn.prepare(
        "SELECT id, scope, scope_ref_id, type, title, content_path FROM memory_entries ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut entries = Vec::new();
    for row in rows {
        let (id, scope, scope_ref_id, memory_type, title, content_path) = row?;
        let Some(level) = parse_scope_level(&scope) else {
            continue;
        };
        entries.push(LegacyEntry {
            id,
            scope: Scope {
                level,
                ref_id: scope_ref_id,
            },
            memory_type: parse_memory_type(&memory_type),
            title,
            body: std::fs::read_to_string(&content_path).unwrap_or_default(),
        });
    }
    Ok(entries)
}

/// What the migration log already holds, for the planner's first idempotency layer.
pub fn imported_index(database: &Database) -> Result<ImportedIndex, AppError> {
    Ok(MemoryMigrationDao(database)
        .list()?
        .into_iter()
        .map(|record| (record.entry_id, record.body_sha256))
        .collect())
}

/// Every page a previous import wrote, addressed for removal.
pub fn imported_pages(database: &Database) -> Result<Vec<(KernelScope, String)>, AppError> {
    Ok(MemoryMigrationDao(database)
        .list()?
        .into_iter()
        .map(|record| {
            (
                KernelScope {
                    workspace: record.workspace,
                    project: record.project,
                    // Only the workspace and project address a page; the prefix is not used for
                    // deletion, so an empty one here is honest rather than a guess.
                    path_prefix: String::new(),
                },
                record.page_path,
            )
        })
        .collect())
}

fn parse_scope_level(name: &str) -> Option<ScopeLevel> {
    Some(match name {
        "global" => ScopeLevel::Global,
        "project" => ScopeLevel::Project,
        "worktree" => ScopeLevel::Worktree,
        "workspace" => ScopeLevel::Workspace,
        "session" => ScopeLevel::Session,
        _ => return None,
    })
}

fn parse_memory_type(name: &str) -> terminal_ai_domain::MemoryType {
    use terminal_ai_domain::MemoryType as T;
    match name {
        "decision" => T::Decision,
        "constraint" => T::Constraint,
        "preference" => T::Preference,
        "glossary" => T::Glossary,
        "known_issue" => T::KnownIssue,
        "command" => T::Command,
        "todo" => T::Todo,
        _ => T::Fact,
    }
}

// -------------------------------------------------------------------------------------------
// Agent wiring
// -------------------------------------------------------------------------------------------

use terminal_ai_memory_kernel::wiring::{
    self, Agent, RemovalPlan, WiringArtifact, WiringKind, WiringPlan,
};
use terminal_ai_persistence::dao::{MemoryWiringDao, MemoryWiringRecord};

/// Run one wiring command against the kernel binary and return its stdout.
///
/// Used for both the dry run and the apply. The only difference between them is `--apply`, which
/// keeps the preview honest: it is literally the same command the user is about to authorise.
async fn run_wiring(cli: &KernelCli, args: &[String]) -> Result<String, AppError> {
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    cli.run_raw(&borrowed).await.map_err(AppError::from)
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Preview what wiring would do, without writing anything.
///
/// The diff is computed from the target file, not parsed out of the kernel's prose: the dry run's
/// text is written for a person and is not a contract, so if its format changes the preview
/// degrades to "cannot show a diff" rather than authorising a change nobody saw.
pub async fn preview(
    cli: &KernelCli,
    agent: Agent,
    kind: WiringKind,
    server_url: &str,
    project_root: Option<&Path>,
    already_managed: bool,
) -> Result<WiringPlan, AppError> {
    if kind == WiringKind::Hooks && !agent.supports_scoped_capture() {
        return Err(AppError {
            code: "MEMORY_CAPTURE_UNSUPPORTED".into(),
            message: agent
                .capture_unavailable_reason()
                .unwrap_or("Capture is not available for this agent.")
                .into(),
        });
    }

    let target = wiring::target_path(agent, kind, project_root);
    let before = target.as_deref().and_then(read_file);
    let stdout = run_wiring(
        cli,
        &wiring::plan_args(
            agent,
            kind,
            server_url,
            target.as_deref(),
            cli.config().hooks_dir.as_deref(),
        ),
    )
    .await?;

    let mut warnings = Vec::new();
    let (diff, conflict) = match wiring::extract_config_block(&stdout) {
        Ok(block) => {
            let rendered = serde_json::to_string_pretty(&block).unwrap_or_default();
            let conflict = (!already_managed
                && before
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .is_some_and(|existing| wiring::has_unmanaged_entry(&existing, server_url)))
            .then(|| {
                "This is already configured outside Terminal AI. It will not be changed.".to_owned()
            });
            (
                wiring::diff(before.as_deref().unwrap_or(""), &rendered),
                conflict,
            )
        }
        Err(err) => {
            // Degrade rather than guess: the apply is refused while we cannot show what it does.
            warnings.push(format!(
                "Terminal AI could not read the kernel's plan ({err}), so it cannot show what \
                 would change. Applying is disabled."
            ));
            ("(unavailable)".to_owned(), None)
        }
    };

    Ok(WiringPlan {
        agent,
        kind,
        path: target.clone(),
        diff,
        will_create: target.as_deref().is_some_and(|p| !p.exists()),
        conflict,
        capture_events: if kind == WiringKind::Hooks {
            wiring::CAPTURED_EVENTS
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        } else {
            Vec::new()
        },
        warnings,
    })
}

/// Apply wiring and record precisely what was left behind.
pub async fn apply(
    cli: &KernelCli,
    agent: Agent,
    kind: WiringKind,
    server_url: &str,
    project_root: Option<&Path>,
    backup_root: &Path,
) -> Result<WiringArtifact, AppError> {
    let target = wiring::target_path(agent, kind, project_root);
    let before = target.as_deref().and_then(read_file);
    let created_file = before.is_none();

    // Back up before touching a file we did not create. Without this, removal has nothing to
    // restore and the only options would be clobbering or giving up.
    let backup_path = match (&target, &before) {
        (Some(path), Some(content)) => {
            let dir = backup_root.join(chrono::Utc::now().timestamp_millis().to_string());
            std::fs::create_dir_all(&dir).map_err(AppError::internal)?;
            let name = path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("config"));
            let backup = dir.join(name);
            std::fs::write(&backup, content).map_err(AppError::internal)?;
            Some(backup)
        }
        _ => None,
    };

    run_wiring(
        cli,
        &wiring::apply_args(
            agent,
            kind,
            server_url,
            target.as_deref(),
            cli.config().hooks_dir.as_deref(),
        ),
    )
    .await?;

    let after = target.as_deref().and_then(read_file).unwrap_or_default();
    Ok(WiringArtifact {
        path: target.unwrap_or_else(|| PathBuf::from("<agent default>")),
        created_file,
        backup_path,
        before_sha256: before.as_deref().map(wiring::hash),
        after_sha256: wiring::hash(&after),
        binary_path: Some(cli.config().binary.clone()),
        applied_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Undo one artifact, refusing anything that changed since we wrote it.
pub async fn remove_artifact(
    cli: &KernelCli,
    kind: WiringKind,
    server_url: &str,
    artifact: &WiringArtifact,
) -> Result<String, AppError> {
    // Tier 1: upstream's own remover, which matches an MCP entry by URL rather than by name.
    if artifact.path.to_string_lossy() == "<agent default>" {
        run_wiring(cli, &wiring::uninstall_args(kind, server_url, true)).await?;
        return Ok(format!("{kind:?} removed by the kernel's own uninstaller"));
    }

    let current = read_file(&artifact.path);
    match wiring::plan_removal(artifact, current.as_deref()) {
        RemovalPlan::NothingToDo => Ok(format!("{} was already gone", artifact.path.display())),
        RemovalPlan::DeleteFile => {
            std::fs::remove_file(&artifact.path).map_err(AppError::internal)?;
            Ok(format!("removed {}", artifact.path.display()))
        }
        RemovalPlan::RestoreBackup(backup) => {
            let content = std::fs::read_to_string(&backup).map_err(AppError::internal)?;
            std::fs::write(&artifact.path, content).map_err(AppError::internal)?;
            Ok(format!("restored {}", artifact.path.display()))
        }
        RemovalPlan::Refuse(refusal) => Err(AppError {
            code: "MEMORY_WIRING_DRIFTED".into(),
            message: format!(
                "{} was changed after Terminal AI configured it ({}), so it was left alone. {}",
                refusal.path.display(),
                refusal.reason,
                refusal.backup_path.map_or_else(
                    || "No backup is available.".to_owned(),
                    |b| format!("A copy of what Terminal AI wrote is at {}.", b.display())
                )
            ),
        }),
    }
}

/// Persisted wiring, with staleness resolved against the binary in use right now.
pub fn list_bindings(
    database: &Database,
    current_binary: Option<&Path>,
) -> Result<Vec<(MemoryWiringRecord, bool)>, AppError> {
    Ok(MemoryWiringDao(database)
        .list()?
        .into_iter()
        .map(|record| {
            let stale = record.artifacts.iter().any(|a| {
                let artifact = WiringArtifact {
                    path: PathBuf::from(&a.path),
                    created_file: a.created_file,
                    backup_path: a.backup_path.as_deref().map(PathBuf::from),
                    before_sha256: a.before_sha256.clone(),
                    after_sha256: a.after_sha256.clone(),
                    binary_path: a.binary_path.as_deref().map(PathBuf::from),
                    applied_at: a.applied_at.clone(),
                };
                wiring::is_stale(&artifact, current_binary)
            });
            (record, stale)
        })
        .collect())
}
