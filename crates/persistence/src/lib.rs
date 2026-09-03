//! SQLite persistence and migrations.
#![forbid(unsafe_code)]

pub mod dao;

use directories::BaseDirs;
use refinery::embed_migrations;
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

embed_migrations!("migrations");

#[derive(Clone)]
pub struct Database {
    inner: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    pub fn open_default() -> Result<Self, PersistenceError> {
        let dirs = BaseDirs::new().ok_or(PersistenceError::NoDataDirectory)?;
        Self::open(
            dirs.home_dir()
                .join("Library/Application Support/AITerminal/app.db"),
        )
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(&path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        migrations::runner().run(&mut connection)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(connection)),
            path,
        })
    }
    pub fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, PersistenceError> {
        self.inner.lock().map_err(|_| PersistenceError::Poisoned)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("application data directory is unavailable")]
    NoDataDirectory,
    #[error("database lock was poisoned")]
    Poisoned,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] refinery::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrates_in_memory_database() {
        let path = std::env::temp_dir().join(format!("terminal-ai-{}.db", uuid::Uuid::new_v4()));
        let db = Database::open(&path).expect("database opens");
        let tables: i64 = db
            .connection()
            .expect("lock")
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert!(tables >= 15);
        let _ = std::fs::remove_file(path);
    }
}
