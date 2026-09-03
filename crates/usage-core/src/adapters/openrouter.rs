use super::{compact, number_at, percent};
use crate::{AuthState, UsageAdapter, UsageCard, UsageError, UsageLine};
use async_trait::async_trait;
use directories::BaseDirs;
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;

pub struct OpenRouterAdapter {
    client: Client,
    base_url: String,
    config_path: Option<PathBuf>,
    api_key: Option<String>,
}

impl OpenRouterAdapter {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            base_url: "https://openrouter.ai".into(),
            config_path: BaseDirs::new()
                .map(|base| base.home_dir().join(".config/opencode/opencode.json")),
            api_key: None,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    fn token(&self) -> Result<String, UsageError> {
        if let Some(token) = self
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        {
            return Ok(token);
        }
        if let Some(path) = &self.config_path {
            if let Ok(contents) = std::fs::read_to_string(path) {
                let value: Value = serde_json::from_str(&contents)?;
                if let Some(token) = value
                    .pointer("/provider/openrouter/options/apiKey")
                    .or_else(|| value.pointer("/openrouter/apiKey"))
                    .and_then(Value::as_str)
                {
                    return Ok(token.to_owned());
                }
            }
        }
        Err(UsageError::Credentials("OpenRouter".into()))
    }
}

pub fn parse_openrouter(value: &Value) -> Result<UsageCard, UsageError> {
    let used = number_at(value, &["/data/usage", "/usage"])
        .ok_or_else(|| UsageError::InvalidResponse("OpenRouter usage absent".into()))?;
    let limit = number_at(value, &["/data/limit", "/limit"]);
    let remaining = limit.map(|limit| (limit - used).max(0.0));
    let mut lines = vec![UsageLine {
        label: "Uso".into(),
        value: format!("${}", compact(used)),
        pct: percent(used, limit),
        resets_at: None,
    }];
    if let Some(remaining) = remaining {
        lines.push(UsageLine {
            label: "Saldo".into(),
            value: format!("${}", compact(remaining)),
            pct: None,
            resets_at: None,
        });
    }
    Ok(UsageCard {
        label: "OpenCode · OpenRouter".into(),
        lines,
        auth: AuthState::Ok,
        stale: false,
    })
}

#[async_trait]
impl UsageAdapter for OpenRouterAdapter {
    fn provider_id(&self) -> &'static str {
        "opencode"
    }

    async fn fetch(&self) -> Result<UsageCard, UsageError> {
        let response = self
            .client
            .get(format!("{}/api/v1/auth/key", self.base_url))
            .bearer_auth(self.token()?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(UsageError::AuthenticationExpired("OpenRouter".into()));
        }
        parse_openrouter(&response.error_for_status()?.json().await?)
    }
}
