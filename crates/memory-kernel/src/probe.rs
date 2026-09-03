//! Deciding whether something listening on the kernel's port *is* the kernel.
//!
//! This deliberately does not use `/api/v1`: that surface is mounted only with `--enable-web`, and
//! a perfectly healthy kernel started without it answers 404 to everything there. Probing it would
//! classify that kernel as an unrelated process — and the app would then try to start a second
//! server on a port that is already in use. `/mcp` is always mounted, so that is what we ask.

use crate::token::AuthToken;
use serde::Deserialize;
use std::time::Duration;

/// What is on the port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// A kernel answered and exposed the tools we expect.
    Kernel,
    /// A kernel answered but rejected our credentials.
    Unauthorized,
    /// Something answered, and it is not a kernel. Do not attach; do not spawn over it.
    Stranger(String),
    /// Nothing is listening.
    Refused,
}

#[derive(Deserialize)]
struct ToolsListResponse {
    result: Option<ToolsResult>,
}

#[derive(Deserialize)]
struct ToolsResult {
    #[serde(default)]
    tools: Vec<Tool>,
}

#[derive(Deserialize)]
struct Tool {
    name: String,
}

/// A tool every ai-memory build exposes. Its presence is what distinguishes the kernel from any
/// other MCP server someone might be running on the same port.
const SENTINEL_TOOL: &str = "memory_query";

/// Ask the endpoint what it is. Never fails: an unreachable port is an answer.
pub async fn probe(server_url: &str, token: Option<&AuthToken>) -> ProbeOutcome {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .no_proxy()
        .build()
    else {
        return ProbeOutcome::Stranger("could not build an HTTP client".into());
    };

    let url = format!("{}/mcp", server_url.trim_end_matches('/'));
    let mut request = client
        .post(&url)
        .header("Content-Type", "application/json")
        // Both media types are required: the server answers 406 with only one of them.
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#);
    if let Some(token) = token {
        request = request.bearer_auth(token.expose());
    }

    let Ok(response) = request.send().await else {
        return ProbeOutcome::Refused;
    };

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return ProbeOutcome::Unauthorized;
    }
    if !status.is_success() {
        return ProbeOutcome::Stranger(format!("answered HTTP {}", status.as_u16()));
    }

    let Ok(body) = response.text().await else {
        return ProbeOutcome::Stranger("sent an unreadable response".into());
    };
    match serde_json::from_str::<ToolsListResponse>(&body) {
        Ok(parsed) => {
            let has_sentinel = parsed
                .result
                .is_some_and(|r| r.tools.iter().any(|t| t.name == SENTINEL_TOOL));
            if has_sentinel {
                ProbeOutcome::Kernel
            } else {
                ProbeOutcome::Stranger(
                    "is an MCP server, but not ai-memory (no memory_query tool)".into(),
                )
            }
        }
        Err(_) => ProbeOutcome::Stranger("is not an MCP server".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_real_kernel_is_recognised() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/mcp")
            .match_header("accept", "application/json, text/event-stream")
            .with_status(200)
            .with_body(r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"memory_query"},{"name":"memory_write_page"}]}}"#)
            .create_async()
            .await;
        assert_eq!(probe(&server.url(), None).await, ProbeOutcome::Kernel);
    }

    #[tokio::test]
    async fn another_mcp_server_is_a_stranger_not_a_kernel() {
        // The dangerous case: something that speaks MCP, so a naive probe would attach to it and
        // the app would start writing the user's memory into someone else's server.
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/mcp")
            .with_status(200)
            .with_body(r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"read_file"}]}}"#)
            .create_async()
            .await;
        assert!(matches!(
            probe(&server.url(), None).await,
            ProbeOutcome::Stranger(_)
        ));
    }

    #[tokio::test]
    async fn a_plain_web_server_is_a_stranger() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/mcp")
            .with_status(200)
            .with_body("<html>hello</html>")
            .create_async()
            .await;
        assert!(matches!(
            probe(&server.url(), None).await,
            ProbeOutcome::Stranger(_)
        ));
    }

    #[tokio::test]
    async fn a_kernel_that_wants_a_token_says_so() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/mcp")
            .with_status(401)
            .create_async()
            .await;
        assert_eq!(probe(&server.url(), None).await, ProbeOutcome::Unauthorized);
    }

    #[tokio::test]
    async fn nothing_listening_is_refused_not_a_stranger() {
        assert_eq!(
            probe("http://127.0.0.1:1", None).await,
            ProbeOutcome::Refused
        );
    }
}
