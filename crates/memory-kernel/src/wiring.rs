//! Connecting agent CLIs to the kernel — and being able to disconnect them again.
//!
//! This is the only part of the feature that writes outside Terminal AI's own data directory, into
//! files it does not own and that merge content from several tools. "Remove only what we created"
//! therefore cannot mean "delete the file"; it means recording precisely what we left behind and
//! refusing to act if someone has since changed it.
//!
//! What is offered per agent was settled by probing a real kernel, not by reading its docs:
//!
//! | agent | MCP | capture |
//! |---|---|---|
//! | `claude-code` | yes | yes, scoped to one project |
//! | `codex` | yes | **no** — upstream has no automatic hook install, only scripts to wire by hand |
//! | `opencode` | yes | **no** — its hook install is a *global* plugin, not confinable to a project |
//!
//! Offering capture for the last two would mean installing machine-wide capture behind a
//! per-project consent, which Principle III forbids. So it is not offered, and the UI says why.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    ClaudeCode,
    Codex,
    OpenCode,
}

impl Agent {
    #[must_use]
    pub const fn cli_value(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "claude-code" | "claude" => Self::ClaudeCode,
            "codex" => Self::Codex,
            "opencode" | "open-code" => Self::OpenCode,
            _ => return None,
        })
    }

    /// Whether lifecycle capture can be confined to a single project for this agent.
    ///
    /// Only Claude Code can, verified against v2.0.2. This is what FR-065 encodes.
    #[must_use]
    pub const fn supports_scoped_capture(self) -> bool {
        matches!(self, Self::ClaudeCode)
    }

    /// Why capture is unavailable, in words a user can act on.
    #[must_use]
    pub const fn capture_unavailable_reason(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => None,
            Self::Codex => Some(
                "Codex has no automatic hook installation, so Terminal AI cannot turn capture on \
                 for one project. Memory is still available to it on demand.",
            ),
            Self::OpenCode => Some(
                "OpenCode installs its hooks as a machine-wide plugin, which cannot be limited to \
                 one project. Memory is still available to it on demand.",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WiringKind {
    /// Register the kernel as an MCP server so the agent can read and write memory on demand.
    Mcp,
    /// Install lifecycle hooks so the agent's own session activity is captured.
    Hooks,
}

impl WiringKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Hooks => "hooks",
        }
    }
}

/// The lifecycle events capture would record. Shown verbatim in the consent step: consent to
/// "capture" means nothing; consent to this list means something.
pub const CAPTURED_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "PreToolUse",
    "PostToolUse",
    "PreCompact",
    "Stop",
    "SubagentStart",
    "SubagentStop",
];

/// One file the app touched, with enough recorded to undo exactly that and no more.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WiringArtifact {
    pub path: PathBuf,
    /// True when the file did not exist before us, so removal may delete it outright.
    pub created_file: bool,
    pub backup_path: Option<PathBuf>,
    pub before_sha256: Option<String>,
    /// What we left behind. Removal compares against this: if it no longer matches, the user has
    /// edited the file since, and restoring a backup would destroy their work.
    pub after_sha256: String,
    /// The kernel binary baked into hook commands. Hooks embed absolute paths, so an app update
    /// that moves the sidecar breaks them silently unless this is checked.
    pub binary_path: Option<PathBuf>,
    pub applied_at: String,
}

/// What an apply would do, shown before anything is written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WiringPlan {
    pub agent: Agent,
    pub kind: WiringKind,
    pub path: Option<PathBuf>,
    pub diff: String,
    pub will_create: bool,
    /// Set when an ai-memory entry is already there and we did not put it there.
    pub conflict: Option<String>,
    pub capture_events: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WiringError {
    #[error("{0}")]
    Unsupported(String),
    /// Something is already configured that Terminal AI did not create. Reported, never
    /// overwritten — the whole point of Principle III.
    #[error("{0} is already configured outside Terminal AI; it will not be changed")]
    Unmanaged(PathBuf),
    /// The file changed after we wrote it. Refusing beats clobbering the user's edit.
    #[error("{0} changed after Terminal AI configured it")]
    Drifted(PathBuf),
    #[error("could not read the kernel's plan: {0}")]
    UnreadablePlan(String),
    #[error("{0}")]
    Io(String),
}

