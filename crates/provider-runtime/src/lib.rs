//! Provider adapters and launch specifications.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use terminal_ai_domain::{
    host::{CommandSpec, LaunchContext, ResumeCapability, ResumeRef},
    ProviderId, ProviderKind, ProviderProfile,
};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthState {
    Ok,
    Expired,
    Unknown,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub detected: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub auth: AuthState,
    pub message: Option<String>,
}

#[async_trait]
pub trait AgentProvider: Send + Sync {
    fn profile(&self) -> &ProviderProfile;
    fn resume_capability(&self) -> ResumeCapability;
    async fn command(
        &self,
        context: &LaunchContext,
        resolved_env: &BTreeMap<String, String>,
    ) -> Result<CommandSpec, ProviderError>;
    fn detect(&self, resolved_env: &BTreeMap<String, String>) -> Detection;
}

#[derive(Debug, Clone)]
pub struct ProviderAdapter {
    profile: ProviderProfile,
    resume: ResumeCapability,
}
impl ProviderAdapter {
    pub fn new(profile: ProviderProfile, resume: ResumeCapability) -> Self {
        Self { profile, resume }
    }
}

#[async_trait]
impl AgentProvider for ProviderAdapter {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }
    fn resume_capability(&self) -> ResumeCapability {
        self.resume
    }
    async fn command(
        &self,
        context: &LaunchContext,
        resolved_env: &BTreeMap<String, String>,
    ) -> Result<CommandSpec, ProviderError> {
        let program = resolve_executable(&self.profile.command, resolved_env)
            .ok_or_else(|| ProviderError::Missing(self.profile.command.display().to_string()))?;
        let mut args = self.profile.args.clone();
        if let Some(resume) = &context.resume {
            match (self.profile.id.0.as_str(), resume) {
                ("claude", ResumeRef::Continue) => args.push("--continue".into()),
                ("claude", ResumeRef::ById(id)) => {
                    args.push("--resume".into());
                    args.push(id.clone());
                }
                ("codex", ResumeRef::Continue) => {
                    args.push("resume".into());
                    args.push("--last".into());
                }
                ("codex", ResumeRef::ById(id)) => {
                    args.push("resume".into());
                    args.push(id.clone());
                }
                ("opencode", ResumeRef::Continue) => args.push("--continue".into()),
                ("opencode", ResumeRef::ById(id)) => {
                    args.push("--session".into());
                    args.push(id.clone());
                }
                (_, _) => return Err(ProviderError::ResumeUnsupported),
            }
        }
        let mut env: Vec<_> = resolved_env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        env.extend(self.profile.env.clone());
        Ok(CommandSpec {
            program,
            args,
            cwd: context.cwd.clone(),
            env,
        })
    }
    fn detect(&self, resolved_env: &BTreeMap<String, String>) -> Detection {
        match resolve_executable(&self.profile.command,resolved_env){Some(path)=>Detection{detected:true,path:Some(path),version:None,auth:AuthState::Unknown,message:None},None=>Detection{detected:false,path:None,version:None,auth:AuthState::Unknown,message:Some(format!("{} was not found on the resolved login-shell PATH. Install it and refresh the environment.",self.profile.command.display()))}}
    }
}

pub fn builtin_providers() -> Vec<ProviderAdapter> {
    vec![
        ProviderAdapter::new(
            profile("claude", "Claude", "claude", "#e879f9"),
            ResumeCapability::ResumeById,
        ),
        ProviderAdapter::new(
            profile("codex", "Codex", "codex", "#22d3ee"),
            ResumeCapability::ResumeById,
        ),
        ProviderAdapter::new(
            profile("opencode", "OpenCode", "opencode", "#a78bfa"),
            ResumeCapability::ResumeById,
        ),
        ProviderAdapter::new(
            ProviderProfile {
                id: ProviderId("shell".into()),
                label: "Shell".into(),
                command: PathBuf::from("/bin/zsh"),
                args: vec!["-l".into()],
                env: vec![],
                color: Some("#a3a3a3".into()),
                kind: ProviderKind::Builtin,
            },
            ResumeCapability::None,
        ),
    ]
}
fn profile(id: &str, label: &str, command: &str, color: &str) -> ProviderProfile {
    ProviderProfile {
        id: ProviderId(id.into()),
        label: label.into(),
        command: PathBuf::from(command),
        args: vec![],
        env: vec![],
        color: Some(color.into()),
        kind: ProviderKind::Builtin,
    }
}

pub fn resolve_executable(command: &Path, env: &BTreeMap<String, String>) -> Option<PathBuf> {
    if command.is_absolute() {
        return command.is_file().then(|| command.to_path_buf());
    }
    env.get("PATH")?
        .split(':')
        .map(Path::new)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}
impl TryFrom<ProfileInput> for ProviderProfile {
    type Error = ProviderError;
    fn try_from(value: ProfileInput) -> Result<Self, Self::Error> {
        if value.id.trim().is_empty() || value.command.trim().is_empty() {
            return Err(ProviderError::InvalidProfile);
        }
        Ok(Self {
            id: ProviderId(value.id),
            label: value.label,
            command: PathBuf::from(value.command),
            args: value.args,
            env: value.env.into_iter().collect(),
            color: value.color,
            kind: ProviderKind::Custom,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider executable missing: {0}")]
    Missing(String),
    #[error("provider does not support the requested resume mode")]
    ResumeUnsupported,
    #[error("provider profile id and command are required")]
    InvalidProfile,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_verified_resume_flags() {
        let env = BTreeMap::from([("PATH".into(), "/bin:/usr/bin".into())]);
        let adapter = ProviderAdapter::new(
            profile("claude", "Claude", "sh", "#fff"),
            ResumeCapability::ResumeById,
        );
        let ctx = LaunchContext {
            project_id: None,
            worktree_id: None,
            provider_id: ProviderId("claude".into()),
            cwd: PathBuf::from("/tmp"),
            cols: 80,
            rows: 24,
            resume: Some(ResumeRef::ById("abc".into())),
        };
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let spec = runtime
            .block_on(adapter.command(&ctx, &env))
            .expect("command");
        assert_eq!(spec.args, vec!["--resume", "abc"]);
    }
}
