use super::{number_at, string_at};
use crate::{AuthState, UsageAdapter, UsageCard, UsageError, UsageLine};
use async_trait::async_trait;
use directories::BaseDirs;
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;

pub struct CodexAdapter {
    client: Client,
    base_url: String,
    credential_path: Option<PathBuf>,
}

impl CodexAdapter {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            base_url: "https://chatgpt.com".into(),
            credential_path: BaseDirs::new().map(|base| base.home_dir().join(".codex/auth.json")),
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
        let path = self
            .credential_path
            .as_ref()
            .ok_or_else(|| UsageError::Credentials("Codex".into()))?;
        let value: Value = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|_| UsageError::Credentials("Codex".into()))?;
        value
            .pointer("/tokens/access_token")
            .or_else(|| value.pointer("/access_token"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| UsageError::Credentials("Codex".into()))
    }
}

pub fn parse_codex(value: &Value) -> Result<UsageCard, UsageError> {
    let windows = [
        ("5 horas", "/rate_limit/primary_window", "/primary"),
        ("Semanal", "/rate_limit/secondary_window", "/secondary"),
        (
            "Review",
            "/code_review_rate_limit/primary_window",
            "/code_review",
        ),
    ];
    let mut lines = Vec::new();
    for (label, primary, fallback) in windows {
        if let Some(pct) = number_at(
            value,
            &[
                &format!("{primary}/used_percent"),
                &format!("{fallback}/used_percent"),
                &format!("{primary}/utilization"),
            ],
        ) {
            let pct = if pct <= 1.0 { pct * 100.0 } else { pct };
            let reset_epoch = number_at(
                value,
                &[
                    &format!("{primary}/reset_at"),
                    &format!("{fallback}/reset_at"),
                ],
            )
            .map(|epoch| epoch.to_string());
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
                )
                .or(reset_epoch),
            });
        }
    }
    if lines.is_empty() {
        return Err(UsageError::InvalidResponse(
            "Codex usage windows absent".into(),
        ));
    }
    Ok(UsageCard {
        label: "Codex".into(),
        lines,
        auth: AuthState::Ok,
        stale: false,
    })
}

#[async_trait]
impl UsageAdapter for CodexAdapter {
    fn provider_id(&self) -> &'static str {
        "codex"
    }

    async fn fetch(&self) -> Result<UsageCard, UsageError> {
        let response = self
            .client
            .get(format!("{}/backend-api/wham/usage", self.base_url))
            .bearer_auth(self.token()?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(UsageError::AuthenticationExpired("Codex".into()));
        }
        parse_codex(&response.error_for_status()?.json().await?)
    }
}
