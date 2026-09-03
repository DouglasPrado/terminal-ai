use crate::{AuthState, UsageAdapter, UsageCard, UsageError, UsageSnapshot};
use chrono::{DateTime, Duration, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::Mutex;

const MIN_REFRESH_SECONDS: i64 = 300;
const CACHE_SECONDS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub scheduled: bool,
    pub next_allowed_at: DateTime<Utc>,
}

#[derive(Default)]
struct PollState {
    snapshot: Option<UsageSnapshot>,
    last_attempt: BTreeMap<String, DateTime<Utc>>,
}

pub struct UsagePoller {
    adapters: Vec<Arc<dyn UsageAdapter>>,
    state: Mutex<PollState>,
    cache_path: PathBuf,
    refresh_seconds: i64,
}

impl UsagePoller {
    pub fn new(
        adapters: Vec<Arc<dyn UsageAdapter>>,
        cache_path: PathBuf,
        refresh_seconds: u64,
    ) -> Self {
        Self {
            adapters,
            state: Mutex::new(PollState::default()),
            cache_path,
            refresh_seconds: i64::try_from(refresh_seconds)
                .unwrap_or(MIN_REFRESH_SECONDS)
                .max(MIN_REFRESH_SECONDS),
        }
    }

    pub async fn snapshot(&self) -> UsageSnapshot {
        let mut state = self.state.lock().await;
        if state.snapshot.is_none() {
            state.snapshot = self.read_cache().ok();
        }
        state.snapshot.clone().unwrap_or_else(empty_snapshot)
    }

    /// Poll usage for `provider_id` (or all providers when `None`).
    ///
    /// `manual` distinguishes an explicit user-initiated refresh from the autonomous poller. A
    /// user click is a distinct action, not per-card polling, so it is bounded by the short ~60s
    /// cache window; the single background poller passes `manual = false` and keeps the 300s floor
    /// (Constitution IV: "≥300s floor, ~60s cache").
    pub async fn refresh(
        &self,
        provider_id: Option<&str>,
        manual: bool,
    ) -> Result<(RefreshResult, UsageSnapshot), UsageError> {
        let now = Utc::now();
        let mut state = self.state.lock().await;
        if state.snapshot.is_none() {
            state.snapshot = self.read_cache().ok();
        }

        let selected: Vec<_> = self
            .adapters
            .iter()
            .filter(|adapter| provider_id.is_none_or(|id| id == adapter.provider_id()))
            .cloned()
            .collect();
        if selected.is_empty() {
            return Err(UsageError::InvalidResponse(format!(
                "unknown usage provider: {}",
                provider_id.unwrap_or("none")
            )));
        }

        let mut snapshot = state.snapshot.clone().unwrap_or_else(empty_snapshot);

        let floor = Duration::seconds(self.refresh_seconds);
        let cache = Duration::seconds(CACHE_SECONDS);
        // A known provider waits out the autonomous floor for the background poller, but only the
        // short cache window for an explicit user click, so clicking a card actually re-fetches.
        let known_threshold = if manual { cache } else { floor };
        let to_fetch: Vec<_> = selected
            .iter()
            .filter(|adapter| {
                let id = adapter.provider_id();
                let known = snapshot.providers.contains_key(id);
                provider_due(
                    now,
                    state.last_attempt.get(id).copied(),
                    known,
                    known_threshold,
                    cache,
                )
            })
            .cloned()
            .collect();

        let next_allowed_at = selected
            .iter()
            .map(|adapter| {
                state
                    .last_attempt
                    .get(adapter.provider_id())
                    .map_or(now, |last| *last + known_threshold)
            })
            .min()
            .unwrap_or(now);

        if to_fetch.is_empty() {
            return Ok((
                RefreshResult {
                    scheduled: false,
                    next_allowed_at,
                },
                snapshot,
            ));
        }

        let mut offline = false;
        for adapter in to_fetch {
            state.last_attempt.insert(adapter.provider_id().into(), now);
            match adapter.fetch().await {
                Ok(card) => {
                    snapshot
                        .providers
                        .insert(adapter.provider_id().into(), card);
                }
                Err(error) => {
                    offline |= matches!(error, UsageError::Request(_) | UsageError::Io(_));
                    let card = snapshot
                        .providers
                        .entry(adapter.provider_id().into())
                        .or_insert_with(|| UsageCard {
                            label: provider_label(adapter.provider_id()).into(),
                            lines: Vec::new(),
                            auth: AuthState::Unknown,
                            stale: true,
                        });
                    card.stale = true;
                    // Only an auth failure may move the auth state. A request or parse error
                    // leaves the last known state alone — the card goes stale, not "reauthenticate".
                    match error {
                        UsageError::TokenExpired(_) => card.auth = AuthState::Expired,
                        UsageError::AuthenticationExpired(_) | UsageError::Credentials(_) => {
                            card.auth = AuthState::Rejected
                        }
                        _ => {}
                    }
                }
            }
        }
        snapshot.updated_at = now;
        snapshot.offline = offline;
        self.write_cache(&snapshot)?;
        state.snapshot = Some(snapshot.clone());
        Ok((
            RefreshResult {
                scheduled: true,
                next_allowed_at: now + known_threshold,
            },
            snapshot,
        ))
    }

