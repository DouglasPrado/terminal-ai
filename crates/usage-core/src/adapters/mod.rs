mod anthropic;
mod codex;
mod openrouter;

pub use anthropic::{parse_anthropic, AnthropicAdapter};
pub use codex::{parse_codex, CodexAdapter};
pub use openrouter::{parse_openrouter, OpenRouterAdapter};

use serde_json::Value;

fn number_at(value: &Value, pointers: &[&str]) -> Option<f64> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_f64))
}

fn string_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn percent(used: f64, limit: Option<f64>) -> Option<f64> {
    limit
        .filter(|limit| *limit > 0.0)
        .map(|limit| (used / limit * 100.0).clamp(0.0, 100.0))
}

fn compact(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}
