//! The read side: `/api/v1` over loopback.
//!
//! Shapes here were taken from a running kernel, not from its documentation — the docs describe
//! `{"workspaces":[…]}` envelopes and a `body` field, and the server actually returns bare arrays
//! and `body_markdown`. Every response therefore goes through one funnel that checks the status,
//! then tries the server's `{"error": …}` shape, and only then the success shape, so a kernel's own
//! message reaches the user instead of a serde "missing field".

use crate::token::AuthToken;
use serde::Deserialize;
use std::time::Duration;
use terminal_ai_domain::memory::{KernelScope, MemoryError};

/// `/api/v1` is mounted only when the kernel runs with `--enable-web`. The app always passes it
/// when it spawns; an attached server might not have, which is why detection uses `/mcp` instead.
pub struct ReadClient {
    client: reqwest::Client,
    base: reqwest::Url,
    token: Option<AuthToken>,
}

#[derive(Debug, Deserialize)]
pub struct SearchHit {
    pub workspace: String,
    pub project: String,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
pub struct PageRecord {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub frontmatter: serde_json::Value,
    /// Note: `body_markdown`, not `body`. The upstream docs say otherwise.
    #[serde(default)]
    pub body_markdown: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PageSummary {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HandoffRecord {
    pub id: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub accepted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
}

impl ReadClient {
    /// # Errors
    /// Fails if `base_url` is not a valid URL or the TLS backend cannot be initialised.
    pub fn new(base_url: &str, token: Option<AuthToken>) -> Result<Self, MemoryError> {
        let base = reqwest::Url::parse(base_url)
            .map_err(|e| MemoryError::Transport(format!("invalid kernel url: {e}")))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            // A corporate HTTP_PROXY in the inherited login-shell environment would otherwise
            // route loopback traffic through a proxy and fail in a very confusing way.
            .no_proxy()
            .build()
            .map_err(|e| MemoryError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            base,
            token,
        })
    }

    fn url(&self, path: &str) -> Result<reqwest::Url, MemoryError> {
        self.base
            .join(path)
            .map_err(|e| MemoryError::Transport(format!("bad path {path}: {e}")))
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        url: reqwest::Url,
    ) -> Result<T, MemoryError> {
        let mut request = self.client.get(url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token.expose());
        }
        let response = request
            .send()
            .await
            .map_err(|e| MemoryError::Transport(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| MemoryError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => MemoryError::Unauthorized,
                404 => MemoryError::NotFound,
                _ => MemoryError::Upstream {
                    code: Some(status.as_u16().to_string()),
                    // Prefer the kernel's own words when it gave us any.
                    message: serde_json::from_str::<ErrorBody>(&body)
                        .map(|e| e.error)
                        .unwrap_or_else(|_| {
                            status
                                .canonical_reason()
                                .unwrap_or("request failed")
                                .to_owned()
                        }),
                },
            });
        }

        // A 200 can still carry an error document.
        if let Ok(err) = serde_json::from_str::<ErrorBody>(&body) {
            return Err(MemoryError::Upstream {
                code: None,
                message: err.error,
            });
        }

        serde_json::from_str(&body).map_err(|e| {
            MemoryError::Protocol(format!(
                "could not read the kernel's response ({e}); the kernel version may have changed"
            ))
        })
    }

