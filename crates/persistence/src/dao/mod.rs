use crate::{Database, PersistenceError};
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use terminal_ai_domain::{LayoutNode, ProjectId, WorkspaceId, WorktreeId};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: ProjectId,
    pub name: String,
    pub path: String,
    pub archived: bool,
    /// User-chosen name; `None` falls back to the directory name in `name`.
    pub display_name: Option<String>,
}

pub struct ProjectsDao<'a>(pub &'a Database);
impl ProjectsDao<'_> {
    pub fn list(&self) -> Result<Vec<ProjectRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id,name,path,archived_at,display_name FROM projects ORDER BY coalesce(last_opened_at,created_at) DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectRecord {
                id: ProjectId(row.get(0)?),
                name: row.get(1)?,
                path: row.get(2)?,
                archived: row.get::<_, Option<String>>(3)?.is_some(),
                display_name: row.get::<_, Option<String>>(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    /// Archives or restores a project. Archiving keeps the row — discovery will keep finding the
    /// directory — so the flag is what hides it, and restoring is lossless.
    pub fn set_archived(&self, id: &ProjectId, archived: bool) -> Result<(), PersistenceError> {
        self.0.connection()?.execute(
            "UPDATE projects SET archived_at=?2 WHERE id=?1",
            rusqlite::params![id.0, archived.then(|| Utc::now().to_rfc3339())],
        )?;
        Ok(())
    }
    /// Sets the user-chosen name, or clears it back to the directory name with `None`.
    pub fn set_display_name(
        &self,
        id: &ProjectId,
        name: Option<&str>,
    ) -> Result<(), PersistenceError> {
        self.0.connection()?.execute(
            "UPDATE projects SET display_name=?2 WHERE id=?1",
            rusqlite::params![id.0, name],
        )?;
        Ok(())
    }
    pub fn insert(&self, record: &ProjectRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO projects(id,name,path,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET name=excluded.name", rusqlite::params![record.id.0,record.name,record.path,Utc::now().to_rfc3339()])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRecord {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub branch: String,
    pub status: String,
}

pub struct WorktreesDao<'a>(pub &'a Database);
impl WorktreesDao<'_> {
    pub fn list(&self, project_id: &str) -> Result<Vec<WorktreeRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id,project_id,path,branch,coalesce(status,'clean') FROM worktrees WHERE project_id=?1 ORDER BY branch",
        )?;
        let rows = stmt.query_map([project_id], |row| {
            Ok(WorktreeRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                path: row.get(2)?,
                branch: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get(&self, id: &str) -> Result<Option<WorktreeRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let result = conn.query_row(
            "SELECT id,project_id,path,branch,coalesce(status,'clean') FROM worktrees WHERE id=?1",
            [id],
            |row| {
                Ok(WorktreeRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    path: row.get(2)?,
                    branch: row.get(3)?,
                    status: row.get(4)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn upsert(&self, record: &WorktreeRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO worktrees(id,project_id,path,branch,status,created_at)VALUES(?1,?2,?3,?4,?5,?6)ON CONFLICT(id)DO UPDATE SET path=excluded.path,branch=excluded.branch,status=excluded.status", rusqlite::params![record.id,record.project_id,record.path,record.branch,record.status,Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), PersistenceError> {
        self.0
            .connection()?
            .execute("DELETE FROM worktrees WHERE id=?1", [id])?;
        Ok(())
    }
}

pub struct WorkspacesDao<'a>(pub &'a Database);
impl WorkspacesDao<'_> {
    pub fn create(
        &self,
        title: &str,
        project_id: Option<&ProjectId>,
        worktree_id: Option<&WorktreeId>,
    ) -> Result<WorkspaceId, PersistenceError> {
        let id = WorkspaceId::new();
        self.0.connection()?.execute(
            "INSERT INTO workspaces(id,project_id,worktree_id,title,created_at) VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![
                id.0,
                project_id.map(|x| &x.0),
                worktree_id.map(|x| &x.0),
                title,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(id)
    }
    #[allow(clippy::type_complexity)]
    pub fn list(
        &self,
    ) -> Result<Vec<(WorkspaceId, String, Option<ProjectId>, Option<String>)>, PersistenceError>
    {
        let conn = self.0.connection()?;
        let mut stmt =
            conn.prepare("SELECT id,title,project_id,root_path FROM workspaces ORDER BY position")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                WorkspaceId(r.get(0)?),
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.map(ProjectId),
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub fn set_title(&self, id: &WorkspaceId, title: &str) -> Result<(), PersistenceError> {
        self.0.connection()?.execute(
            "UPDATE workspaces SET title=?2 WHERE id=?1",
            rusqlite::params![id.0, title],
        )?;
        Ok(())
    }
    /// The folder whose repositories this workspace lists, if it has its own.
    pub fn root_path(&self, id: &WorkspaceId) -> Result<Option<String>, PersistenceError> {
        Ok(self
            .0
            .connection()?
            .query_row(
                "SELECT root_path FROM workspaces WHERE id=?1",
                [&id.0],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }
    /// Every workspace-scoped root, so the allowed-path check covers them too.
    pub fn all_root_paths(&self) -> Result<Vec<String>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt =
            conn.prepare("SELECT root_path FROM workspaces WHERE root_path IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub fn set_root_path(
        &self,
        id: &WorkspaceId,
        path: Option<&str>,
    ) -> Result<(), PersistenceError> {
        self.0.connection()?.execute(
            "UPDATE workspaces SET root_path=?2 WHERE id=?1",
            rusqlite::params![id.0, path],
        )?;
        Ok(())
    }
}

pub struct LayoutsDao<'a>(pub &'a Database);
impl LayoutsDao<'_> {
    pub fn save(
        &self,
        workspace_id: &WorkspaceId,
        layout: &LayoutNode,
    ) -> Result<(), PersistenceError> {
        layout.validate().map_err(|e| {
            PersistenceError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;
        self.0.connection()?.execute("INSERT INTO workspace_layouts(id,workspace_id,layout_json,updated_at) VALUES(?1,?2,?3,?4)",rusqlite::params![Uuid::new_v4().to_string(),workspace_id.0,serde_json::to_string(layout)?,Utc::now().to_rfc3339()])?;
        Ok(())
    }
    pub fn load(&self, workspace_id: &WorkspaceId) -> Result<Option<LayoutNode>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt=conn.prepare("SELECT layout_json FROM workspace_layouts WHERE workspace_id=?1 ORDER BY updated_at DESC LIMIT 1")?;
        let result = stmt.query_row([&workspace_id.0], |r| r.get::<_, String>(0));
        match result {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPresetRecord {
    pub id: String,
    pub name: String,
    pub layout: LayoutNode,
    pub pane_providers: std::collections::BTreeMap<String, String>,
}

pub struct LayoutPresetsDao<'a>(pub &'a Database);
impl LayoutPresetsDao<'_> {
    pub fn ensure_seeded(&self) -> Result<(), PersistenceError> {
        for (id, name, layout) in seed_presets() {
            let payload = serde_json::json!({"layout": layout, "paneProviders": {}});
            self.0.connection()?.execute(
                "INSERT OR IGNORE INTO layout_presets(id,name,layout_json,created_at)VALUES(?1,?2,?3,?4)",
                rusqlite::params![id, name, serde_json::to_string(&payload)?, Utc::now().to_rfc3339()],
            )?;
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<LayoutPresetRecord>, PersistenceError> {
        self.ensure_seeded()?;
        let conn = self.0.connection()?;
        let mut stmt =
            conn.prepare("SELECT id,name,layout_json FROM layout_presets ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            let value: serde_json::Value = serde_json::from_str(&row.get::<_, String>(2)?)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(LayoutPresetRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                layout: serde_json::from_value(value["layout"].clone()).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                pane_providers: serde_json::from_value(value["paneProviders"].clone())
                    .unwrap_or_default(),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get(&self, id: &str) -> Result<Option<LayoutPresetRecord>, PersistenceError> {
        Ok(self.list()?.into_iter().find(|preset| preset.id == id))
    }

    pub fn save(&self, preset: &LayoutPresetRecord) -> Result<(), PersistenceError> {
        preset.layout.validate().map_err(|error| {
            PersistenceError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
        })?;
        let payload =
            serde_json::json!({"layout": preset.layout, "paneProviders": preset.pane_providers});
        self.0.connection()?.execute("INSERT INTO layout_presets(id,name,layout_json,created_at)VALUES(?1,?2,?3,?4)ON CONFLICT(name)DO UPDATE SET layout_json=excluded.layout_json", rusqlite::params![preset.id,preset.name,serde_json::to_string(&payload)?,Utc::now().to_rfc3339()])?;
        Ok(())
    }
}

fn seed_presets() -> Vec<(&'static str, &'static str, LayoutNode)> {
    use terminal_ai_domain::{PaneId, SplitDirection};
    let pane = |id: &str| LayoutNode::Pane {
        pane_id: PaneId(id.into()),
    };
    let pair = |prefix: &str, direction| LayoutNode::Split {
        direction,
        sizes: vec![50.0, 50.0],
        children: vec![pane(&format!("{prefix}-1")), pane(&format!("{prefix}-2"))],
    };
    vec![
        (
            "preset-review",
            "Review",
            pair("review", SplitDirection::Horizontal),
        ),
        (
            "preset-implementation",
            "Implementation",
            pair("implementation", SplitDirection::Vertical),
        ),
        (
            "preset-debug",
            "Debug",
            pair("debug", SplitDirection::Horizontal),
        ),
        (
            "preset-multi-agent",
            "Multi-agent",
            LayoutNode::Split {
                direction: SplitDirection::Vertical,
                sizes: vec![50.0, 50.0],
                children: vec![
                    pair("multi-top", SplitDirection::Horizontal),
                    pair("multi-bottom", SplitDirection::Horizontal),
                ],
            },
        ),
    ]
}

pub struct SettingsDao<'a>(pub &'a Database);
impl SettingsDao<'_> {
    pub fn get(&self, key: &str) -> Result<Option<serde_json::Value>, PersistenceError> {
        let conn = self.0.connection()?;
        let result = conn.query_row(
            "SELECT value_json FROM app_settings WHERE key=?1",
            [key],
            |r| r.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(serde_json::from_str(&v)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn set(&self, key: &str, value: &serde_json::Value) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO app_settings(key,value_json)VALUES(?1,?2)ON CONFLICT(key)DO UPDATE SET value_json=excluded.value_json",rusqlite::params![key,serde_json::to_string(value)?])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneRecord {
    pub id: String,
    pub workspace_id: String,
    pub pane_key: String,
    pub provider_id: Option<String>,
    pub project_id: Option<String>,
    pub worktree_id: Option<String>,
    pub title: Option<String>,
}
pub struct PanesDao<'a>(pub &'a Database);
impl PanesDao<'_> {
    pub fn upsert(&self, pane: &PaneRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO panes(id,workspace_id,pane_key,provider_id,project_id,worktree_id,title,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)ON CONFLICT(workspace_id,pane_key)DO UPDATE SET provider_id=excluded.provider_id,project_id=excluded.project_id,worktree_id=excluded.worktree_id,title=excluded.title",rusqlite::params![pane.id,pane.workspace_id,pane.pane_key,pane.provider_id,pane.project_id,pane.worktree_id,pane.title,Utc::now().to_rfc3339()])?;
        Ok(())
    }
    pub fn list(&self, workspace_id: &str) -> Result<Vec<PaneRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt=conn.prepare("SELECT id,workspace_id,pane_key,provider_id,project_id,worktree_id,title FROM panes WHERE workspace_id=?1")?;
        let rows = stmt.query_map([workspace_id], |r| {
            Ok(PaneRecord {
                id: r.get(0)?,
                workspace_id: r.get(1)?,
                pane_key: r.get(2)?,
                provider_id: r.get(3)?,
                project_id: r.get(4)?,
                worktree_id: r.get(5)?,
                title: r.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: String,
    pub pane_id: Option<String>,
    pub project_id: Option<String>,
    pub worktree_id: Option<String>,
    pub provider_id: String,
    pub cwd: String,
    pub title: Option<String>,
    pub state: String,
    pub exit_code: Option<i32>,
    pub resume_ref: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}
pub struct SessionsDao<'a>(pub &'a Database);
impl SessionsDao<'_> {
    pub fn insert(&self, row: &SessionRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO terminal_sessions(id,pane_id,project_id,worktree_id,provider_id,cwd,title,state,exit_code,resume_ref,started_at,ended_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",rusqlite::params![row.id,row.pane_id,row.project_id,row.worktree_id,row.provider_id,row.cwd,row.title,row.state,row.exit_code,row.resume_ref,row.started_at,row.ended_at])?;
        Ok(())
    }
    pub fn finish(
        &self,
        id: &str,
        state: &str,
        exit_code: Option<i32>,
    ) -> Result<(), PersistenceError> {
        self.0.connection()?.execute(
            "UPDATE terminal_sessions SET state=?2,exit_code=?3,ended_at=?4 WHERE id=?1",
            rusqlite::params![id, state, exit_code, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
    pub fn history(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt=conn.prepare("SELECT id,pane_id,project_id,worktree_id,provider_id,cwd,title,state,exit_code,resume_ref,started_at,ended_at FROM terminal_sessions WHERE project_id=?1 ORDER BY started_at DESC LIMIT ?2")?;
        let rows = stmt.query_map(rusqlite::params![project_id, limit.min(500)], |r| {
            Ok(SessionRecord {
                id: r.get(0)?,
                pane_id: r.get(1)?,
                project_id: r.get(2)?,
                worktree_id: r.get(3)?,
                provider_id: r.get(4)?,
                cwd: r.get(5)?,
                title: r.get(6)?,
                state: r.get(7)?,
                exit_code: r.get(8)?,
                resume_ref: r.get(9)?,
                started_at: r.get(10)?,
                ended_at: r.get(11)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub fn get(&self, id: &str) -> Result<Option<SessionRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let result=conn.query_row("SELECT id,pane_id,project_id,worktree_id,provider_id,cwd,title,state,exit_code,resume_ref,started_at,ended_at FROM terminal_sessions WHERE id=?1",[id],|r|Ok(SessionRecord{id:r.get(0)?,pane_id:r.get(1)?,project_id:r.get(2)?,worktree_id:r.get(3)?,provider_id:r.get(4)?,cwd:r.get(5)?,title:r.get(6)?,state:r.get(7)?,exit_code:r.get(8)?,resume_ref:r.get(9)?,started_at:r.get(10)?,ended_at:r.get(11)?}));
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileRecord {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: serde_json::Value,
    pub env: serde_json::Value,
    pub color: Option<String>,
    pub kind: String,
}
pub struct ProviderProfilesDao<'a>(pub &'a Database);
impl ProviderProfilesDao<'_> {
    pub fn list(&self) -> Result<Vec<ProviderProfileRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt=conn.prepare("SELECT id,label,command,args_json,env_json,color,kind FROM provider_profiles ORDER BY kind,label")?;
        let rows = stmt.query_map([], |r| {
            Ok(ProviderProfileRecord {
                id: r.get(0)?,
                label: r.get(1)?,
                command: r.get(2)?,
                args: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                env: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                color: r.get(5)?,
                kind: r.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub fn upsert(&self, row: &ProviderProfileRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO provider_profiles(id,label,command,args_json,env_json,color,kind,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)ON CONFLICT(id)DO UPDATE SET label=excluded.label,command=excluded.command,args_json=excluded.args_json,env_json=excluded.env_json,color=excluded.color",rusqlite::params![row.id,row.label,row.command,serde_json::to_string(&row.args)?,serde_json::to_string(&row.env)?,row.color,row.kind,Utc::now().to_rfc3339()])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshotRecord {
    pub provider_id: String,
    pub snapshot: serde_json::Value,
    pub fetched_at: String,
    pub stale: bool,
}
pub struct UsageSnapshotsDao<'a>(pub &'a Database);
impl UsageSnapshotsDao<'_> {
    pub fn list(&self) -> Result<Vec<UsageSnapshotRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt =
            conn.prepare("SELECT provider_id,snapshot_json,fetched_at,stale FROM usage_snapshots")?;
        let rows = stmt.query_map([], |r| {
            Ok(UsageSnapshotRecord {
                provider_id: r.get(0)?,
                snapshot: serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or_default(),
                fetched_at: r.get(2)?,
                stale: r.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    pub fn upsert(&self, row: &UsageSnapshotRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO usage_snapshots(provider_id,snapshot_json,fetched_at,stale)VALUES(?1,?2,?3,?4)ON CONFLICT(provider_id)DO UPDATE SET snapshot_json=excluded.snapshot_json,fetched_at=excluded.fetched_at,stale=excluded.stale",rusqlite::params![row.provider_id,serde_json::to_string(&row.snapshot)?,row.fetched_at,row.stale])?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub providers: Vec<String>,
    pub content_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBindingRecord {
    pub id: String,
    pub skill_id: String,
    pub scope: String,
    pub scope_ref_id: Option<String>,
    pub enabled: bool,
    pub precedence: i32,
    pub applied_artifacts: serde_json::Value,
}

pub struct SkillsDao<'a>(pub &'a Database);
impl SkillsDao<'_> {
    pub fn upsert(&self, skill: &SkillRecord) -> Result<(), PersistenceError> {
        self.0.connection()?.execute("INSERT INTO skills(id,slug,name,version,description,providers_json,content_path,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)ON CONFLICT(slug)DO UPDATE SET name=excluded.name,version=excluded.version,description=excluded.description,providers_json=excluded.providers_json,content_path=excluded.content_path",rusqlite::params![skill.id,skill.slug,skill.name,skill.version,skill.description,serde_json::to_string(&skill.providers)?,skill.content_path,Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<SkillRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt = conn.prepare("SELECT id,slug,name,version,description,providers_json,content_path FROM skills ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(SkillRecord {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                description: row.get(4)?,
                providers: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
                content_path: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Forgets a skill and every binding that referenced it.
    pub fn delete(&self, skill_id: &str) -> Result<(), PersistenceError> {
        let conn = self.0.connection()?;
        conn.execute("DELETE FROM skill_bindings WHERE skill_id=?1", [skill_id])?;
        conn.execute("DELETE FROM skills WHERE id=?1", [skill_id])?;
        Ok(())
    }
    pub fn list_bindings(&self) -> Result<Vec<SkillBindingRecord>, PersistenceError> {
        let conn = self.0.connection()?;
        let mut stmt = conn.prepare("SELECT id,skill_id,scope,scope_ref_id,enabled,precedence,applied_artifacts_json FROM skill_bindings ORDER BY precedence DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(SkillBindingRecord {
                id: row.get(0)?,
                skill_id: row.get(1)?,
                scope: row.get(2)?,
                scope_ref_id: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                precedence: row.get(5)?,
                applied_artifacts: serde_json::from_str(&row.get::<_, String>(6)?)
                    .unwrap_or_default(),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_binding(&self, binding: &SkillBindingRecord) -> Result<(), PersistenceError> {
        let conn = self.0.connection()?;
        let existing = conn.query_row(
            "SELECT id FROM skill_bindings WHERE skill_id=?1 AND scope=?2 AND scope_ref_id IS ?3",
            rusqlite::params![binding.skill_id, binding.scope, binding.scope_ref_id],
            |row| row.get::<_, String>(0),
        );
        match existing {
            Ok(id) => {
                conn.execute("UPDATE skill_bindings SET enabled=?2,precedence=?3,applied_artifacts_json=?4 WHERE id=?1",rusqlite::params![id,binding.enabled,binding.precedence,serde_json::to_string(&binding.applied_artifacts)?])?;
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                conn.execute("INSERT INTO skill_bindings(id,skill_id,scope,scope_ref_id,enabled,precedence,applied_artifacts_json,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",rusqlite::params![binding.id,binding.skill_id,binding.scope,binding.scope_ref_id,binding.enabled,binding.precedence,serde_json::to_string(&binding.applied_artifacts)?,Utc::now().to_rfc3339()])?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub fn binding(
        &self,
        skill_id: &str,
        scope: &str,
        scope_ref_id: Option<&str>,
    ) -> Result<Option<SkillBindingRecord>, PersistenceError> {
        Ok(self.list_bindings()?.into_iter().find(|binding| {
            binding.skill_id == skill_id
                && binding.scope == scope
                && binding.scope_ref_id.as_deref() == scope_ref_id
        }))
    }
}
