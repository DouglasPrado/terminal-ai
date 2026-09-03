//! Scoped Markdown memory with revision history and FTS5 search.
#![forbid(unsafe_code)]

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use terminal_ai_domain::{MemoryType, Scope, ScopeLevel};
use terminal_ai_persistence::{Database, PersistenceError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub scope: Scope,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySource {
    pub entry_id: String,
    pub scope: Scope,
}

pub fn add(
    database: &Database,
    root: &Path,
    scope: &Scope,
    memory_type: MemoryType,
    title: &str,
    body: &str,
) -> Result<String, MemoryError> {
    validate_scope(scope)?;
    let title = title.trim();
    let body = body.trim();
    if title.is_empty() || body.is_empty() || title.len() > 200 || body.len() > 100_000 {
        return Err(MemoryError::InvalidContent);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let directory = root
        .join(scope_name(scope.level))
        .join(scope.ref_id.as_deref().unwrap_or("global"));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{id}.md"));
    atomic_write(&path, body)?;
    let revision_directory = root.join("revisions").join(&id);
    std::fs::create_dir_all(&revision_directory)?;
    let revision_path = revision_directory.join(format!("{}.md", Utc::now().timestamp_millis()));
    atomic_write(&revision_path, body)?;
    let now = Utc::now().to_rfc3339();
    let connection = database.connection()?;
    connection.execute("INSERT INTO memory_entries(id,scope,scope_ref_id,type,title,content_path,created_at,updated_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",rusqlite::params![id,scope_name(scope.level),scope.ref_id,memory_type_name(memory_type),title,path.to_string_lossy(),now,now])?;
    connection.execute(
        "INSERT INTO memory_revisions(id,entry_id,content_path,created_at)VALUES(?1,?2,?3,?4)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            id,
            revision_path.to_string_lossy(),
            now
        ],
    )?;
    connection.execute(
        "UPDATE memory_fts SET body=?2 WHERE entry_id=?1",
        rusqlite::params![id, body],
    )?;
    Ok(id)
}

pub fn list(
    database: &Database,
    scope: &Scope,
    root: &Path,
) -> Result<Vec<MemoryEntry>, MemoryError> {
    validate_scope(scope)?;
    query_entries(database, root, None, Some(scope))
}

pub fn search(
    database: &Database,
    root: &Path,
    query: &str,
    scope: Option<&Scope>,
) -> Result<Vec<MemoryEntry>, MemoryError> {
    if let Some(scope) = scope {
        validate_scope(scope)?;
    }
    let safe_query = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
    query_entries(database, root, Some(&safe_query), scope)
}

/// Edit an existing entry's body (and optionally title), keeping the Markdown file, a new
/// revision, and the FTS index (title + body) in sync so search never silently desyncs.
pub fn update(
    database: &Database,
    root: &Path,
    id: &str,
    title: Option<&str>,
    body: &str,
) -> Result<(), MemoryError> {
    let body = body.trim();
    if body.is_empty() || body.len() > 100_000 {
        return Err(MemoryError::InvalidContent);
    }
    let title = title.map(str::trim);
    if let Some(title) = title {
        if title.is_empty() || title.len() > 200 {
            return Err(MemoryError::InvalidContent);
        }
    }
    let connection = database.connection()?;
    let content_path: String = connection
        .query_row(
            "SELECT content_path FROM memory_entries WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => MemoryError::NotFound,
            other => MemoryError::Sql(other),
        })?;
    atomic_write(&PathBuf::from(&content_path), body)?;
    let now = Utc::now().to_rfc3339();
    let revision_directory = root.join("revisions").join(id);
    std::fs::create_dir_all(&revision_directory)?;
    let revision_path = revision_directory.join(format!("{}.md", Utc::now().timestamp_millis()));
    atomic_write(&revision_path, body)?;
    connection.execute(
        "INSERT INTO memory_revisions(id,entry_id,content_path,created_at)VALUES(?1,?2,?3,?4)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            id,
            revision_path.to_string_lossy(),
            now
        ],
    )?;
    if let Some(title) = title {
        connection.execute(
            "UPDATE memory_entries SET title=?2, updated_at=?3 WHERE id=?1",
            rusqlite::params![id, title, now],
        )?;
    } else {
        connection.execute(
            "UPDATE memory_entries SET updated_at=?2 WHERE id=?1",
            rusqlite::params![id, now],
        )?;
    }
    // The AFTER UPDATE trigger keeps `title` synced; `body` is file-backed so we sync it explicitly.
    connection.execute(
        "UPDATE memory_fts SET body=?2 WHERE entry_id=?1",
        rusqlite::params![id, body],
    )?;
    Ok(())
}

