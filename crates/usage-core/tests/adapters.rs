use serde_json::json;
use terminal_ai_usage_core::{
    adapters::{parse_anthropic, parse_codex, OpenCodeAdapter},
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

/// OpenCode has no quota endpoint; usage comes from the token counts it records for every
/// message in its own database. The adapter must read that read-only and survive rows whose
/// `data` is not the shape it expects.
#[tokio::test]
async fn opencode_adapter_sums_tokens_from_the_local_database() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("opencode.db");
    let now = chrono::Utc::now().timestamp_millis();
    {
        let connection = rusqlite::Connection::open(&path).expect("create db");
        connection
            .execute(
                "CREATE TABLE message (id TEXT PRIMARY KEY, time_created INTEGER NOT NULL, data TEXT NOT NULL)",
                [],
            )
            .expect("schema");
        let insert = |id: &str, created: i64, data: &str| {
            connection
                .execute(
                    "INSERT INTO message(id,time_created,data) VALUES(?1,?2,?3)",
                    rusqlite::params![id, created, data],
                )
                .expect("insert");
        };
        insert("a", now, r#"{"tokens":{"input":1200,"output":300}}"#);
        insert("b", now, r#"{"tokens":{"input":800,"output":200}}"#);
        // Outside the 24h window: counted in 7d only.
        insert(
            "c",
            now - 1000 * 60 * 60 * 48,
            r#"{"tokens":{"input":5,"output":5}}"#,
        );
        // Rows without token data must be skipped, not abort the read.
        insert("d", now, r#"{"role":"user"}"#);
        insert("e", now, "not json at all");
    }
    let card = OpenCodeAdapter::new()
        .with_database_path(path)
        .fetch()
        .await
        .expect("reads the local database");
    assert_eq!(card.label, "OpenCode");
    assert_eq!(card.lines[0].label, "24h");
    assert_eq!(card.lines[0].value, "2,0k \u{2191} 500 \u{2193}");
    assert_eq!(card.lines[1].label, "7d");
    assert_eq!(card.lines[1].value, "2,0k \u{2191} 505 \u{2193}");
    assert_eq!(card.lines[2].value, "3");
    // Nothing is authenticated: the reading is local.
    assert_eq!(card.auth, terminal_ai_usage_core::AuthState::Ok);
}
