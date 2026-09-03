//! Centralized, credential-safe provider usage polling.
#![forbid(unsafe_code)]

pub mod adapters;
pub mod poller;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageLine {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageCard {
    pub label: String,
    pub lines: Vec<UsageLine>,
    pub auth: AuthState,
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthState {
    Ok,
    /// Not determined: no successful poll yet, and the last failure was not about auth.
    /// A network outage must never be reported to the user as a credential problem.
    Unknown,
    /// The stored token is past its own `expiresAt`. The credential is still good — running
    /// the provider's CLI once refreshes it. This app deliberately does not refresh it
    /// itself: rewriting another tool's credential store risks rotating a refresh token out
    /// from under the CLI that owns it (Principle III).
    Expired,
    /// The provider rejected a token we believed valid, or no credential exists at all.
    /// This is the state that genuinely requires logging in again.
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub providers: std::collections::BTreeMap<String, UsageCard>,
    pub updated_at: DateTime<Utc>,
    pub offline: bool,
}

#[async_trait]
pub trait UsageAdapter: Send + Sync {
    fn provider_id(&self) -> &'static str;
    async fn fetch(&self) -> Result<UsageCard, UsageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("credentials unavailable for {0}")]
    Credentials(String),
    #[error("authentication expired for {0}")]
    AuthenticationExpired(String),
    #[error("stored token for {0} is past its expiry; run the provider CLI once to refresh it")]
    TokenExpired(String),
    #[error("provider request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid provider response: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