pub fn preview_context(
    database: &Database,
    root: &Path,
    scope: &Scope,
) -> Result<(String, Vec<MemorySource>), MemoryError> {
    let entries = list_with_global(database, scope, root)?;
    let mut composed = String::from("# Terminal AI memory context\n\n");
    let mut sources = Vec::new();
    for entry in entries {
        composed.push_str(&format!("## {}\n{}\n\n", entry.title, entry.body));
        sources.push(MemorySource {
            entry_id: entry.id,
            scope: entry.scope,
        });
        if composed.len() > 50_000 {
            composed.truncate(50_000);
            break;
        }
    }
    Ok((composed, sources))
}

fn list_with_global(
    database: &Database,
    scope: &Scope,
    root: &Path,
) -> Result<Vec<MemoryEntry>, MemoryError> {
    let mut entries = list(database, scope, root)?;
    if scope.level != ScopeLevel::Global {
        entries.extend(list(
            database,
            &Scope {
                level: ScopeLevel::Global,
                ref_id: None,
            },
            root,
        )?);
    }
    entries.sort_by_key(|entry| entry.scope.level.precedence());
    Ok(entries)
}

fn query_entries(
    database: &Database,
    _root: &Path,
    query: Option<&str>,
    scope: Option<&Scope>,
) -> Result<Vec<MemoryEntry>, MemoryError> {
    let connection = database.connection()?;
    // No scope → search across ALL scopes (union); Some(scope) keeps strict scope + ref isolation.
    let (mut sql, prefix, order) = if query.is_some() {
        (
            String::from(
                "SELECT e.id,e.scope,e.scope_ref_id,e.type,e.title,e.content_path,e.created_at,e.updated_at FROM memory_fts f JOIN memory_entries e ON e.id=f.entry_id WHERE memory_fts MATCH ?",
            ),
            "e.",
            " ORDER BY bm25(memory_fts) LIMIT 100",
        )
    } else {
        (
            String::from(
                "SELECT id,scope,scope_ref_id,type,title,content_path,created_at,updated_at FROM memory_entries WHERE 1=1",
            ),
            "",
            " ORDER BY updated_at DESC LIMIT 100",
        )
    };
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(term) = query {
        binds.push(Box::new(term.to_string()));
    }
    if let Some(scope) = scope {
        sql.push_str(&format!(
            " AND {prefix}scope=? AND {prefix}scope_ref_id IS ?"
        ));
        binds.push(Box::new(scope_name(scope.level).to_string()));
        binds.push(Box::new(scope.ref_id.clone()));
    }
    sql.push_str(order);
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        rusqlite::params_from_iter(binds.iter().map(|bind| bind.as_ref())),
        |row| {
            let path: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                path,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        },
    )?;
    rows.map(|row| {
        let (id, scope, ref_id, memory_type, title, path, created_at, updated_at) = row?;
        Ok(MemoryEntry {
            id,
            scope: Scope {
                level: parse_scope(&scope)?,
                ref_id,
            },
            memory_type: parse_memory_type(&memory_type)?,
            title,
            body: std::fs::read_to_string(path)?,
            created_at,
            updated_at,
        })
    })
    .collect()
}

fn validate_scope(scope: &Scope) -> Result<(), MemoryError> {
    if scope.level == ScopeLevel::Global && scope.ref_id.is_some()
        || scope.level != ScopeLevel::Global && scope.ref_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(MemoryError::InvalidScope);
    }
    Ok(())
}

fn scope_name(level: ScopeLevel) -> &'static str {
    match level {
        ScopeLevel::Global => "global",
        ScopeLevel::Project => "project",
        ScopeLevel::Worktree => "worktree",
        ScopeLevel::Workspace => "workspace",
        ScopeLevel::Session => "session",
    }
}

fn parse_scope(value: &str) -> Result<ScopeLevel, MemoryError> {
    match value {
        "global" => Ok(ScopeLevel::Global),
        "project" => Ok(ScopeLevel::Project),
        "worktree" => Ok(ScopeLevel::Worktree),
        "workspace" => Ok(ScopeLevel::Workspace),
        "session" => Ok(ScopeLevel::Session),
        _ => Err(MemoryError::InvalidScope),
    }
}

