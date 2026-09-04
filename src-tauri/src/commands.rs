use crate::events::{HostErrorEvent, TerminalChunk};
use crate::state::AppState;
use base64::Engine;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use tauri::{ipc::Channel, AppHandle, Emitter, State};
use terminal_ai_domain::{
    host::{LaunchContext, ResumeRef, SessionHost, Signal},
    ProjectId, ProviderId, SessionId, WorkspaceId, WorktreeId,
};
use terminal_ai_domain::{
    LayoutNode, MemoryType, ProviderKind, ProviderProfile, Scope, ScopeLevel,
};
use terminal_ai_memory_manager as memory_manager;
use terminal_ai_persistence::dao::{
    LayoutPresetRecord, LayoutPresetsDao, ProjectRecord, ProjectsDao, ProviderProfileRecord,
    ProviderProfilesDao, SessionRecord, SessionsDao, SettingsDao, SkillBindingRecord, SkillRecord,
    SkillsDao, UsageSnapshotRecord, UsageSnapshotsDao, WorkspacesDao, WorktreeRecord, WorktreesDao,
};
use terminal_ai_platform_macos::resolve_login_shell_env;
use terminal_ai_project_manager as project_manager;
use terminal_ai_provider_runtime::{AgentProvider, ProfileInput, ProviderAdapter};
use terminal_ai_skill_manager as skill_manager;
use terminal_ai_usage_core::{poller::RefreshResult, AuthState, UsageCard, UsageSnapshot};
use terminal_ai_worktree_manager as worktree_manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}
impl AppError {
    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            code: "INTERNAL".into(),
            message: error.to_string(),
        }
    }
}
impl From<terminal_ai_persistence::PersistenceError> for AppError {
    fn from(error: terminal_ai_persistence::PersistenceError) -> Self {
        Self::internal(error)
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::internal(error)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentResponse {
    pub path: String,
    pub env: BTreeMap<String, String>,
    pub cached_at: String,
}

#[tauri::command]
pub async fn resolve_env(
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<EnvironmentResponse, AppError> {
    if !force.unwrap_or(false) {
        if let Some(env) = state
            .environment
            .read()
            .map_err(|_| AppError::internal("environment cache poisoned"))?
            .clone()
        {
            return Ok(EnvironmentResponse {
                path: env.path,
                env: env.env,
                cached_at: env.cached_at,
            });
        }
    }
    let resolved = tokio::task::spawn_blocking(resolve_login_shell_env)
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    *state
        .environment
        .write()
        .map_err(|_| AppError::internal("environment cache poisoned"))? = Some(resolved.clone());
    Ok(EnvironmentResponse {
        path: resolved.path,
        env: resolved.env,
        cached_at: resolved.cached_at,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OkResponse {
    pub ok: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub pid: u32,
    pub title: String,
    pub state: String,
}

fn launch_cwd(
    state: &AppState,
    project_id: Option<&str>,
    worktree_id: Option<&str>,
) -> Result<(PathBuf, Option<ProjectId>, Option<WorktreeId>), AppError> {
    let conn = state.database.connection().map_err(AppError::internal)?;
    if let Some(worktree_id) = worktree_id {
        let (path, project): (String, String) = conn
            .query_row(
                "SELECT w.path,w.project_id FROM worktrees w WHERE w.id=?1",
                [worktree_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(AppError::internal)?;
        let cwd = std::fs::canonicalize(path).map_err(AppError::internal)?;
        return Ok((
            cwd,
            Some(ProjectId(project)),
            Some(WorktreeId(worktree_id.into())),
        ));
    }
    if let Some(project_id) = project_id {
        let path: String = conn
            .query_row(
                "SELECT path FROM projects WHERE id=?1",
                [project_id],
                |row| row.get(0),
            )
            .map_err(AppError::internal)?;
        let cwd = std::fs::canonicalize(path).map_err(AppError::internal)?;
        return Ok((cwd, Some(ProjectId(project_id.into())), None));
    }
    let cwd = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::internal("home directory unavailable"))?;
    Ok((cwd, None, None))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // IPC contract is intentionally flat for typed Tauri calls.
pub async fn create_session(
    project_id: Option<String>,
    worktree_id: Option<String>,
    provider_id: String,
    cols: u16,
    rows: u16,
    on_output: Channel<TerminalChunk>,
    resume: Option<ResumeRef>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<CreateSessionResponse, AppError> {
    let (cwd, project_id, worktree_id) =
        launch_cwd(&state, project_id.as_deref(), worktree_id.as_deref())?;
    let history = (
        project_id.clone(),
        worktree_id.clone(),
        provider_id.clone(),
        cwd.clone(),
    );
    // T063: honest resume reference — an explicit ById id round-trips for `--resume <id>`;
    // resume-capable providers get "continue"; shells get None. Capturing an agent's own
    // session id for ById requires scanning the agent's session files and is out of scope here.
    let resume_ref = match &resume {
        Some(ResumeRef::ById(id)) => Some(id.clone()),
        _ if provider_id != "shell" => Some("continue".into()),
        _ => None,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let info = match state
        .host
        .create(
            LaunchContext {
                project_id,
                worktree_id,
                provider_id: ProviderId(provider_id),
                cwd,
                cols,
                rows,
                resume,
            },
            tx,
        )
        .await
    {
        Ok(info) => info,
        Err(error) => {
            // T070: surface spawn failures to the UI.
            let _ = app.emit(
                "host-error",
                HostErrorEvent {
                    scope: "session".into(),
                    session_id: None,
                    code: "SPAWN_FAILED".into(),
                    message: error.to_string(),
                },
            );
            return Err(AppError::internal(error));
        }
    };
    if let Some(project) = history.0 {
        let database = state.database.clone();
        let record = SessionRecord {
            id: info.id.0.clone(),
            pane_id: None,
            project_id: Some(project.0),
            worktree_id: history.1.map(|id| id.0),
            provider_id: history.2.clone(),
            cwd: history.3.to_string_lossy().into_owned(),
            title: Some(info.title.clone()),
            state: "running".into(),
            exit_code: None,
            resume_ref,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: None,
        };
        tokio::task::spawn_blocking(move || SessionsDao(&database).insert(&record))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?;
        // T081: after Claude has had a moment to create its session file, capture that native
        // session id and store it so a history entry resumes the *specific* session. Best-effort.
        if history.2 == "claude" {
            let capture_db = state.database.clone();
            let capture_session = info.id.0.clone();
            let capture_cwd = history.3.clone();
            let spawn_at = std::time::SystemTime::now();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                let Ok(Some(native_id)) = tokio::task::spawn_blocking(move || {
                    capture_claude_session_id(&capture_cwd, spawn_at)
                })
                .await
                else {
                    return;
                };
                let _ = tokio::task::spawn_blocking(move || {
                    capture_db.connection().and_then(|conn| {
                        conn.execute(
                            "UPDATE terminal_sessions SET resume_ref=?1 WHERE id=?2",
                            rusqlite::params![native_id, capture_session],
                        )?;
                        Ok::<(), terminal_ai_persistence::PersistenceError>(())
                    })
                })
                .await;
            });
        }
    }
    let session_id = info.id.0.clone();
    let channel_id = session_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let message = TerminalChunk {
                session_id: channel_id.clone(),
                seq: chunk.seq,
                bytes: base64::engine::general_purpose::STANDARD.encode(chunk.bytes),
            };
            if on_output.send(message).is_err() {
                break;
            }
        }
    });
    Ok(CreateSessionResponse {
        session_id,
        pid: info.pid,
        title: info.title,
        state: "running".into(),
    })
}
#[tauri::command]
pub async fn write_input(
    session_id: String,
    data: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    state
        .host
        .write(&SessionId(session_id), data.as_bytes())
        .await
        .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[tauri::command]
pub async fn resize_session(
    session_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    if !(1..=1000).contains(&cols) || !(1..=1000).contains(&rows) {
        return Err(AppError {
            code: "INVALID_SIZE".into(),
            message: "Terminal size must be between 1 and 1000.".into(),
        });
    }
    state
        .host
        .resize(&SessionId(session_id), cols, rows)
        .await
        .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[tauri::command]
pub async fn send_signal(
    session_id: String,
    signal: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let signal = match signal.as_str() {
        "SIGINT" => Signal::Int,
        "SIGTERM" => Signal::Term,
        "SIGKILL" => Signal::Kill,
        "SIGHUP" => Signal::Hup,
        _ => {
            return Err(AppError {
                code: "INVALID_SIGNAL".into(),
                message: "Signal is not allowed.".into(),
            })
        }
    };
    state
        .host
        .signal(&SessionId(session_id), signal)
        .await
        .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionResponse {
    pub ok: bool,
    pub exit_code: Option<i32>,
}
#[tauri::command]
pub async fn close_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<CloseSessionResponse, AppError> {
    let history_id = session_id.clone();
    let exit_code = state
        .host
        .close(&SessionId(session_id))
        .await
        .map_err(AppError::internal)?;
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        SessionsDao(&database).finish(&history_id, "exited", exit_code)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(CloseSessionResponse {
        ok: true,
        exit_code,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartResponse {
    pub session_id: String,
    pub pid: u32,
}
#[tauri::command]
pub async fn restart_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<RestartResponse, AppError> {
    let old_id = session_id.clone();
    // T067: capture the old row before restarting (SessionInfo lacks cwd), so we can finalize
    // the old DB row and insert a new one — keeping history and the live session map in sync.
    // Sessions without a project have no history row (mirrors create_session), so we skip those.
    let old_row = {
        let database = state.database.clone();
        let lookup = old_id.clone();
        tokio::task::spawn_blocking(move || SessionsDao(&database).get(&lookup))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?
    };
    let info = state
        .host
        .restart(&SessionId(session_id))
        .await
        .map_err(AppError::internal)?;
    if let Some(row) = old_row.filter(|row| row.project_id.is_some()) {
        let database = state.database.clone();
        let new_record = SessionRecord {
            id: info.id.0.clone(),
            pane_id: row.pane_id,
            project_id: row.project_id,
            worktree_id: row.worktree_id,
            provider_id: row.provider_id,
            cwd: row.cwd,
            title: Some(info.title.clone()),
            state: "running".into(),
            exit_code: None,
            resume_ref: row.resume_ref,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: None,
        };
        tokio::task::spawn_blocking(
            move || -> Result<(), terminal_ai_persistence::PersistenceError> {
                let dao = SessionsDao(&database);
                dao.finish(&old_id, "exited", None)?;
                dao.insert(&new_record)?;
                Ok(())
            },
        )
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    }
    Ok(RestartResponse {
        session_id: info.id.0,
        pid: info.pid,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub provider_id: String,
    pub project_id: Option<String>,
    pub worktree_id: Option<String>,
    pub title: String,
    pub state: String,
    pub pid: u32,
}
#[derive(Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionSummary>,
}
#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<SessionsResponse, AppError> {
    let sessions = state
        .host
        .list()
        .await
        .map_err(AppError::internal)?
        .into_iter()
        .map(|info| SessionSummary {
            session_id: info.id.0,
            provider_id: info.provider_id.0,
            project_id: info.project_id.map(|id| id.0),
            worktree_id: info.worktree_id.map(|id| id.0),
            title: info.title,
            state: "running".into(),
            pid: info.pid,
        })
        .collect();
    Ok(SessionsResponse { sessions })
}
#[derive(Serialize)]
pub struct ScrollbackResponse {
    pub data: String,
    pub truncated: bool,
}
#[tauri::command]
pub async fn get_scrollback(
    session_id: String,
    max_bytes: Option<usize>,
    state: State<'_, AppState>,
) -> Result<ScrollbackResponse, AppError> {
    let (data, truncated) = state
        .host
        .scrollback(&SessionId(session_id), max_bytes.unwrap_or(1_000_000))
        .await
        .map_err(AppError::internal)?;
    Ok(ScrollbackResponse {
        data: base64::engine::general_purpose::STANDARD.encode(data),
        truncated,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub provider_id: String,
    pub worktree_id: Option<String>,
    pub cwd: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub title: String,
    pub resume: Option<ResumeRef>,
}
#[derive(Serialize)]
pub struct HistoryResponse {
    pub entries: Vec<HistoryEntry>,
}
#[tauri::command]
pub async fn get_session_history(
    project_id: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<HistoryResponse, AppError> {
    let database = state.database.clone();
    let rows = tokio::task::spawn_blocking(move || {
        SessionsDao(&database).history(&project_id, limit.unwrap_or(100))
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(HistoryResponse {
        entries: rows
            .into_iter()
            .map(|row| HistoryEntry {
                id: row.id,
                provider_id: row.provider_id,
                worktree_id: row.worktree_id,
                cwd: row.cwd,
                started_at: row.started_at,
                ended_at: row.ended_at,
                title: row.title.unwrap_or_else(|| "Session".into()),
                resume: row.resume_ref.map(|value| {
                    if value == "continue" {
                        ResumeRef::Continue
                    } else {
                        ResumeRef::ById(value)
                    }
                }),
            })
            .collect(),
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeResponse {
    pub session_id: String,
    pub pid: u32,
    pub resumed: bool,
}
#[tauri::command]
pub async fn resume_session(
    history_id: String,
    cols: u16,
    rows: u16,
    on_output: Channel<TerminalChunk>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ResumeResponse, AppError> {
    let database = state.database.clone();
    let lookup = history_id.clone();
    let row = tokio::task::spawn_blocking(move || SessionsDao(&database).get(&lookup))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError {
            code: "HISTORY_NOT_FOUND".into(),
            message: "Session history entry does not exist.".into(),
        })?;
    let (project_id, worktree_id) = (row.project_id.clone(), row.worktree_id.clone());
    let (cwd, project_id, worktree_id) =
        launch_cwd(&state, project_id.as_deref(), worktree_id.as_deref())?;
    let resume = row.resume_ref.map(|value| {
        if value == "continue" {
            ResumeRef::Continue
        } else {
            ResumeRef::ById(value)
        }
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let (info, resumed) = match state
        .host
        .resume(
            LaunchContext {
                project_id,
                worktree_id,
                provider_id: ProviderId(row.provider_id),
                cwd,
                cols,
                rows,
                resume,
            },
            tx,
        )
        .await
    {
        Ok(result) => result,
        Err(error) => {
            // T070: surface resume/spawn failures to the UI.
            let _ = app.emit(
                "host-error",
                HostErrorEvent {
                    scope: "session".into(),
                    session_id: None,
                    code: "RESUME_FAILED".into(),
                    message: error.to_string(),
                },
            );
            return Err(AppError::internal(error));
        }
    };
    let session_id = info.id.0.clone();
    let channel_id = session_id.clone();
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            if on_output
                .send(TerminalChunk {
                    session_id: channel_id.clone(),
                    seq: chunk.seq,
                    bytes: base64::engine::general_purpose::STANDARD.encode(chunk.bytes),
                })
                .is_err()
            {
                break;
            }
        }
    });
    Ok(ResumeResponse {
        session_id,
        pid: info.pid,
        resumed,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub active: bool,
    pub root_path: Option<String>,
}
#[derive(Serialize)]
pub struct WorkspacesResponse {
    pub workspaces: Vec<WorkspaceSummary>,
}
#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> Result<WorkspacesResponse, AppError> {
    let database = state.database.clone();
    let rows = tokio::task::spawn_blocking(move || WorkspacesDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(WorkspacesResponse {
        workspaces: rows
            .into_iter()
            .enumerate()
            .map(
                |(index, (id, title, project_id, root_path))| WorkspaceSummary {
                    id: id.0,
                    title,
                    project_id: project_id.map(|id| id.0),
                    active: index == 0,
                    root_path,
                },
            )
            .collect(),
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCreated {
    pub workspace_id: String,
}
#[tauri::command]
pub async fn create_workspace(
    title: Option<String>,
    project_id: Option<String>,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<WorkspaceCreated, AppError> {
    let database = state.database.clone();
    let project_id = project_id.map(ProjectId);
    let worktree_id = worktree_id.map(WorktreeId);
    let id = tokio::task::spawn_blocking(move || {
        if let (Some(project), Some(worktree)) = (&project_id, &worktree_id) {
            let belongs: i64 = database.connection()?.query_row(
                "SELECT count(*) FROM worktrees WHERE id=?1 AND project_id=?2",
                rusqlite::params![worktree.0, project.0],
                |row| row.get(0),
            )?;
            if belongs == 0 {
                return Err(terminal_ai_persistence::PersistenceError::Io(
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "worktree does not belong to project",
                    ),
                ));
            }
        }
        WorkspacesDao(&database).create(
            title.as_deref().unwrap_or("Workspace"),
            project_id.as_ref(),
            worktree_id.as_ref(),
        )
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(WorkspaceCreated { workspace_id: id.0 })
}
#[tauri::command]
pub async fn close_workspace(
    workspace_id: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        database
            .connection()?
            .execute("DELETE FROM workspaces WHERE id=?1", [workspace_id])?;
        Ok::<(), terminal_ai_persistence::PersistenceError>(())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[tauri::command]
pub async fn save_layout(
    workspace_id: String,
    layout: LayoutNode,
    pane_bindings: BTreeMap<String, PaneBinding>,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    layout.validate().map_err(AppError::internal)?;
    let pane_ids = layout_pane_ids(&layout);
    if pane_bindings.keys().any(|id| !pane_ids.contains(id)) {
        return Err(AppError {
            code: "INVALID_PANE_BINDING".into(),
            message: "Pane binding does not belong to the layout.".into(),
        });
    }
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || -> Result<(),terminal_ai_persistence::PersistenceError> {
        let mut connection=database.connection()?;let transaction=connection.transaction()?;let layout_id=uuid::Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO workspace_layouts(id,workspace_id,layout_json,updated_at)VALUES(?1,?2,?3,?4)",rusqlite::params![layout_id,workspace_id,serde_json::to_string(&layout)?,chrono::Utc::now().to_rfc3339()])?;
        transaction.execute("UPDATE workspaces SET active_layout_id=?2 WHERE id=?1",rusqlite::params![workspace_id,layout_id])?;
        let existing={let mut statement=transaction.prepare("SELECT pane_key FROM panes WHERE workspace_id=?1")?;let rows=statement.query_map([&workspace_id],|row|row.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;rows};
        for pane_id in &pane_ids{let binding=pane_bindings.get(pane_id).cloned().unwrap_or_default();transaction.execute("INSERT INTO panes(id,workspace_id,pane_key,provider_id,project_id,worktree_id,title,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)ON CONFLICT(workspace_id,pane_key)DO UPDATE SET provider_id=excluded.provider_id,project_id=excluded.project_id,worktree_id=excluded.worktree_id,title=excluded.title",rusqlite::params![uuid::Uuid::new_v4().to_string(),workspace_id,pane_id,binding.provider_id,binding.project_id,binding.worktree_id,binding.title,chrono::Utc::now().to_rfc3339()])?;}
        for removed in existing.into_iter().filter(|id|!pane_ids.contains(id)){transaction.execute("DELETE FROM panes WHERE workspace_id=?1 AND pane_key=?2",rusqlite::params![workspace_id,removed])?;}
        transaction.commit()?;Ok(())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneBinding {
    pub provider_id: Option<String>,
    pub project_id: Option<String>,
    pub worktree_id: Option<String>,
    pub title: Option<String>,
}
type PersistedLayout = Option<(LayoutNode, BTreeMap<String, PaneBinding>)>;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutResponse {
    pub layout: LayoutNode,
    pub pane_bindings: BTreeMap<String, PaneBinding>,
}
#[tauri::command]
pub async fn load_layout(
    workspace_id: String,
    state: State<'_, AppState>,
) -> Result<LayoutResponse, AppError> {
    let database = state.database.clone();
    let (layout,pane_bindings)=tokio::task::spawn_blocking(move || -> Result<PersistedLayout,terminal_ai_persistence::PersistenceError> {let connection=database.connection()?;let json=match connection.query_row("SELECT layout_json FROM workspace_layouts WHERE workspace_id=?1 ORDER BY updated_at DESC LIMIT 1",[&workspace_id],|row|row.get::<_,String>(0)){Ok(json)=>json,Err(rusqlite::Error::QueryReturnedNoRows)=>return Ok(None),Err(error)=>return Err(error.into())};let layout=serde_json::from_str(&json)?;let mut stmt=connection.prepare("SELECT pane_key,provider_id,project_id,worktree_id,title FROM panes WHERE workspace_id=?1")?;let rows=stmt.query_map([&workspace_id],|row|Ok((row.get::<_,String>(0)?,PaneBinding{provider_id:row.get(1)?,project_id:row.get(2)?,worktree_id:row.get(3)?,title:row.get(4)?})))?;let bindings=rows.collect::<Result<BTreeMap<_,_>,_>>()?;Ok(Some((layout,bindings)))})
            .await.map_err(AppError::internal)?.map_err(AppError::internal)?.ok_or_else(|| AppError {
                code: "LAYOUT_NOT_FOUND".into(),
                message: "Workspace has no saved layout.".into(),
            })?;
    Ok(LayoutResponse {
        layout,
        pane_bindings,
    })
}

fn layout_pane_ids(layout: &LayoutNode) -> BTreeSet<String> {
    fn visit(node: &LayoutNode, ids: &mut BTreeSet<String>) {
        match node {
            LayoutNode::Pane { pane_id } => {
                ids.insert(pane_id.0.clone());
            }
            LayoutNode::Split { children, .. } => {
                children.iter().for_each(|child| visit(child, ids))
            }
        }
    }
    let mut ids = BTreeSet::new();
    visit(layout, &mut ids);
    ids
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub active_sessions: usize,
    pub color: Option<String>,
    pub archived: bool,
}
#[derive(Serialize)]
pub struct ProjectsResponse {
    pub projects: Vec<ProjectSummary>,
}
fn project_summary(record: ProjectRecord) -> ProjectSummary {
    let status = project_manager::inspect(std::path::Path::new(&record.path)).ok();
    summary_from(record, status)
}

/// A stored row is only a project for as long as its directory is still a git repository on
/// disk, so the list reflects the folder rather than the database's memory of it. The row is
/// deliberately kept rather than deleted: `projects` cascades into `terminal_sessions` and
/// `workspaces`, so pruning on a transient absence — an unmounted volume, a folder being moved —
/// would silently destroy session history. `remove_project` stays the way to forget one.
fn live_project_summary(record: ProjectRecord) -> Option<ProjectSummary> {
    let status = project_manager::inspect(std::path::Path::new(&record.path)).ok()?;
    Some(summary_from(record, Some(status)))
}

fn summary_from(
    record: ProjectRecord,
    status: Option<project_manager::DiscoveredProject>,
) -> ProjectSummary {
    ProjectSummary {
        id: record.id.0,
        // The user's own name wins over the directory name discovery keeps rewriting.
        name: record.display_name.unwrap_or(record.name),
        path: record.path,
        remote: status.as_ref().and_then(|project| project.remote.clone()),
        branch: status.as_ref().map(|project| project.status.branch.clone()),
        dirty: status.as_ref().is_some_and(|project| project.status.dirty),
        ahead: status.as_ref().map_or(0, |project| project.status.ahead),
        behind: status.as_ref().map_or(0, |project| project.status.behind),
        active_sessions: 0,
        color: None,
        archived: record.archived,
    }
}
#[tauri::command]
pub async fn list_projects(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProjectsResponse, AppError> {
    let database = state.database.clone();
    let database_for_discovery = database.clone();
    // A workspace may pin its own folder; otherwise fall back to the globally configured roots.
    // Whichever applies both drives discovery and filters the listing, so a workspace never
    // shows repositories from outside the folder it is pointed at.
    let roots = match workspace_scoped_root(&state, workspace_id.as_deref())? {
        Some(root) => vec![root],
        None => configured_roots(&state)?,
    };
    let scope = roots.clone();
    tokio::task::spawn_blocking(
        move || -> Result<(), terminal_ai_persistence::PersistenceError> {
            if let Ok(discovered) = project_manager::discover(&roots) {
                let dao = ProjectsDao(&database_for_discovery);
                for project in discovered {
                    dao.insert(&ProjectRecord {
                        id: ProjectId::new(),
                        name: project.name,
                        path: project.path.to_string_lossy().into_owned(),
                        archived: false,
                        display_name: None,
                    })?;
                }
            }
            Ok(())
        },
    )
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    let records = tokio::task::spawn_blocking(move || ProjectsDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for session in state.host.list().await.map_err(AppError::internal)? {
        if let Some(project) = session.project_id {
            *counts.entry(project.0).or_default() += 1;
        }
    }
    Ok(ProjectsResponse {
        projects: records
            .into_iter()
            // A project belongs to a root when that root is its *parent*, matching how
            // `project_manager::discover` scans — one level of `read_dir`, no recursion. A
            // prefix test would disagree with discovery: `~/www` would list repositories under
            // `~/www/thayna` that its own scan can never find, leaking a nested workspace's
            // projects into the one above it.
            .filter(|record| {
                let path = std::path::Path::new(&record.path);
                path.parent()
                    .is_some_and(|parent| scope.iter().any(|root| parent == root))
            })
            .filter_map(live_project_summary)
            .map(|mut project| {
                project.active_sessions = counts.get(&project.id).copied().unwrap_or_default();
                project
            })
            .collect(),
    })
}

/// The folder a workspace pins for its project list, canonicalized. `None` means it has none
/// and the globally configured roots apply.
fn workspace_scoped_root(
    state: &AppState,
    workspace_id: Option<&str>,
) -> Result<Option<PathBuf>, AppError> {
    let Some(workspace_id) = workspace_id else {
        return Ok(None);
    };
    let raw = WorkspacesDao(&state.database)
        .root_path(&WorkspaceId(workspace_id.to_owned()))
        .map_err(AppError::internal)?;
    Ok(raw
        .and_then(|entry| expand_home(&entry))
        .and_then(|path| path.canonicalize().ok()))
}
#[derive(Serialize)]
pub struct ProjectResponse {
    pub project: ProjectSummary,
}

/// Renames a project for display. Passing an empty name restores the directory name.
#[tauri::command]
pub async fn set_project_name(
    project_id: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let trimmed = name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        ProjectsDao(&database).set_display_name(&ProjectId(project_id), trimmed.as_deref())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}

/// Opens the native folder chooser and returns the picked path, or `None` if it was cancelled.
/// The dialog is driven from Rust so the WebView never receives the plugin's JS API — the
/// frontend still reaches this through one typed command (Principle I).
#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |picked| {
        let _ = sender.send(picked);
    });
    let picked = receiver.await.map_err(AppError::internal)?;
    Ok(picked
        .and_then(|path| path.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned()))
}

/// Hides a project from the sidebar without forgetting it. The row and its history stay; only
/// the flag changes, so restoring is lossless and rediscovery will not resurrect it.
#[tauri::command]
pub async fn set_project_archived(
    project_id: String,
    archived: bool,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        ProjectsDao(&database).set_archived(&ProjectId(project_id), archived)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRootResponse {
    pub root_path: Option<String>,
}

/// Renames a workspace tab.
#[tauri::command]
pub async fn rename_workspace(
    workspace_id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let title = title.trim().to_owned();
    if title.is_empty() {
        return Err(AppError {
            code: "INVALID_NAME".into(),
            message: "Workspace name cannot be empty.".into(),
        });
    }
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        WorkspacesDao(&database).set_title(&WorkspaceId(workspace_id), &title)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}

/// Points a workspace at its own project folder. `path: None` clears it, returning the
/// workspace to the globally configured roots.
#[tauri::command]
pub async fn set_workspace_root(
    workspace_id: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<WorkspaceRootResponse, AppError> {
    let canonical = match path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(entry) => {
            let expanded = expand_home(entry).ok_or_else(|| AppError {
                code: "PATH_NOT_ALLOWED".into(),
                message: "Home directory unavailable.".into(),
            })?;
            let resolved = expanded.canonicalize().map_err(|_| AppError {
                code: "PATH_NOT_FOUND".into(),
                message: format!("{entry} does not exist."),
            })?;
            if !resolved.is_dir() {
                return Err(AppError {
                    code: "PATH_NOT_A_DIRECTORY".into(),
                    message: format!("{entry} is not a directory."),
                });
            }
            Some(resolved.to_string_lossy().into_owned())
        }
        None => None,
    };
    let database = state.database.clone();
    let stored = canonical.clone();
    tokio::task::spawn_blocking(move || {
        WorkspacesDao(&database).set_root_path(&WorkspaceId(workspace_id), stored.as_deref())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(WorkspaceRootResponse {
        root_path: canonical,
    })
}

/// The globally configured project roots (`project_root_dirs`, default `~/www`), `~` expanded
/// and canonicalized. This is what a workspace lists when it has pinned no folder of its own.
fn configured_roots(state: &AppState) -> Result<Vec<PathBuf>, AppError> {
    let raw: Vec<String> = SettingsDao(&state.database)
        .get("project_root_dirs")
        .map_err(AppError::internal)?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(|| vec!["~/www".into()]);
    let mut roots = Vec::new();
    for entry in raw {
        if let Some(canonical) = expand_home(&entry).and_then(|path| path.canonicalize().ok()) {
            roots.push(canonical);
        }
    }
    Ok(roots)
}

/// Every location a session is allowed to launch under: the configured roots plus each
/// workspace's pinned folder. This is the security boundary of Principle I and must stay a
/// union — it is NOT a display scope. Listing through it leaks one workspace's repositories
/// into every workspace that pinned no folder of its own.
fn allowed_roots(state: &AppState) -> Result<Vec<PathBuf>, AppError> {
    let mut roots = configured_roots(state)?;
    for entry in WorkspacesDao(&state.database)
        .all_root_paths()
        .map_err(AppError::internal)?
    {
        if let Some(canonical) = expand_home(&entry).and_then(|path| path.canonicalize().ok()) {
            if !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
    }
    Ok(roots)
}

/// Expands a leading `~` against `$HOME`. Returns `None` when the home directory is unknown.
fn expand_home(entry: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if entry == "~" {
        home
    } else if let Some(rest) = entry.strip_prefix("~/") {
        home.map(|home| home.join(rest))
    } else {
        Some(PathBuf::from(entry))
    }
}

/// Ensures an already-canonicalized `path` is inside one of the allowed roots.
fn ensure_under_roots(path: &std::path::Path, roots: &[PathBuf]) -> Result<(), AppError> {
    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(AppError {
            code: "PATH_NOT_ALLOWED".into(),
            message: "Path is outside the configured project roots.".into(),
        })
    }
}

/// T081: best-effort discovery of Claude's own session id for `cwd` — the newest
/// `~/.claude/projects/<cwd with '/'→'-'>/<uuid>.jsonl` modified after the session started.
/// Returns None on any error (Codex/OpenCode use other stores and are not covered here).
fn capture_claude_session_id(
    cwd: &std::path::Path,
    after: std::time::SystemTime,
) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let encoded = cwd.to_string_lossy().replace('/', "-");
    let dir = home.join(".claude").join("projects").join(encoded);
    let mut newest: Option<(std::time::SystemTime, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if modified < after {
            continue;
        }
        let Some(stem) = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
        else {
            continue;
        };
        if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
            newest = Some((modified, stem));
        }
    }
    newest.map(|(_, id)| id)
}

#[tauri::command]
pub async fn add_project_folder(
    path: String,
    state: State<'_, AppState>,
) -> Result<ProjectResponse, AppError> {
    let inspected =
        project_manager::inspect(std::path::Path::new(&path)).map_err(AppError::internal)?;
    ensure_under_roots(&inspected.path, &allowed_roots(&state)?)?;
    let record = ProjectRecord {
        id: ProjectId::new(),
        name: inspected.name,
        path: inspected.path.to_string_lossy().into_owned(),
        archived: false,
        display_name: None,
    };
    let response = project_summary(record.clone());
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || ProjectsDao(&database).insert(&record))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(ProjectResponse { project: response })
}
#[tauri::command]
pub async fn clone_project(
    url: String,
    dest_root: String,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProjectResponse, AppError> {
    let root = std::fs::canonicalize(&dest_root).map_err(AppError::internal)?;
    ensure_under_roots(&root, &allowed_roots(&state)?)?;
    let inferred = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("project")
        .trim_end_matches(".git");
    let name = name.unwrap_or_else(|| inferred.into());
    if name.contains('/') || name.contains("..") || name.is_empty() {
        return Err(AppError {
            code: "INVALID_NAME".into(),
            message: "Project name is invalid.".into(),
        });
    }
    let destination = root.join(name);
    let inspected =
        tokio::task::spawn_blocking(move || project_manager::clone_repository(&url, &destination))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?;
    let record = ProjectRecord {
        id: ProjectId::new(),
        name: inspected.name,
        path: inspected.path.to_string_lossy().into_owned(),
        archived: false,
        display_name: None,
    };
    let response = project_summary(record.clone());
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || ProjectsDao(&database).insert(&record))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(ProjectResponse { project: response })
}
#[tauri::command]
pub async fn remove_project(
    project_id: String,
    delete_files: Option<bool>,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    if delete_files.unwrap_or(false) {
        return Err(AppError {
            code: "DELETE_NOT_ALLOWED".into(),
            message: "Terminal AI never deletes project files.".into(),
        });
    }
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        database
            .connection()?
            .execute("DELETE FROM projects WHERE id=?1", [project_id])?;
        Ok::<(), terminal_ai_persistence::PersistenceError>(())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResponse {
    pub branch: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub worktrees: Vec<serde_json::Value>,
}
#[tauri::command]
pub async fn get_git_status(
    project_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<GitStatusResponse, AppError> {
    let path = state
        .database
        .connection()
        .map_err(AppError::internal)?
        .query_row(
            "SELECT path FROM projects WHERE id=?1",
            [&project_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(AppError::internal)?;
    let status =
        tokio::task::spawn_blocking(move || project_manager::status(std::path::Path::new(&path)))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?;
    let response = GitStatusResponse {
        branch: status.branch,
        dirty: status.dirty,
        ahead: status.ahead,
        behind: status.behind,
        worktrees: vec![],
    };
    let _ = app.emit(
        "git-status-changed",
        crate::events::GitStatusChanged {
            project_id,
            worktree_id: None,
            branch: response.branch.clone(),
            dirty: response.dirty,
            ahead: response.ahead,
            behind: response.behind,
        },
    );
    Ok(response)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub color: Option<String>,
    pub detected: bool,
    pub auth: String,
}
#[derive(Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderSummary>,
}
fn environment_map(state: &AppState) -> Result<BTreeMap<String, String>, AppError> {
    Ok(state
        .environment
        .read()
        .map_err(|_| AppError::internal("environment cache poisoned"))?
        .as_ref()
        .map(|value| value.env.clone())
        .unwrap_or_else(|| std::env::vars().collect()))
}
fn record_profile(record: &ProviderProfileRecord) -> ProviderProfile {
    ProviderProfile {
        id: ProviderId(record.id.clone()),
        label: record.label.clone(),
        command: PathBuf::from(&record.command),
        args: serde_json::from_value(record.args.clone()).unwrap_or_default(),
        env: serde_json::from_value::<BTreeMap<String, String>>(record.env.clone())
            .unwrap_or_default()
            .into_iter()
            .collect(),
        color: record.color.clone(),
        kind: if record.kind == "builtin" {
            ProviderKind::Builtin
        } else {
            ProviderKind::Custom
        },
    }
}
#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<ProvidersResponse, AppError> {
    let database = state.database.clone();
    let records = tokio::task::spawn_blocking(move || ProviderProfilesDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    let env = environment_map(&state)?;
    let providers = records
        .into_iter()
        .map(|record| {
            let adapter = ProviderAdapter::new(
                record_profile(&record),
                terminal_ai_domain::host::ResumeCapability::None,
            );
            let detection = adapter.detect(&env);
            ProviderSummary {
                id: record.id,
                label: record.label,
                kind: record.kind,
                color: record.color,
                detected: detection.detected,
                auth: format!("{:?}", detection.auth).to_lowercase(),
            }
        })
        .collect();
    Ok(ProvidersResponse { providers })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectProviderResponse {
    pub detected: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub auth: String,
    pub message: Option<String>,
}
#[tauri::command]
pub async fn detect_provider(
    provider_id: String,
    state: State<'_, AppState>,
) -> Result<DetectProviderResponse, AppError> {
    let database = state.database.clone();
    let id = provider_id.clone();
    let records = tokio::task::spawn_blocking(move || ProviderProfilesDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    let record = records
        .iter()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError {
            code: "PROVIDER_NOT_FOUND".into(),
            message: "Provider profile does not exist.".into(),
        })?;
    let adapter = ProviderAdapter::new(
        record_profile(record),
        terminal_ai_domain::host::ResumeCapability::None,
    );
    let detection = adapter.detect(&environment_map(&state)?);
    Ok(DetectProviderResponse {
        detected: detection.detected,
        path: detection
            .path
            .map(|path| path.to_string_lossy().into_owned()),
        version: detection.version,
        auth: format!("{:?}", detection.auth).to_lowercase(),
        message: detection.message,
    })
}
#[tauri::command]
pub async fn upsert_provider_profile(
    id: String,
    label: String,
    command: String,
    args: Vec<String>,
    color: Option<String>,
    env: Option<BTreeMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let profile = ProviderProfile::try_from(ProfileInput {
        id,
        label,
        command,
        args,
        color,
        env: env.unwrap_or_default(),
    })
    .map_err(AppError::internal)?;
    let adapter = ProviderAdapter::new(
        profile.clone(),
        terminal_ai_domain::host::ResumeCapability::None,
    );
    let detection = adapter.detect(&environment_map(&state)?);
    if !detection.detected {
        return Err(AppError {
            code: "PROVIDER_MISSING".into(),
            message: detection
                .message
                .unwrap_or_else(|| "Provider command was not found.".into()),
        });
    }
    let record = ProviderProfileRecord {
        id: profile.id.0.clone(),
        label: profile.label.clone(),
        command: profile.command.to_string_lossy().into_owned(),
        args: serde_json::json!(profile.args),
        env: serde_json::json!(profile.env.clone().into_iter().collect::<BTreeMap<_, _>>()),
        color: profile.color.clone(),
        kind: "custom".into(),
    };
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || ProviderProfilesDao(&database).upsert(&record))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    state
        .host
        .register_provider(adapter)
        .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}

#[tauri::command]
pub async fn get_usage(state: State<'_, AppState>) -> Result<UsageSnapshot, AppError> {
    // Prefer the poller's live snapshot (kept fresh by the autonomous loop + on-demand refresh).
    // The DB snapshot is only a fallback for the brief window on boot before the first poll,
    // because the background poller updates its cache but not the DB (only refresh_usage does).
    let live = state.usage.snapshot().await;
    if !live.providers.is_empty() {
        return Ok(live);
    }
    let database = state.database.clone();
    let rows = tokio::task::spawn_blocking(move || UsageSnapshotsDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    if rows.is_empty() {
        return Ok(live);
    }
    let updated_at = rows
        .iter()
        .filter_map(|row| chrono::DateTime::parse_from_rfc3339(&row.fetched_at).ok())
        .map(|date| date.with_timezone(&chrono::Utc))
        .max()
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);
    let offline = rows.iter().any(|row| row.stale);
    let providers = rows
        .into_iter()
        .filter_map(|row| {
            serde_json::from_value::<UsageCard>(row.snapshot)
                .ok()
                .map(|card| (row.provider_id, card))
        })
        .collect();
    Ok(UsageSnapshot {
        providers,
        updated_at,
        offline,
    })
}

#[tauri::command]
pub async fn refresh_usage(
    provider_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RefreshResult, AppError> {
    let (result, snapshot) = state
        .usage
        // A user click is an explicit action bounded by the ~60s cache, not the 300s poller floor.
        .refresh(provider_id.as_deref(), true)
        .await
        .map_err(AppError::internal)?;
    if result.scheduled {
        let database = state.database.clone();
        let records = snapshot
            .providers
            .iter()
            .map(
                |(provider_id, card)| -> Result<UsageSnapshotRecord, AppError> {
                    Ok(UsageSnapshotRecord {
                        provider_id: provider_id.clone(),
                        // T078: propagate a serialize error instead of persisting a null snapshot.
                        snapshot: serde_json::to_value(card).map_err(AppError::internal)?,
                        fetched_at: snapshot.updated_at.to_rfc3339(),
                        stale: card.stale,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        tokio::task::spawn_blocking(move || {
            let dao = UsageSnapshotsDao(&database);
            for record in records {
                dao.upsert(&record)?;
            }
            Ok::<_, terminal_ai_persistence::PersistenceError>(())
        })
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
        app.emit("usage-updated", &snapshot)
            .map_err(AppError::internal)?;
        for (provider_id, card) in &snapshot.providers {
            if matches!(card.auth, AuthState::Expired | AuthState::Rejected) {
                app.emit("provider-authentication-expired", provider_id)
                    .map_err(AppError::internal)?;
            }
        }
    }
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeResponse {
    pub worktree: worktree_manager::WorktreeSummary,
}

#[derive(Serialize)]
pub struct WorktreesResponse {
    pub worktrees: Vec<worktree_manager::WorktreeSummary>,
}

#[tauri::command]
pub async fn create_worktree(
    project_id: String,
    branch: String,
    create_branch: bool,
    state: State<'_, AppState>,
) -> Result<WorktreeResponse, AppError> {
    let database = state.database.clone();
    let project_id_for_task = project_id.clone();
    let worktree = tokio::task::spawn_blocking(move || {
        let connection = database.connection()?;
        let path: String = connection.query_row(
            "SELECT path FROM projects WHERE id=?1",
            [&project_id_for_task],
            |row| row.get(0),
        )?;
        drop(connection);
        worktree_manager::create(std::path::Path::new(&path), &branch, create_branch)
            .map_err(AppError::internal)
    })
    .await
    .map_err(AppError::internal)??;
    let record = WorktreeRecord {
        id: worktree.id.clone(),
        project_id,
        path: worktree.path.to_string_lossy().into_owned(),
        branch: worktree.branch.clone(),
        status: if worktree.dirty { "dirty" } else { "clean" }.into(),
    };
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || WorktreesDao(&database).upsert(&record))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(WorktreeResponse { worktree })
}

#[tauri::command]
pub async fn list_worktrees(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<WorktreesResponse, AppError> {
    let database = state.database.clone();
    let project_id_for_task = project_id.clone();
    let worktrees = tokio::task::spawn_blocking(move || {
        let path: String = database.connection()?.query_row(
            "SELECT path FROM projects WHERE id=?1",
            [&project_id_for_task],
            |row| row.get(0),
        )?;
        worktree_manager::list(std::path::Path::new(&path)).map_err(AppError::internal)
    })
    .await
    .map_err(AppError::internal)??;
    let records = worktrees
        .iter()
        .map(|worktree| WorktreeRecord {
            id: worktree.id.clone(),
            project_id: project_id.clone(),
            path: worktree.path.to_string_lossy().into_owned(),
            branch: worktree.branch.clone(),
            status: if worktree.dirty { "dirty" } else { "clean" }.into(),
        })
        .collect::<Vec<_>>();
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        let dao = WorktreesDao(&database);
        for record in records {
            dao.upsert(&record)?;
        }
        Ok::<_, terminal_ai_persistence::PersistenceError>(())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(WorktreesResponse { worktrees })
}

#[tauri::command]
pub async fn remove_worktree(
    worktree_id: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    if state
        .host
        .list()
        .await
        .map_err(AppError::internal)?
        .iter()
        .any(|session| {
            session
                .worktree_id
                .as_ref()
                .is_some_and(|id| id.0 == worktree_id)
        })
    {
        return Err(AppError {
            code: "WORKTREE_IN_USE".into(),
            message: "Close sessions using this worktree before removing it.".into(),
        });
    }
    let database = state.database.clone();
    let id = worktree_id.clone();
    tokio::task::spawn_blocking(move || {
        let record = WorktreesDao(&database).get(&id)?.ok_or_else(|| AppError {
            code: "WORKTREE_NOT_FOUND".into(),
            message: "Worktree does not exist.".into(),
        })?;
        let project_path: String = database.connection()?.query_row(
            "SELECT path FROM projects WHERE id=?1",
            [&record.project_id],
            |row| row.get(0),
        )?;
        worktree_manager::remove(std::path::Path::new(&project_path), &record.id)
            .map_err(AppError::internal)?;
        WorktreesDao(&database).delete(&record.id)?;
        Ok::<_, AppError>(())
    })
    .await
    .map_err(AppError::internal)??;
    Ok(OkResponse { ok: true })
}

#[derive(Serialize)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct PresetsResponse {
    pub presets: Vec<PresetSummary>,
}

#[tauri::command]
pub async fn list_presets(state: State<'_, AppState>) -> Result<PresetsResponse, AppError> {
    let database = state.database.clone();
    let presets = tokio::task::spawn_blocking(move || LayoutPresetsDao(&database).list())
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(PresetsResponse {
        presets: presets
            .into_iter()
            .map(|preset| PresetSummary {
                id: preset.id,
                name: preset.name,
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSaved {
    pub preset_id: String,
}

#[tauri::command]
pub async fn save_preset(
    name: String,
    layout: LayoutNode,
    pane_providers: BTreeMap<String, String>,
    state: State<'_, AppState>,
) -> Result<PresetSaved, AppError> {
    let name = name.trim().to_owned();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError {
            code: "INVALID_PRESET_NAME".into(),
            message: "Preset name must contain 1–80 characters.".into(),
        });
    }
    layout.validate().map_err(AppError::internal)?;
    let pane_ids = layout_pane_ids(&layout);
    if pane_providers.keys().any(|id| !pane_ids.contains(id)) {
        return Err(AppError {
            code: "INVALID_PANE_BINDING".into(),
            message: "Preset provider does not belong to this layout.".into(),
        });
    }
    let database = state.database.clone();
    let preset = LayoutPresetRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        layout,
        pane_providers,
    };
    let preset_id = tokio::task::spawn_blocking(move || {
        let dao = LayoutPresetsDao(&database);
        dao.save(&preset)?;
        dao.list()?
            .into_iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| candidate.id)
            .ok_or_else(|| AppError::internal("saved preset missing"))
    })
    .await
    .map_err(AppError::internal)??;
    Ok(PresetSaved { preset_id })
}

#[tauri::command]
pub async fn create_workspace_from_preset(
    preset_id: String,
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<WorkspaceCreated, AppError> {
    let database = state.database.clone();
    let workspace_id = tokio::task::spawn_blocking(move || {
        let preset = LayoutPresetsDao(&database)
            .get(&preset_id)?
            .ok_or_else(|| AppError {
                code: "PRESET_NOT_FOUND".into(),
                message: "Layout preset does not exist.".into(),
            })?;
        let project = project_id.map(ProjectId);
        let workspace_id =
            WorkspacesDao(&database).create(&preset.name, project.as_ref(), None)?;
        let mut connection = database.connection()?;
        let transaction = connection.transaction()?;
        let layout_id = uuid::Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO workspace_layouts(id,workspace_id,layout_json,updated_at)VALUES(?1,?2,?3,?4)",
            rusqlite::params![layout_id, workspace_id.0, serde_json::to_string(&preset.layout).map_err(AppError::internal)?, chrono::Utc::now().to_rfc3339()],
        )?;
        transaction.execute(
            "UPDATE workspaces SET active_layout_id=?2 WHERE id=?1",
            rusqlite::params![workspace_id.0, layout_id],
        )?;
        for pane_id in layout_pane_ids(&preset.layout) {
            transaction.execute("INSERT INTO panes(id,workspace_id,pane_key,provider_id,project_id,created_at)VALUES(?1,?2,?3,?4,?5,?6)", rusqlite::params![uuid::Uuid::new_v4().to_string(),workspace_id.0,pane_id,preset.pane_providers.get(&pane_id),project.as_ref().map(|id|&id.0),chrono::Utc::now().to_rfc3339()])?;
        }
        transaction.commit()?;
        Ok::<_, AppError>(workspace_id.0)
    })
    .await
    .map_err(AppError::internal)??;
    Ok(WorkspaceCreated { workspace_id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsResponse {
    pub skills: Vec<SkillRecord>,
    pub bindings: Vec<SkillBindingRecord>,
}

#[tauri::command]
pub async fn list_skills(
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<SkillsResponse, AppError> {
    let root = state.skills_root.clone();
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || {
        let dao = SkillsDao(&database);
        // The app's own library, the user's global Claude skills, and — only — the selected
        // project's `.claude/skills`. Scanning every known project turned this list into a wall
        // of a hundred-plus skills belonging to repositories the user is not working in.
        let mut roots = vec![root.clone()];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".claude").join("skills"));
        }
        if let Some(project_id) = &project_id {
            let connection = database.connection()?;
            let path: Option<String> = connection
                .query_row(
                    "SELECT path FROM projects WHERE id=?1",
                    [project_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(path) = path {
                roots.push(PathBuf::from(path).join(".claude").join("skills"));
            }
        }
        std::fs::create_dir_all(&root).ok();
        for skill in skill_manager::discover(&roots) {
            dao.upsert(&SkillRecord {
                id: skill.id,
                slug: skill.slug,
                name: skill.name,
                version: skill.version,
                description: skill.description,
                providers: skill.providers,
                content_path: skill.instructions_path.to_string_lossy().into_owned(),
            })?;
        }
        // Discovery only ever upserts, so rows linger for skills that have since moved, been
        // deleted, or belong to a project no longer in scope. Reconcile at read time against the
        // roots actually scanned, the same way the project list does.
        let skills = dao
            .list()?
            .into_iter()
            .filter(|skill| {
                let path = PathBuf::from(&skill.content_path);
                path.exists() && roots.iter().any(|root| path.starts_with(root))
            })
            .collect();
        Ok::<_, AppError>(SkillsResponse {
            skills,
            bindings: dao.list_bindings()?,
        })
    })
    .await
    .map_err(AppError::internal)?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewResponse {
    pub diff: String,
    pub will_create: Vec<String>,
}

fn skill_scope_name(level: ScopeLevel) -> &'static str {
    match level {
        ScopeLevel::Global => "global",
        ScopeLevel::Project => "project",
        ScopeLevel::Worktree => "worktree",
        ScopeLevel::Workspace => "workspace",
        ScopeLevel::Session => "session",
    }
}

fn skill_target_root(state: &AppState, scope: &skill_manager::Scope) -> Result<PathBuf, AppError> {
    scope.validate().map_err(AppError::internal)?;
    if scope.level == ScopeLevel::Global {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| AppError::internal("home directory unavailable"));
    }
    let id = scope.ref_id.as_deref().unwrap_or_default();
    let connection = state.database.connection()?;
    let path: String = match scope.level {
        ScopeLevel::Project => connection.query_row(
            "SELECT path FROM projects WHERE id=?1",
            [id],
            |row| row.get(0),
        )?,
        ScopeLevel::Worktree => connection.query_row(
            "SELECT path FROM worktrees WHERE id=?1",
            [id],
            |row| row.get(0),
        )?,
        ScopeLevel::Workspace => connection.query_row("SELECT coalesce(wt.path,p.path) FROM workspaces ws LEFT JOIN worktrees wt ON wt.id=ws.worktree_id LEFT JOIN projects p ON p.id=ws.project_id WHERE ws.id=?1",[id],|row|row.get(0))?,
        ScopeLevel::Session => connection.query_row(
            "SELECT cwd FROM terminal_sessions WHERE id=?1",
            [id],
            |row| row.get(0),
        )?,
        ScopeLevel::Global => {
            return Err(AppError {
                code: "INVALID_SCOPE".into(),
                message: "Global scope has no filesystem path.".into(),
            })
        }
    };
    Ok(PathBuf::from(path))
}

/// Resolves a skill by id across every root the listing can surface — the app's own library,
/// the user's global `~/.claude/skills`, and each project's `.claude/skills`. Looking only in
/// `skills_root` meant most of what the panel listed could not be previewed or applied at all:
/// the list and the actions disagreed about what a skill is. Lookup is deliberately broader
/// than the *display* scope, which stays narrow to keep the list readable.
fn find_skill(state: &AppState, id: &str) -> Result<skill_manager::Skill, AppError> {
    let mut roots = vec![state.skills_root.clone()];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".claude").join("skills"));
    }
    if let Ok(connection) = state.database.connection() {
        if let Ok(mut statement) = connection.prepare("SELECT path FROM projects") {
            if let Ok(paths) = statement.query_map([], |row| row.get::<_, String>(0)) {
                for path in paths.flatten() {
                    roots.push(PathBuf::from(path).join(".claude").join("skills"));
                }
            }
        }
    }
    skill_manager::discover(&roots)
        .into_iter()
        .find(|skill| skill.id == id)
        .ok_or_else(|| AppError {
            code: "SKILL_NOT_FOUND".into(),
            message: "Skill not found in the app library, ~/.claude/skills, or any project.".into(),
        })
}

#[tauri::command]
pub async fn preview_skill_apply(
    skill_id: String,
    provider_id: String,
    scope: skill_manager::Scope,
    state: State<'_, AppState>,
) -> Result<SkillPreviewResponse, AppError> {
    let skill = find_skill(&state, &skill_id)?;
    let target = skill_target_root(&state, &scope)?;
    let preview = skill_manager::preview(&skill, &provider_id, &scope, &target)
        .map_err(AppError::internal)?;
    Ok(SkillPreviewResponse {
        diff: preview.diff,
        will_create: preview
            .will_create
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    })
}

#[derive(Serialize)]
pub struct CreatedArtifactsResponse {
    pub created: Vec<String>,
}

#[tauri::command]
pub async fn apply_skill(
    skill_id: String,
    provider_id: String,
    scope: skill_manager::Scope,
    state: State<'_, AppState>,
) -> Result<CreatedArtifactsResponse, AppError> {
    let skill = find_skill(&state, &skill_id)?;
    let target = skill_target_root(&state, &scope)?;
    let preview = skill_manager::preview(&skill, &provider_id, &scope, &target)
        .map_err(AppError::internal)?;
    let artifact = skill_manager::apply(&preview).map_err(AppError::internal)?;
    let database = state.database.clone();
    let scope_name = skill_scope_name(scope.level).to_owned();
    let scope_ref = scope.ref_id.clone();
    let skill_for_record = skill_id.clone();
    let artifact_for_record = artifact.clone();
    tokio::task::spawn_blocking(move || {
        let dao = SkillsDao(&database);
        let mut artifacts = dao
            .binding(&skill_for_record, &scope_name, scope_ref.as_deref())?
            .and_then(|binding| {
                serde_json::from_value::<Vec<skill_manager::AppliedArtifact>>(
                    binding.applied_artifacts,
                )
                .ok()
            })
            .unwrap_or_default();
        artifacts.retain(|existing| existing.provider_id != artifact_for_record.provider_id);
        artifacts.push(artifact_for_record);
        dao.set_binding(&SkillBindingRecord {
            id: uuid::Uuid::new_v4().to_string(),
            skill_id: skill_for_record,
            scope: scope_name,
            scope_ref_id: scope_ref,
            enabled: true,
            precedence: scope.precedence(),
            applied_artifacts: serde_json::to_value(artifacts).map_err(AppError::internal)?,
        })?;
        Ok::<_, AppError>(())
    })
    .await
    .map_err(AppError::internal)??;
    Ok(CreatedArtifactsResponse {
        created: vec![artifact.path.to_string_lossy().into_owned()],
    })
}

#[derive(Serialize)]
pub struct RemovedArtifactsResponse {
    pub removed: Vec<String>,
}

#[tauri::command]
pub async fn remove_skill(
    skill_id: String,
    provider_id: String,
    scope: skill_manager::Scope,
    state: State<'_, AppState>,
) -> Result<RemovedArtifactsResponse, AppError> {
    let scope_name = skill_scope_name(scope.level).to_owned();
    let database = state.database.clone();
    let binding = SkillsDao(&database)
        .binding(&skill_id, &scope_name, scope.ref_id.as_deref())?
        .ok_or_else(|| AppError {
            code: "SKILL_BINDING_NOT_FOUND".into(),
            message: "Skill is not applied in this scope.".into(),
        })?;
    let mut artifacts: Vec<skill_manager::AppliedArtifact> =
        serde_json::from_value(binding.applied_artifacts.clone()).map_err(AppError::internal)?;
    let mut removed = Vec::new();
    for artifact in artifacts
        .iter()
        .filter(|artifact| artifact.provider_id == provider_id)
    {
        if skill_manager::remove(artifact).map_err(AppError::internal)? {
            removed.push(artifact.path.to_string_lossy().into_owned());
        }
    }
    artifacts.retain(|artifact| artifact.provider_id != provider_id);
    SkillsDao(&database).set_binding(&SkillBindingRecord {
        applied_artifacts: serde_json::to_value(&artifacts).map_err(AppError::internal)?,
        enabled: !artifacts.is_empty(),
        ..binding
    })?;
    Ok(RemovedArtifactsResponse { removed })
}

/// Deletes a skill from the app's own library. Refuses anything outside `skills_root`: the
/// listing also discovers skills from `~/.claude/skills` and each project's `.claude/skills`,
/// and those files belong to the user's other tools — removing them here would be exactly the
/// blind overwrite Principle III forbids.
#[tauri::command]
pub async fn delete_skill(
    skill_id: String,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    let root = state.skills_root.clone();
    let database = state.database.clone();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let dao = SkillsDao(&database);
        let record = dao
            .list()
            .map_err(AppError::internal)?
            .into_iter()
            .find(|skill| skill.id == skill_id)
            .ok_or_else(|| AppError {
                code: "SKILL_NOT_FOUND".into(),
                message: "Skill not found.".into(),
            })?;
        let content = PathBuf::from(&record.content_path);
        let directory = content.parent().unwrap_or(&content).to_path_buf();
        let inside = directory
            .canonicalize()
            .ok()
            .zip(root.canonicalize().ok())
            .is_some_and(|(dir, root)| dir.starts_with(&root) && dir != root);
        if !inside {
            return Err(AppError {
                code: "SKILL_NOT_OWNED".into(),
                message: format!(
                    "This skill lives in {} and is managed by another tool. Delete it there.",
                    directory.display()
                ),
            });
        }
        // Take the applied artifacts down first. Deleting the skill without them left files
        // behind in every agent with no owner left to manage them — orphans the UI could no
        // longer reach. `remove` checks its own marker, so it still only deletes what the app
        // wrote (Principle III).
        for binding in dao.list_bindings().map_err(AppError::internal)? {
            if binding.skill_id != record.id {
                continue;
            }
            let artifacts: Vec<skill_manager::AppliedArtifact> =
                serde_json::from_value(binding.applied_artifacts).unwrap_or_default();
            for artifact in artifacts {
                skill_manager::remove(&artifact).map_err(AppError::internal)?;
            }
        }
        std::fs::remove_dir_all(&directory).map_err(AppError::internal)?;
        dao.delete(&record.id).map_err(AppError::internal)?;
        Ok(())
    })
    .await
    .map_err(AppError::internal)??;
    Ok(OkResponse { ok: true })
}

#[tauri::command]
pub async fn set_skill_binding(
    skill_id: String,
    scope: skill_manager::Scope,
    active: bool,
    state: State<'_, AppState>,
) -> Result<OkResponse, AppError> {
    scope.validate().map_err(AppError::internal)?;
    let scope_name = skill_scope_name(scope.level).to_owned();
    let precedence = scope.precedence();
    let dao = SkillsDao(&state.database);
    let previous = dao.binding(&skill_id, &scope_name, scope.ref_id.as_deref())?;
    dao.set_binding(&SkillBindingRecord {
        id: previous
            .as_ref()
            .map(|binding| binding.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        skill_id,
        scope: scope_name,
        scope_ref_id: scope.ref_id,
        enabled: active,
        precedence,
        applied_artifacts: previous
            .map(|binding| binding.applied_artifacts)
            .unwrap_or_else(|| serde_json::json!([])),
    })?;
    Ok(OkResponse { ok: true })
}

#[derive(Serialize)]
pub struct MemoryEntriesResponse {
    pub entries: Vec<memory_manager::MemoryEntry>,
}

#[tauri::command]
pub async fn list_memory(
    scope: Scope,
    state: State<'_, AppState>,
) -> Result<MemoryEntriesResponse, AppError> {
    let database = state.database.clone();
    let root = state.memory_root.clone();
    let entries =
        tokio::task::spawn_blocking(move || memory_manager::list(&database, &scope, &root))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?;
    Ok(MemoryEntriesResponse { entries })
}

#[tauri::command]
pub async fn search_memory(
    query: String,
    scope: Option<Scope>,
    state: State<'_, AppState>,
) -> Result<MemoryEntriesResponse, AppError> {
    let database = state.database.clone();
    let root = state.memory_root.clone();
    let entries = tokio::task::spawn_blocking(move || {
        memory_manager::search(&database, &root, &query, scope.as_ref())
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(MemoryEntriesResponse { entries })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCreatedResponse {
    pub entry_id: String,
}

#[tauri::command]
pub async fn add_memory(
    scope: Scope,
    r#type: MemoryType,
    title: String,
    body: String,
    state: State<'_, AppState>,
) -> Result<MemoryCreatedResponse, AppError> {
    let database = state.database.clone();
    let root = state.memory_root.clone();
    let entry_id = tokio::task::spawn_blocking(move || {
        memory_manager::add(&database, &root, &scope, r#type, &title, &body)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(MemoryCreatedResponse { entry_id })
}

#[tauri::command]
pub async fn capture_selection_to_memory(
    session_id: String,
    text: String,
    scope: Scope,
    r#type: MemoryType,
    state: State<'_, AppState>,
) -> Result<MemoryCreatedResponse, AppError> {
    if text.trim().is_empty() {
        return Err(AppError {
            code: "EMPTY_SELECTION".into(),
            message: "Select terminal text before saving memory.".into(),
        });
    }
    let (project_id, worktree_id): (Option<String>, Option<String>) = {
        let connection = state.database.connection()?;
        connection.query_row(
            "SELECT project_id,worktree_id FROM terminal_sessions WHERE id=?1",
            [&session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
    };
    let allowed = match scope.level {
        ScopeLevel::Global => true,
        ScopeLevel::Project => scope.ref_id == project_id,
        ScopeLevel::Worktree => scope.ref_id == worktree_id,
        ScopeLevel::Session => scope.ref_id.as_deref() == Some(&session_id),
        ScopeLevel::Workspace => true,
    };
    if !allowed {
        return Err(AppError {
            code: "MEMORY_SCOPE_MISMATCH".into(),
            message: "The selected session does not belong to that memory scope.".into(),
        });
    }
    let title = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| crate::events::sanitize_title(line.trim()))
        .unwrap_or_else(|| "Terminal selection".into());
    add_memory(scope, r#type, title, text, state).await
}

#[derive(Serialize)]
pub struct MemoryContextResponse {
    pub composed: String,
    pub sources: Vec<memory_manager::MemorySource>,
}

#[tauri::command]
pub async fn preview_memory_context(
    scope: Scope,
    state: State<'_, AppState>,
) -> Result<MemoryContextResponse, AppError> {
    let database = state.database.clone();
    let root = state.memory_root.clone();
    let (composed, sources) = tokio::task::spawn_blocking(move || {
        memory_manager::preview_context(&database, &root, &scope)
    })
    .await
    .map_err(AppError::internal)?
    .map_err(AppError::internal)?;
    Ok(MemoryContextResponse { composed, sources })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub project_roots: Vec<String>,
    pub keybindings: BTreeMap<String, String>,
    pub scrollback_lines: u32,
    pub memory_auto_capture: bool,
    pub usage_refresh_seconds: u64,
}

#[derive(Serialize)]
pub struct SettingsResponse {
    pub settings: AppSettings,
}

fn read_settings(database: &terminal_ai_persistence::Database) -> Result<AppSettings, AppError> {
    let dao = SettingsDao(database);
    let mut keybindings: BTreeMap<String, String> =
        serde_json::from_value(dao.get("keybindings")?.unwrap_or_else(default_keybindings))
            .unwrap_or_default();
    if keybindings.is_empty() {
        keybindings = serde_json::from_value(default_keybindings()).unwrap_or_default();
    }
    Ok(AppSettings {
        project_roots: serde_json::from_value(
            dao.get("project_root_dirs")?
                .unwrap_or(serde_json::json!(["~/www"])),
        )
        .unwrap_or_else(|_| vec!["~/www".into()]),
        keybindings,
        scrollback_lines: serde_json::from_value(
            dao.get("scrollback_lines")?
                .unwrap_or(serde_json::json!(10_000)),
        )
        .unwrap_or(10_000),
        memory_auto_capture: serde_json::from_value(
            dao.get("memory_auto_capture")?
                .unwrap_or(serde_json::json!(false)),
        )
        .unwrap_or(false),
        usage_refresh_seconds: serde_json::from_value(
            dao.get("usage_refresh_seconds")?
                .unwrap_or(serde_json::json!(300)),
        )
        .unwrap_or(300),
    })
}

fn default_keybindings() -> serde_json::Value {
    serde_json::json!({
        "newWorkspace": "Meta+N",
        "splitRight": "Meta+D",
        "splitDown": "Meta+Shift+D",
        "maximizePane": "Meta+Shift+Enter",
        "focusLeft": "Meta+Shift+ArrowLeft",
        "focusRight": "Meta+Shift+ArrowRight",
        "focusUp": "Meta+Shift+ArrowUp",
        "focusDown": "Meta+Shift+ArrowDown"
    })
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsResponse, AppError> {
    Ok(SettingsResponse {
        settings: read_settings(&state.database)?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub project_roots: Option<Vec<String>>,
    pub keybindings: Option<BTreeMap<String, String>>,
    pub scrollback_lines: Option<u32>,
    pub memory_auto_capture: Option<bool>,
    pub usage_refresh_seconds: Option<u64>,
}

#[tauri::command]
pub async fn set_settings(
    patch: SettingsPatch,
    state: State<'_, AppState>,
) -> Result<SettingsResponse, AppError> {
    let mut settings = read_settings(&state.database)?;
    if let Some(roots) = patch.project_roots {
        if roots.is_empty()
            || roots.iter().any(|root| {
                let expanded = root.strip_prefix("~/").and_then(|suffix| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(suffix))
                });
                !expanded.unwrap_or_else(|| PathBuf::from(root)).is_dir()
            })
        {
            return Err(AppError {
                code: "INVALID_PROJECT_ROOT".into(),
                message: "Every project root must be an existing directory.".into(),
            });
        }
        settings.project_roots = roots;
    }
    if let Some(keybindings) = patch.keybindings {
        if keybindings.len() > 50
            || keybindings
                .iter()
                .any(|(action, shortcut)| action.len() > 64 || shortcut.len() > 64)
        {
            return Err(AppError {
                code: "INVALID_KEYBINDINGS".into(),
                message: "Keybinding map is too large.".into(),
            });
        }
        settings.keybindings = keybindings;
    }
    if let Some(lines) = patch.scrollback_lines {
        if !(1_000..=100_000).contains(&lines) {
            return Err(AppError {
                code: "INVALID_SCROLLBACK".into(),
                message: "Scrollback must be between 1,000 and 100,000 lines.".into(),
            });
        }
        settings.scrollback_lines = lines;
    }
    if let Some(enabled) = patch.memory_auto_capture {
        settings.memory_auto_capture = enabled;
    }
    if let Some(seconds) = patch.usage_refresh_seconds {
        if seconds < 300 {
            return Err(AppError {
                code: "INVALID_USAGE_INTERVAL".into(),
                message: "Usage refresh cannot be lower than 300 seconds.".into(),
            });
        }
        settings.usage_refresh_seconds = seconds;
    }
    let dao = SettingsDao(&state.database);
    dao.set(
        "project_root_dirs",
        &serde_json::json!(&settings.project_roots),
    )?;
    dao.set("keybindings", &serde_json::json!(&settings.keybindings))?;
    dao.set(
        "scrollback_lines",
        &serde_json::json!(settings.scrollback_lines),
    )?;
    dao.set(
        "memory_auto_capture",
        &serde_json::json!(settings.memory_auto_capture),
    )?;
    dao.set(
        "usage_refresh_seconds",
        &serde_json::json!(settings.usage_refresh_seconds),
    )?;
    Ok(SettingsResponse { settings })
}

#[tauri::command]
pub async fn notify(title: String, body: String) -> Result<OkResponse, AppError> {
    let title = crate::events::sanitize_title(&title);
    let body = crate::events::sanitize_title(&body);
    tokio::task::spawn_blocking(move || terminal_ai_platform_macos::notify(&title, &body))
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(OkResponse { ok: true })
}
