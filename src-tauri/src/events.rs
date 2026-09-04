use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalChunk {
    pub session_id: String,
    pub seq: u64,
    pub bytes: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostErrorEvent {
    pub scope: String,
    pub session_id: Option<String>,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExited {
    pub session_id: String,
    pub exit_code: Option<i32>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusChanged {
    pub project_id: String,
    pub worktree_id: Option<String>,
    pub branch: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
}

pub fn sanitize_title(input: &str) -> String {
    input
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect()
}
