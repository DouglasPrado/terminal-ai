//! OpenCode usage read from OpenCode's own local database.
//!
//! OpenCode is a client, not a subscription: it routes to whichever provider you configure, so
//! there is no quota endpoint to poll and no credential to hold. What it does keep is a record
//! of every message it sent, with the token counts the provider reported. Reading that is the
//! only measure of OpenCode usage that stays true whichever provider is behind it — and it needs
//! no network and no secrets.

use crate::{AuthState, UsageAdapter, UsageCard, UsageError, UsageLine};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use directories::BaseDirs;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;

pub struct OpenCodeAdapter {
    database_path: Option<PathBuf>,
}

#[derive(Default)]
struct Totals {
    messages: u64,
    input: u64,
    output: u64,
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        Self {
            database_path: BaseDirs::new()
                .map(|base| base.home_dir().join(".local/share/opencode/opencode.db")),
        }
    }

    pub fn with_database_path(mut self, path: PathBuf) -> Self {
        self.database_path = Some(path);
        self
    }
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Sums the token counts OpenCode recorded for messages created at or after `since_ms`.
/// Opened read-only: OpenCode may well be running and writing to this database.
fn read_totals(path: &std::path::Path, since_ms: i64) -> Result<Totals, UsageError> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| UsageError::InvalidResponse(error.to_string()))?;
    let mut statement = connection
        .prepare("SELECT data FROM message WHERE time_created >= ?1")
        .map_err(|error| UsageError::InvalidResponse(error.to_string()))?;
    let rows = statement
        .query_map([since_ms], |row| row.get::<_, String>(0))
        .map_err(|error| UsageError::InvalidResponse(error.to_string()))?;
    let mut totals = Totals::default();
    for row in rows.flatten() {
        let Ok(value) = serde_json::from_str::<Value>(&row) else {
            continue;
        };
        let Some(tokens) = value.get("tokens").filter(|tokens| tokens.is_object()) else {
            continue;
        };
        totals.messages += 1;
        totals.input += tokens
            .pointer("/input")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        totals.output += tokens
            .pointer("/output")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    }
    Ok(totals)
}

/// `12345` → `12,3k`, `1234567` → `1,2M`. A sidebar card has no room for raw digits.
fn compact(value: u64) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => format!("{:.1}k", value as f64 / 1_000.0).replace('.', ","),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0).replace('.', ","),
    }
}

#[async_trait]
impl UsageAdapter for OpenCodeAdapter {
    fn provider_id(&self) -> &'static str {
        "opencode"
    }

    async fn fetch(&self) -> Result<UsageCard, UsageError> {
        let path = self
            .database_path
            .clone()
            .ok_or_else(|| UsageError::Credentials("OpenCode".into()))?;
        if !path.exists() {
            return Err(UsageError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "opencode.db not found",
            )));
        }
        let now = Utc::now();
        let day = read_totals(&path, (now - Duration::hours(24)).timestamp_millis())?;
        let week = read_totals(&path, (now - Duration::days(7)).timestamp_millis())?;
        Ok(UsageCard {
            label: "OpenCode".into(),
            lines: vec![
                UsageLine {
                    label: "24h".into(),
                    value: format!("{} ↑ {} ↓", compact(day.input), compact(day.output)),
                    // No percentage: OpenCode has no quota to be a percentage of.
                    pct: None,
                    resets_at: None,
                },
                UsageLine {
                    label: "7d".into(),
                    value: format!("{} ↑ {} ↓", compact(week.input), compact(week.output)),
                    pct: None,
                    resets_at: None,
                },
                UsageLine {
                    label: "msgs 7d".into(),
                    value: week.messages.to_string(),
                    pct: None,
                    resets_at: None,
                },
            ],
            // Nothing is authenticated here — the reading is local.
            auth: AuthState::Ok,
            stale: false,
        })
    }
}
