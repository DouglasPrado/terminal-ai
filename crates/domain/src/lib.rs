//! Pure domain model shared by all Terminal AI runtime crates.
#![forbid(unsafe_code)]

pub mod host;
pub mod invisible_mode;
pub mod memory;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(ProjectId);
id_type!(WorktreeId);
id_type!(WorkspaceId);
id_type!(PaneId);
id_type!(SessionId);
id_type!(ProviderId);
id_type!(SkillId);
id_type!(MemoryId);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayoutNode {
    Pane {
        #[serde(rename = "paneId")]
        pane_id: PaneId,
    },
    Split {
        direction: SplitDirection,
        sizes: Vec<f64>,
        children: Vec<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Pane { pane_id } if pane_id.0.trim().is_empty() => {
                Err(DomainError::InvalidLayout("pane id cannot be empty".into()))
            }
            Self::Pane { .. } => Ok(()),
            Self::Split {
                sizes, children, ..
            } => {
                if children.len() < 2 || sizes.len() != children.len() {
                    return Err(DomainError::InvalidLayout(
                        "split sizes must align with at least two children".into(),
                    ));
                }
                if sizes.iter().any(|size| !(0.0..=100.0).contains(size)) {
                    return Err(DomainError::InvalidLayout(
                        "split size must be between 0 and 100".into(),
                    ));
                }
                let sum: f64 = sizes.iter().sum();
                if (sum - 100.0).abs() > 0.1 {
                    return Err(DomainError::InvalidLayout(
                        "split sizes must sum to 100".into(),
                    ));
                }
                children.iter().try_for_each(Self::validate)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Starting,
    Running,
    Exited,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeLevel {
    Global,
    Project,
    Worktree,
    Workspace,
    Session,
}
impl ScopeLevel {
    pub const fn precedence(self) -> i32 {
        match self {
            Self::Global => 0,
            Self::Project => 1,
            Self::Worktree => 2,
            Self::Workspace => 3,
            Self::Session => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub level: ScopeLevel,
    pub ref_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Constraint,
    Preference,
    Glossary,
    KnownIssue,
    Command,
    Todo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Builtin,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: ProviderId,
    pub label: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub color: Option<String>,
    pub kind: ProviderKind,
}

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("invalid layout: {0}")]
    InvalidLayout(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_misaligned_layout() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            sizes: vec![100.0],
            children: vec![
                LayoutNode::Pane {
                    pane_id: PaneId::new(),
                },
                LayoutNode::Pane {
                    pane_id: PaneId::new(),
                },
            ],
        };
        assert!(node.validate().is_err());
    }
}