/// Where each kind of wiring is written.
///
/// MCP is global: it only grants on-demand access, it has to reach worktrees and every project, and
/// upstream's `uninstall --only mcp --mcp-url` can remove it precisely. Capture is project-scoped,
/// because consenting for one project must never turn capture on machine-wide.
#[must_use]
pub fn target_path(agent: Agent, kind: WiringKind, project_root: Option<&Path>) -> Option<PathBuf> {
    match kind {
        WiringKind::Mcp => None, // upstream's default location for the client
        WiringKind::Hooks => match agent {
            Agent::ClaudeCode => {
                project_root.map(|root| root.join(".claude").join("settings.json"))
            }
            _ => None,
        },
    }
}

/// Build the argv for a dry run — the kernel prints its plan and writes nothing.
#[must_use]
pub fn plan_args(
    agent: Agent,
    kind: WiringKind,
    server_url: &str,
    config_file: Option<&Path>,
) -> Vec<String> {
    build_args(agent, kind, server_url, config_file, false)
}

/// The same command with `--apply`.
#[must_use]
pub fn apply_args(
    agent: Agent,
    kind: WiringKind,
    server_url: &str,
    config_file: Option<&Path>,
) -> Vec<String> {
    build_args(agent, kind, server_url, config_file, true)
}

fn build_args(
    agent: Agent,
    kind: WiringKind,
    server_url: &str,
    config_file: Option<&Path>,
    apply: bool,
) -> Vec<String> {
    let mut args: Vec<String> = match kind {
        WiringKind::Mcp => vec![
            "install-mcp".into(),
            "--client".into(),
            agent.cli_value().into(),
        ],
        WiringKind::Hooks => {
            let mut args = vec![
                "install-hooks".into(),
                "--agent".into(),
                agent.cli_value().into(),
                // Folds worktrees into the parent repository's project, so sibling worktrees share
                // memory instead of fragmenting it — and without writing a marker into the tree.
                "--project-strategy".into(),
                "repo-root".into(),
            ];
            if agent.supports_scoped_capture() {
                // Claude-Code-only flag; passing it elsewhere makes the command exit 1.
                args.push("--no-capture-prompts".into());
            }
            args
        }
    };
    args.push("--server-url".into());
    args.push(server_url.to_owned());
    if let Some(path) = config_file {
        args.push("--config-file".into());
        args.push(path.to_string_lossy().into_owned());
    }
    if apply {
        args.push("--apply".into());
    }
    args
}

/// The argv that removes what we installed, using upstream's own remover.
///
/// `--mcp-url` matters: upstream never matches an entry by name alone, so identifying ours by the
/// endpoint it points at is what keeps removal from touching someone else's server entry.
#[must_use]
pub fn uninstall_args(kind: WiringKind, server_url: &str, apply: bool) -> Vec<String> {
    let mut args = vec![
        "uninstall".into(),
        "--only".into(),
        kind.as_str().to_owned(),
        "--mcp-url".into(),
        format!("{}/mcp", server_url.trim_end_matches('/')),
    ];
    if apply {
        args.push("--apply".into());
        args.push("--yes".into());
    }
    args
}

/// Pull the JSON block out of the kernel's human-readable plan.
///
/// The plan's prose is not a contract — it is output meant for a person. We take only the JSON
/// object from it, and compute the diff from the file itself, so a change to the surrounding text
/// degrades the preview rather than corrupting an apply.
///
/// # Errors
/// Fails when no JSON object can be found or parsed.
pub fn extract_config_block(stdout: &str) -> Result<serde_json::Value, WiringError> {
    let start = stdout
        .find('{')
        .ok_or_else(|| WiringError::UnreadablePlan("no configuration block was printed".into()))?;
    let candidate = &stdout[start..];
    let mut depth = 0usize;
    let mut end = None;
    for (index, ch) in candidate.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(index + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| WiringError::UnreadablePlan("unbalanced JSON block".into()))?;
    serde_json::from_str(&candidate[..end]).map_err(|e| WiringError::UnreadablePlan(e.to_string()))
}