fn memory_type_name(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Fact => "fact",
        MemoryType::Decision => "decision",
        MemoryType::Constraint => "constraint",
        MemoryType::Preference => "preference",
        MemoryType::Glossary => "glossary",
        MemoryType::KnownIssue => "known_issue",
        MemoryType::Command => "command",
        MemoryType::Todo => "todo",
    }
}

fn parse_memory_type(value: &str) -> Result<MemoryType, MemoryError> {
    match value {
        "fact" => Ok(MemoryType::Fact),
        "decision" => Ok(MemoryType::Decision),
        "constraint" => Ok(MemoryType::Constraint),
        "preference" => Ok(MemoryType::Preference),
        "glossary" => Ok(MemoryType::Glossary),
        "known_issue" => Ok(MemoryType::KnownIssue),
        "command" => Ok(MemoryType::Command),
        "todo" => Ok(MemoryType::Todo),
        _ => Err(MemoryError::InvalidContent),
    }
}

fn atomic_write(path: &Path, body: &str) -> Result<(), MemoryError> {
    let temporary = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&temporary, body)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("scope requires the appropriate reference id")]
    InvalidScope,
    #[error("memory entry not found")]
    NotFound,
    #[error("memory title and body must be non-empty and within limits")]
    InvalidContent,
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_search_never_crosses_scope() {
        let root =
            std::env::temp_dir().join(format!("terminal-ai-memory-{}", uuid::Uuid::new_v4()));
        let database = Database::open(root.join("app.db")).expect("database");
        let alpha = Scope {
            level: ScopeLevel::Project,
            ref_id: Some("alpha".into()),
        };
        let beta = Scope {
            level: ScopeLevel::Project,
            ref_id: Some("beta".into()),
        };
        add(
            &database,
            &root,
            &alpha,
            MemoryType::Fact,
            "Alpha",
            "unique token",
        )
        .expect("alpha memory");
        add(
            &database,
            &root,
            &beta,
            MemoryType::Fact,
            "Beta",
            "unique token",
        )
        .expect("beta memory");
        let results = search(&database, &root, "unique", Some(&alpha)).expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].scope.ref_id.as_deref(), Some("alpha"));
        drop(database);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unscoped_search_spans_all_scopes() {
        let root =
            std::env::temp_dir().join(format!("terminal-ai-memory-{}", uuid::Uuid::new_v4()));
        let database = Database::open(root.join("app.db")).expect("database");
        let global = Scope {
            level: ScopeLevel::Global,
            ref_id: None,
        };
        let project = Scope {
            level: ScopeLevel::Project,
            ref_id: Some("alpha".into()),
        };
        add(
            &database,
            &root,
            &global,
            MemoryType::Fact,
            "Global",
            "shared token",
        )
        .expect("global");
        add(
            &database,
            &root,
            &project,
            MemoryType::Fact,
            "Project",
            "shared token",
        )
        .expect("project");
        // Unscoped search reaches every scope.
        assert_eq!(
            search(&database, &root, "shared", None)
                .expect("search")
                .len(),
            2
        );
        // Scoped search still isolates to its scope.
        let scoped = search(&database, &root, "shared", Some(&project)).expect("scoped");
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].scope.ref_id.as_deref(), Some("alpha"));
        drop(database);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn update_keeps_fts_body_in_sync() {
        let root =
            std::env::temp_dir().join(format!("terminal-ai-memory-{}", uuid::Uuid::new_v4()));
        let database = Database::open(root.join("app.db")).expect("database");
        let scope = Scope {
            level: ScopeLevel::Project,
            ref_id: Some("proj".into()),
        };
        let id = add(
            &database,
            &root,
            &scope,
            MemoryType::Fact,
            "Note",
            "alpha keyword",
        )
        .expect("add");
        assert_eq!(
            search(&database, &root, "alpha", Some(&scope))
                .expect("before")
                .len(),
            1
        );
        update(&database, &root, &id, Some("Note v2"), "beta keyword").expect("update");
        assert!(search(&database, &root, "alpha", Some(&scope))
            .expect("stale gone")
            .is_empty());
        let hit = search(&database, &root, "beta", Some(&scope)).expect("fresh");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].title, "Note v2");
        assert_eq!(hit[0].body, "beta keyword");
        drop(database);
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
