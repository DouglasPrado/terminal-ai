mod anthropic;
mod codex;
mod opencode;

pub use anthropic::{parse_anthropic, AnthropicAdapter};
pub use codex::{parse_codex, CodexAdapter};
pub use opencode::OpenCodeAdapter;

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