    /// Full-text + entity + graph search, always scoped.
    ///
    /// There is no unscoped variant on purpose: a query without workspace and project returns
    /// pages from every project, which is a silent cross-project leak.
    pub async fn search(
        &self,
        scope: &KernelScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, MemoryError> {
        let mut url = self.url("/api/v1/search")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("workspace", &scope.workspace)
            .append_pair("project", &scope.project)
            .append_pair("limit", &limit.clamp(1, 100).to_string());
        self.get(url).await
    }

    pub async fn list_pages(&self, scope: &KernelScope) -> Result<Vec<PageSummary>, MemoryError> {
        let url = self.url(&format!(
            "/api/v1/workspaces/{}/projects/{}/pages",
            scope.workspace, scope.project
        ))?;
        self.get(url).await
    }

    pub async fn read_page(
        &self,
        scope: &KernelScope,
        path: &str,
    ) -> Result<PageRecord, MemoryError> {
        crate::scope::validate_path(path)?;
        let url = self.url(&format!(
            "/api/v1/workspaces/{}/projects/{}/pages/{}",
            scope.workspace, scope.project, path
        ))?;
        self.get(url).await
    }

    /// Handoffs waiting in this project.
    ///
    /// Read-only, and deliberately so: accepting one is the next agent's move, not the app's.
    pub async fn handoffs(
        &self,
        scope: &KernelScope,
        state: Option<&str>,
    ) -> Result<Vec<HandoffRecord>, MemoryError> {
        let mut url = self.url(&format!(
            "/api/v1/workspaces/{}/projects/{}/handoffs",
            scope.workspace, scope.project
        ))?;
        if let Some(state) = state {
            url.query_pairs_mut().append_pair("state", state);
        }
        self.get(url).await
    }

    pub async fn recent(
        &self,
        scope: &KernelScope,
        limit: usize,
    ) -> Result<Vec<PageSummary>, MemoryError> {
        let mut url = self.url(&format!(
            "/api/v1/workspaces/{}/projects/{}/recent",
            scope.workspace, scope.project
        ))?;
        url.query_pairs_mut()
            .append_pair("limit", &limit.clamp(1, 100).to_string());
        self.get(url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> KernelScope {
        KernelScope {
            workspace: "default".into(),
            project: "albert".into(),
            path_prefix: "terminal-ai/project".into(),
        }
    }

    fn client(server: &mockito::Server) -> ReadClient {
        ReadClient::new(&server.url(), None).expect("client builds")
    }

    #[tokio::test]
    async fn search_sends_the_scope_and_reads_a_bare_array() {
        let mut server = mockito::Server::new_async().await;
        // The scope parameters are the FR-046 guarantee. If they ever stop being sent, this
        // mock stops matching and the test fails — which is the point.
        let mock = server
            .mock("GET", "/api/v1/search")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("q".into(), "rustls".into()),
                mockito::Matcher::UrlEncoded("workspace".into(), "default".into()),
                mockito::Matcher::UrlEncoded("project".into(), "albert".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"workspace":"default","project":"albert","path":"terminal-ai/project/decision/a-1.md",
                     "title":"Use rustls","kind":"decision","snippet":"<mark>rustls</mark> it is","rank":-1.0}]"#,
            )
            .create_async()
            .await;

        let hits = client(&server)
            .search(&scope(), "rustls", 10)
            .await
            .expect("search succeeds");

        mock.assert_async().await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].project, "albert");
        // Snippets carry HTML from the server; the renderer must sanitise, so we keep it verbatim.
        assert!(hits[0].snippet.contains("<mark>"));
    }

    #[tokio::test]
    async fn a_page_is_read_from_body_markdown_not_body() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock(
                "GET",
                "/api/v1/workspaces/default/projects/albert/pages/terminal-ai/project/fact/x-1.md",
            )
            .with_status(200)
            .with_body(
                r#"{"path":"terminal-ai/project/fact/x-1.md","title":"X","kind":"fact",
                    "frontmatter":{"terminal_ai_type":"fact"},"body_markdown":"the body",
                    "created_at":"2026-09-03T00:00:00Z","updated_at":"2026-09-03T00:00:00Z"}"#,
            )
            .create_async()
            .await;

        let page = client(&server)
            .read_page(&scope(), "terminal-ai/project/fact/x-1.md")
            .await
            .expect("read succeeds");
        assert_eq!(page.body_markdown, "the body");
    }

    #[tokio::test]
    async fn the_kernels_own_error_message_survives() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(400)
            .with_body(r#"{"error":"q must not be empty"}"#)
            .create_async()
            .await;

        let err = client(&server)
            .search(&scope(), "x", 10)
            .await
            .expect_err("400 is an error");
        match err {
            MemoryError::Upstream { message, .. } => {
                assert_eq!(message, "q must not be empty");
            }
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn status_codes_map_to_distinct_variants() {
        for (status, expect_unauthorized, expect_not_found) in
            [(401, true, false), (403, true, false), (404, false, true)]
        {
            let mut server = mockito::Server::new_async().await;
            server
                .mock("GET", mockito::Matcher::Any)
                .with_status(status)
                .with_body("{}")
                .create_async()
                .await;

            let err = client(&server)
                .search(&scope(), "x", 10)
                .await
                .expect_err("should fail");
            assert_eq!(
                matches!(err, MemoryError::Unauthorized),
                expect_unauthorized,
                "status {status}"
            );
            assert_eq!(
                matches!(err, MemoryError::NotFound),
                expect_not_found,
                "status {status}"
            );
        }
    }

    #[tokio::test]
    async fn a_moved_response_shape_is_a_protocol_error_not_a_crash() {
        // This is what a kernel upgrade looks like from here: valid JSON, wrong shape. It must be
        // distinguishable from "the kernel is down" so the UI can say something useful.
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_body(r#"{"hits":[{"path":"a.md"}]}"#)
            .create_async()
            .await;

        let err = client(&server)
            .search(&scope(), "x", 10)
            .await
            .expect_err("wrong shape should fail");
        assert!(matches!(err, MemoryError::Protocol(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn an_unreachable_kernel_is_a_transport_error() {
        // Port 1 is reserved and nothing listens there.
        let client = ReadClient::new("http://127.0.0.1:1", None).expect("client builds");
        let err = client
            .search(&scope(), "x", 10)
            .await
            .expect_err("should fail");
        assert!(matches!(err, MemoryError::Transport(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_traversal_path_never_reaches_the_network() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .expect(0)
            .create_async()
            .await;

        let err = client(&server)
            .read_page(&scope(), "../../etc/passwd")
            .await
            .expect_err("traversal must be rejected");
        assert!(matches!(err, MemoryError::InvalidPath(_)), "got {err:?}");
        mock.assert_async().await;
    }
}
