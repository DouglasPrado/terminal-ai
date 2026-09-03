use super::{number_at, string_at};
use crate::{AuthState, UsageAdapter, UsageCard, UsageError, UsageLine};
use async_trait::async_trait;
use directories::BaseDirs;
use reqwest::Client;
use serde_json::Value;
use std::{path::PathBuf, process::Command};

pub struct AnthropicAdapter {
    client: Client,
    base_url: String,
    credential_path: Option<PathBuf>,
}

impl AnthropicAdapter {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            base_url: "https://api.anthropic.com".into(),
            credential_path: BaseDirs::new()
                .map(|base| base.home_dir().join(".claude/.credentials.json")),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_credential_path(mut self, path: PathBuf) -> Self {
        self.credential_path = Some(path);
        self
    }

    fn token(&self) -> Result<String, UsageError> {
        // Both sources hold the same shape; collect from each, prefer one that is still
        // valid, and only report expiry when every candidate is past its own `expiresAt`.
        // A malformed file must never abort the chain — the Keychain is the real store on
        // macOS, and a broken JSON file used to mask it entirely.
        let mut candidates = Vec::new();
        if let Some(path) = &self.credential_path {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(value) = serde_json::from_str::<Value>(&contents) {
                    candidates.extend(oauth_candidate(&value));
                }
            }
        }
        let output = Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .output()?;
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                candidates.extend(oauth_candidate(&value));
            } else if !trimmed.is_empty() {
                candidates.push((trimmed.to_owned(), None));
            }
        }
        if candidates.is_empty() {
            return Err(UsageError::Credentials("Claude".into()));
        }
        let now = chrono::Utc::now().timestamp_millis();
        if let Some((token, _)) = candidates
            .iter()
            .find(|(_, expires_at)| expires_at.is_none_or(|value| value > now))
        {
            return Ok(token.clone());
        }
        Err(UsageError::TokenExpired("Claude".into()))
    }
}

/// Pulls `(accessToken, expiresAt-in-millis)` out of a Claude Code credential blob, whether it
/// came from `~/.claude/.credentials.json` or the Keychain item.
fn oauth_candidate(value: &Value) -> Option<(String, Option<i64>)> {
    let node = value
        .pointer("/claudeAiOauth")
        .or_else(|| value.pointer("/oauth"))
        .unwrap_or(value);
    let token = node
        .pointer("/accessToken")
        .or_else(|| node.pointer("/access_token"))
        .and_then(Value::as_str)?;
    let expires_at = node
        .pointer("/expiresAt")
        .or_else(|| node.pointer("/expires_at"))
        .and_then(Value::as_i64)
        // Seconds and milliseconds both appear in the wild; normalise to millis.
        .map(|value| {
            if value < 100_000_000_000 {
                value * 1000
            } else {
                value
            }
        });
    Some((token.to_owned(), expires_at))
}

pub fn parse_anthropic(value: &Value) -> Result<UsageCard, UsageError> {
    let windows = [
        ("Sessão", "/five_hour", "/session"),
        ("Semanal", "/seven_day", "/weekly"),
        ("Sonnet", "/seven_day_sonnet", "/model"),
    ];
    let mut lines = Vec::new();
    for (label, primary, fallback) in windows {
        let utilization = number_at(
            value,
            &[
                &format!("{primary}/utilization"),
                &format!("{fallback}/utilization"),
                &format!("{primary}/percent_used"),
            ],
        );
        if let Some(pct) = utilization {
            let pct = if pct <= 1.0 { pct * 100.0 } else { pct };
            lines.push(UsageLine {
                label: label.into(),
                value: format!("{pct:.0}%"),
                pct: Some(pct.clamp(0.0, 100.0)),
                resets_at: string_at(
                    value,
                    &[
                        &format!("{primary}/resets_at"),
                        &format!("{fallback}/resets_at"),
                    ],
                ),
            });
        }
    }
    if lines.is_empty() {
        return Err(UsageError::InvalidResponse(
            "Anthropic usage windows absent".into(),
        ));
    }
    Ok(UsageCard {
        label: "Claude".into(),
        lines,
        auth: AuthState::Ok,
        stale: false,
    })
}

#[async_trait]
impl UsageAdapter for AnthropicAdapter {
    fn provider_id(&self) -> &'static str {
        "claude"
    }

    async fn fetch(&self) -> Result<UsageCard, UsageError> {
        let response = self
            .client
            .get(format!("{}/api/oauth/usage", self.base_url))
            .bearer_auth(self.token()?)
            .header("anthropic-beta", "oauth-2025-04-20")
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(UsageError::AuthenticationExpired("Claude".into()));
        }
        parse_anthropic(&response.error_for_status()?.json().await?)
    }
}
