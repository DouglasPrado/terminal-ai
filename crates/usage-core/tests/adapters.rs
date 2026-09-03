use serde_json::json;
use terminal_ai_usage_core::{
    adapters::{parse_anthropic, parse_codex, parse_openrouter, OpenRouterAdapter},
    UsageAdapter,
};

#[test]
fn parses_anthropic_windows() {
    let card = parse_anthropic(&json!({
        "five_hour": {"utilization": 23.0, "resets_at": "2026-07-14T20:00:00Z"},
        "seven_day": {"utilization": 61.5, "resets_at": "2026-07-20T00:00:00Z"},
        "seven_day_sonnet": {"utilization": 0.12}
    }))
    .expect("valid Anthropic response");
    insta::assert_json_snapshot!(card, @r###"
    {
      "label": "Claude",
      "lines": [
        {
          "label": "Sessão",
          "value": "23%",
          "pct": 23.0,
          "resetsAt": "2026-07-14T20:00:00Z"
        },
        {
          "label": "Semanal",
          "value": "62%",
          "pct": 61.5,
          "resetsAt": "2026-07-20T00:00:00Z"
        },
        {
          "label": "Sonnet",
          "value": "12%",
          "pct": 12.0
        }
      ],
      "auth": "ok",
      "stale": false
    }
    "###);
}

#[test]
fn parses_codex_windows() {
    let card = parse_codex(&json!({
        "rate_limit": {
            "primary_window": {"used_percent": 42, "reset_at": 1784066400},
            "secondary_window": {"used_percent": 8, "resets_at": "2026-07-20T00:00:00Z"}
        },
        "code_review_rate_limit": {"primary_window": {"used_percent": 3}}
    }))
    .expect("valid Codex response");
    insta::assert_json_snapshot!(card, @r###"
    {
      "label": "Codex",
      "lines": [
        {
          "label": "5 horas",
          "value": "42%",
          "pct": 42.0,
          "resetsAt": "1784066400"
        },
        {
          "label": "Semanal",
          "value": "8%",
          "pct": 8.0,
          "resetsAt": "2026-07-20T00:00:00Z"
        },
        {
          "label": "Review",
          "value": "3%",
          "pct": 3.0
        }
      ],
      "auth": "ok",
      "stale": false
    }
    "###);
}

#[test]
fn parses_openrouter_balance() {
    let card = parse_openrouter(&json!({"data": {"usage": 12.5, "limit": 50.0}}))
        .expect("valid OpenRouter response");
    insta::assert_json_snapshot!(card, @r###"
    {
      "label": "OpenCode · OpenRouter",
      "lines": [
        {
          "label": "Uso",
          "value": "$12.50",
          "pct": 25.0
        },
        {
          "label": "Saldo",
          "value": "$37.50"
        }
      ],
      "auth": "ok",
      "stale": false
    }
    "###);
}

#[tokio::test]
async fn openrouter_adapter_fetches_once_and_parses() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/v1/auth/key")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":{"usage":1.25,"limit":10}}"#)
        .expect(1)
        .create_async()
        .await;
    let adapter = OpenRouterAdapter::new(reqwest::Client::new())
        .with_base_url(server.url())
        .with_api_key("test-key");
    let card = adapter.fetch().await.expect("mocked fetch succeeds");
    assert_eq!(card.lines[0].value, "$1.25");
    mock.assert_async().await;
}