/// Does this configuration already point at an ai-memory server we did not install?
#[must_use]
pub fn has_unmanaged_entry(existing: &serde_json::Value, server_url: &str) -> bool {
    let needle = server_url.trim_end_matches('/');
    fn walk(value: &serde_json::Value, needle: &str) -> bool {
        match value {
            serde_json::Value::String(s) => s.contains(needle),
            serde_json::Value::Array(items) => items.iter().any(|v| walk(v, needle)),
            serde_json::Value::Object(map) => map.values().any(|v| walk(v, needle)),
            _ => false,
        }
    }
    walk(existing, needle)
}

/// A readable before/after. Line-based and deliberately simple: it exists so a person can see what
/// changes, not to be applied by a machine.
#[must_use]
pub fn diff(before: &str, after: &str) -> String {
    if before == after {
        return "(no change)".to_owned();
    }
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut out = String::new();
    for line in &before_lines {
        if !after_lines.contains(line) {
            out.push_str(&format!("- {line}\n"));
        }
    }
    for line in &after_lines {
        if !before_lines.contains(line) {
            out.push_str(&format!("+ {line}\n"));
        }
    }
    if out.is_empty() {
        out.push_str("(only ordering changed)\n");
    }
    out
}

#[must_use]
pub fn hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Whether removal may safely act on this artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalPlan {
    /// We created the file and it is untouched: delete it.
    DeleteFile,
    /// We merged into someone else's file and it is untouched: restore the backup.
    RestoreBackup(PathBuf),
    /// Already gone.
    NothingToDo,
    /// Changed since we wrote it. Refuse, and hand the user the backup path.
    Refuse(WiringRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiringRefusal {
    pub path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub reason: String,
}

/// Decide how to undo one artifact, given what the file looks like now.
///
/// Pure, so the case that matters — a user edited the file after we wrote it — is exercised by a
/// test rather than discovered by someone losing their configuration.
#[must_use]
pub fn plan_removal(artifact: &WiringArtifact, current: Option<&str>) -> RemovalPlan {
    let Some(current) = current else {
        return RemovalPlan::NothingToDo;
    };
    if hash(current) != artifact.after_sha256 {
        return RemovalPlan::Refuse(WiringRefusal {
            path: artifact.path.clone(),
            backup_path: artifact.backup_path.clone(),
            reason: "the file has changed since Terminal AI configured it".to_owned(),
        });
    }
    if artifact.created_file {
        RemovalPlan::DeleteFile
    } else if let Some(backup) = artifact.backup_path.clone() {
        RemovalPlan::RestoreBackup(backup)
    } else {
        // Merged into a pre-existing file with no backup: there is nothing safe to do, so say so
        // instead of guessing.
        RemovalPlan::Refuse(WiringRefusal {
            path: artifact.path.clone(),
            backup_path: None,
            reason: "no backup was recorded for this file".to_owned(),
        })
    }
}

/// Has the sidecar moved since this wiring was applied? Hook commands bake an absolute path, so a
/// moved binary breaks capture with no error anywhere.
#[must_use]
pub fn is_stale(artifact: &WiringArtifact, current_binary: Option<&Path>) -> bool {
    match (&artifact.binary_path, current_binary) {
        (Some(recorded), Some(current)) => recorded != current,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(created: bool, after: &str, backup: Option<&str>) -> WiringArtifact {
        WiringArtifact {
            path: PathBuf::from("/Users/x/.claude/settings.json"),
            created_file: created,
            backup_path: backup.map(PathBuf::from),
            before_sha256: None,
            after_sha256: hash(after),
            binary_path: Some(PathBuf::from("/Applications/Terminal AI.app/ai-memory")),
            applied_at: "2026-09-03T00:00:00Z".into(),
        }
    }

    #[test]
    fn capture_is_offered_only_where_it_can_be_confined_to_one_project() {
        // FR-065. Verified against the real kernel: Codex has no automatic hook install at all,
        // and OpenCode's is a machine-wide plugin. Offering either behind a per-project consent
        // would be installing something broader than what the user agreed to.
        assert!(Agent::ClaudeCode.supports_scoped_capture());
        assert!(!Agent::Codex.supports_scoped_capture());
        assert!(!Agent::OpenCode.supports_scoped_capture());

        // And when it is not offered, the user is told why rather than left guessing.
        assert!(Agent::ClaudeCode.capture_unavailable_reason().is_none());
        for agent in [Agent::Codex, Agent::OpenCode] {
            let reason = agent.capture_unavailable_reason().expect("a reason");
            assert!(
                reason.contains("on demand"),
                "{agent:?} must say what still works"
            );
        }
    }

    #[test]
    fn the_prompt_capture_flag_is_only_sent_to_claude_code() {
        // Sending it elsewhere makes the kernel exit 1: other agents deliver handoff context
        // through their prompt hook, so it cannot be removed there.
        let claude = plan_args(
            Agent::ClaudeCode,
            WiringKind::Hooks,
            "http://127.0.0.1:49374",
            None,
        );
        assert!(claude.iter().any(|a| a == "--no-capture-prompts"));

        for agent in [Agent::Codex, Agent::OpenCode] {
            let args = plan_args(agent, WiringKind::Hooks, "http://127.0.0.1:49374", None);
            assert!(
                !args.iter().any(|a| a == "--no-capture-prompts"),
                "{agent:?}"
            );
        }
    }

    #[test]
    fn a_preview_never_carries_apply() {
        // The single most important property of the preview: it writes nothing.
        for kind in [WiringKind::Mcp, WiringKind::Hooks] {
            let args = plan_args(Agent::ClaudeCode, kind, "http://127.0.0.1:49374", None);
            assert!(!args.iter().any(|a| a == "--apply"), "{kind:?}");
            let applied = apply_args(Agent::ClaudeCode, kind, "http://127.0.0.1:49374", None);
            assert!(applied.iter().any(|a| a == "--apply"), "{kind:?}");
        }
    }

    #[test]
    fn worktrees_fold_into_the_parent_project() {
        // Achieved with a flag rather than a `.ai-memory.toml` in the working tree, which would
        // make the worktree dirty and break `remove_worktree`.
        let args = plan_args(Agent::ClaudeCode, WiringKind::Hooks, "http://x", None);
        let index = args
            .iter()
            .position(|a| a == "--project-strategy")
            .expect("flag");
        assert_eq!(args[index + 1], "repo-root");
    }

    #[test]
    fn capture_is_written_per_project_and_mcp_is_not() {
        let root = PathBuf::from("/Users/x/www/albert");
        assert_eq!(
            target_path(Agent::ClaudeCode, WiringKind::Hooks, Some(&root)),
            Some(root.join(".claude").join("settings.json")),
            "consenting for one project must not enable capture machine-wide"
        );
        // MCP only grants on-demand access and has to reach worktrees too, so it uses the client's
        // own default location and is removed by URL.
        assert_eq!(
            target_path(Agent::ClaudeCode, WiringKind::Mcp, Some(&root)),
            None
        );
    }

    #[test]
    fn removal_identifies_our_entry_by_url_not_by_name() {
        // Upstream never matches an MCP entry by name alone; passing the URL is what stops removal
        // touching a server entry someone else configured.
        let args = uninstall_args(WiringKind::Mcp, "http://127.0.0.1:49374", true);
        let index = args.iter().position(|a| a == "--mcp-url").expect("flag");
        assert_eq!(args[index + 1], "http://127.0.0.1:49374/mcp");
        assert!(args.iter().any(|a| a == "--apply"));
        assert!(!uninstall_args(WiringKind::Mcp, "http://x", false)
            .iter()
            .any(|a| a == "--apply"));
    }

    #[test]
    fn a_file_edited_after_we_wrote_it_is_never_clobbered() {
        // The property that matters most in this module. Restoring a backup over a user's later
        // edit would destroy work they did on a file that is theirs, not ours.
        let art = artifact(false, "what we wrote", Some("/tmp/backup.json"));

        match plan_removal(&art, Some("what we wrote")) {
            RemovalPlan::RestoreBackup(path) => {
                assert_eq!(path, PathBuf::from("/tmp/backup.json"));
            }
            other => panic!("untouched file should restore, got {other:?}"),
        }

        match plan_removal(&art, Some("what we wrote, plus the user's own edit")) {
            RemovalPlan::Refuse(refusal) => {
                assert_eq!(refusal.backup_path, Some(PathBuf::from("/tmp/backup.json")));
                assert!(!refusal.reason.is_empty());
            }
            other => panic!("an edited file must be refused, got {other:?}"),
        }
    }

    #[test]
    fn a_file_we_created_is_deleted_and_a_missing_one_is_a_no_op() {
        assert_eq!(
            plan_removal(&artifact(true, "ours alone", None), Some("ours alone")),
            RemovalPlan::DeleteFile
        );
        assert_eq!(
            plan_removal(&artifact(true, "ours alone", None), None),
            RemovalPlan::NothingToDo
        );
    }

    #[test]
    fn merging_into_a_file_with_no_backup_is_refused_rather_than_guessed() {
        match plan_removal(&artifact(false, "merged", None), Some("merged")) {
            RemovalPlan::Refuse(refusal) => assert!(refusal.reason.contains("backup")),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_entry_we_did_not_create_is_detected() {
        let existing: serde_json::Value = serde_json::from_str(
            r#"{"mcpServers":{"ai-memory":{"type":"http","url":"http://127.0.0.1:49374/mcp"}}}"#,
        )
        .expect("valid json");
        assert!(has_unmanaged_entry(&existing, "http://127.0.0.1:49374"));

        let unrelated: serde_json::Value =
            serde_json::from_str(r#"{"mcpServers":{"other":{"url":"http://localhost:3000"}}}"#)
                .expect("valid json");
        assert!(!has_unmanaged_entry(&unrelated, "http://127.0.0.1:49374"));
    }

    #[test]
    fn the_config_block_is_extracted_from_human_readable_output() {
        // The kernel's dry run is prose written for a person, with a JSON block inside. We take
        // only the block; if the prose changes, the preview degrades instead of misapplying.
        let stdout = "# Claude Code — register the MCP server\n\
                      #\n# Recommended:\nclaude mcp add --transport http ai-memory http://x/mcp\n\
                      #\n# Equivalent JSON:\n\
                      {\n  \"mcpServers\": {\n    \"ai-memory\": {\"url\": \"http://x/mcp\"}\n  }\n}\n";
        let block = extract_config_block(stdout).expect("block extracted");
        assert!(block.get("mcpServers").is_some());
    }

    #[test]
    fn unreadable_output_is_an_error_not_a_guess() {
        assert!(extract_config_block("no json here at all").is_err());
        assert!(extract_config_block("{ unbalanced").is_err());
    }

    #[test]
    fn a_moved_sidecar_marks_wiring_stale() {
        // Hook commands bake an absolute binary path, so an app update silently breaks capture
        // unless this is noticed and re-applied.
        let art = artifact(true, "x", None);
        assert!(!is_stale(
            &art,
            Some(Path::new("/Applications/Terminal AI.app/ai-memory"))
        ));
        assert!(is_stale(
            &art,
            Some(Path::new("/Applications/Terminal AI 2.app/ai-memory"))
        ));
        assert!(is_stale(&art, None), "no binary at all is also stale");
    }

    #[test]
    fn the_diff_shows_both_sides() {
        let out = diff("a\nb\n", "a\nc\n");
        assert!(out.contains("- b"));
        assert!(out.contains("+ c"));
        assert_eq!(diff("same", "same"), "(no change)");
    }
}