    fn read_cache(&self) -> Result<UsageSnapshot, UsageError> {
        let lock = self.lock_file()?;
        lock.lock_shared()?;
        let result = std::fs::read_to_string(&self.cache_path)
            .map_err(UsageError::from)
            .and_then(|contents| serde_json::from_str(&contents).map_err(UsageError::from));
        FileExt::unlock(&lock)?;
        result
    }

    fn write_cache(&self, snapshot: &UsageSnapshot) -> Result<(), UsageError> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock = self.lock_file()?;
        lock.lock_exclusive()?;
        let temporary = self.cache_path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(snapshot)?)?;
        std::fs::rename(temporary, &self.cache_path)?;
        FileExt::unlock(&lock)?;
        Ok(())
    }

    fn lock_file(&self) -> Result<File, UsageError> {
        let path = self.cache_path.with_extension("lock");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?)
    }
}

/// Whether a provider is due for a fetch, given when it was last attempted (Constitution IV
/// per-provider cadence). A never-attempted provider is always due — even if a sibling was polled
/// seconds ago. A known provider waits out `known_threshold` (the 300s floor for the autonomous
/// poller, the ~60s cache for an explicit user click). A never-succeeded provider retries no faster
/// than the short `cache` window so a persistently-failing endpoint is not hammered.
fn provider_due(
    now: DateTime<Utc>,
    last_attempt: Option<DateTime<Utc>>,
    known: bool,
    known_threshold: Duration,
    cache: Duration,
) -> bool {
    match last_attempt {
        None => true,
        Some(last) if known => now - last >= known_threshold,
        Some(last) => now - last >= cache,
    }
}

fn empty_snapshot() -> UsageSnapshot {
    UsageSnapshot {
        providers: BTreeMap::new(),
        updated_at: DateTime::UNIX_EPOCH,
        offline: false,
    }
}

fn provider_label(id: &str) -> &str {
    match id {
        "claude" => "Claude",
        "codex" => "Codex",
        "opencode" => "OpenCode · OpenRouter",
        _ => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: Duration = Duration::seconds(MIN_REFRESH_SECONDS);
    const CACHE: Duration = Duration::seconds(CACHE_SECONDS);

    // A manual refresh passes `known_threshold = cache`; the autonomous poller passes `floor`.
    fn threshold(manual: bool) -> Duration {
        if manual {
            CACHE
        } else {
            FLOOR
        }
    }

    #[test]
    fn never_attempted_is_always_due() {
        let now = DateTime::UNIX_EPOCH + Duration::seconds(1_000);
        assert!(provider_due(now, None, true, threshold(true), CACHE));
        assert!(provider_due(now, None, true, threshold(false), CACHE));
        assert!(provider_due(now, None, false, threshold(true), CACHE));
    }

    #[test]
    fn known_provider_manual_click_honors_60s_cache_not_300s_floor() {
        // This is the fix: a card fetched 90s ago is re-fetched on a user click (past the 60s
        // cache) but skipped by the autonomous poller (still inside the 300s floor).
        let now = DateTime::UNIX_EPOCH + Duration::seconds(1_000);
        let last = Some(now - Duration::seconds(90));
        assert!(
            provider_due(now, last, true, threshold(true), CACHE),
            "manual click should re-fetch after the 60s cache window"
        );
        assert!(
            !provider_due(now, last, true, threshold(false), CACHE),
            "autonomous poller should still wait out the 300s floor"
        );
    }

    #[test]
    fn known_provider_within_cache_is_throttled_even_for_a_manual_click() {
        let now = DateTime::UNIX_EPOCH + Duration::seconds(1_000);
        let last = Some(now - Duration::seconds(30));
        assert!(!provider_due(now, last, true, threshold(true), CACHE));
        assert!(!provider_due(now, last, false, threshold(false), CACHE));
    }

    #[test]
    fn never_succeeded_provider_retries_no_faster_than_the_cache_window() {
        let now = DateTime::UNIX_EPOCH + Duration::seconds(1_000);
        // `known == false`: the provider errored before, so the short cache always applies.
        assert!(provider_due(
            now,
            Some(now - Duration::seconds(90)),
            false,
            threshold(false),
            CACHE
        ));
        assert!(!provider_due(
            now,
            Some(now - Duration::seconds(30)),
            false,
            threshold(false),
            CACHE
        ));
    }
}
