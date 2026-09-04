//! Non-destructive skill discovery and provider synchronization.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use terminal_ai_domain::ScopeLevel;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub providers: Vec<String>,
    pub instructions_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub level: ScopeLevel,
    pub ref_id: Option<String>,
}

impl Scope {
    pub fn validate(&self) -> Result<(), SkillError> {
        if self.level == ScopeLevel::Global && self.ref_id.is_some()
            || self.level != ScopeLevel::Global && self.ref_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(SkillError::InvalidScope);
        }
        Ok(())
    }

    pub const fn precedence(&self) -> i32 {
        self.level.precedence()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppliedArtifact {
    pub provider_id: String,
    pub path: PathBuf,
    pub marker: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPreview {
    pub diff: String,
    pub will_create: Vec<PathBuf>,
    pub artifact: AppliedArtifact,
    pub compiled: String,
}

pub fn scan(root: &Path) -> Result<Vec<Skill>, SkillError> {
    std::fs::create_dir_all(root)?;
    let mut skills = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let directory = entry?.path();
        if directory.is_dir() {
            if let Some(skill) = parse_skill_dir(&directory) {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

/// Discover skills across several roots (read-only; never creates directories), de-duplicated by
/// slug. Supports the app's `skill.toml` format AND Claude's `SKILL.md` (YAML frontmatter), so a
/// user's existing `.claude/skills` appear alongside app-managed skills.
pub fn discover(roots: &[PathBuf]) -> Vec<Skill> {
    let mut skills: Vec<Skill> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let Ok(read) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in read.flatten() {
            let directory = entry.path();
            if !directory.is_dir() {
                continue;
            }
            if let Some(skill) = parse_skill_dir(&directory) {
                if seen.insert(skill.slug.clone()) {
                    skills.push(skill);
                }
            }
        }
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

/// Parse a single skill directory: prefer app `skill.toml` + `instructions.md`, otherwise fall back
/// to a Claude-format `SKILL.md` (YAML frontmatter). Returns None if neither is present.
fn parse_skill_dir(directory: &Path) -> Option<Skill> {
    let slug = directory
        .file_name()
        .and_then(|value| value.to_str())?
        .to_owned();
    if validate_slug(&slug).is_err() {
        return None;
    }
    let manifest_path = directory.join("skill.toml");
    let instructions_path = directory.join("instructions.md");
    if manifest_path.is_file() && instructions_path.is_file() {
        let manifest = std::fs::read_to_string(&manifest_path).ok()?;
        return Some(Skill {
            id: field(&manifest, "id").unwrap_or_else(|| slug.clone()),
            name: field(&manifest, "name").unwrap_or_else(|| slug.clone()),
            version: field(&manifest, "version").unwrap_or_else(|| "0.1.0".into()),
            description: field(&manifest, "description"),
            providers: array_field(&manifest, "providers"),
            slug,
            instructions_path,
        });
    }
    let skill_md = directory.join("SKILL.md");
    if skill_md.is_file() {
        let content = std::fs::read_to_string(&skill_md).ok()?;
        return Some(Skill {
            id: slug.clone(),
            name: frontmatter(&content, "name").unwrap_or_else(|| slug.clone()),
            version: "—".into(),
            description: frontmatter(&content, "description"),
            providers: vec!["claude".into(), "codex".into(), "opencode".into()],
            slug,
            instructions_path: skill_md,
        });
    }
    None
}

/// Extract a `key: value` field from a leading `---` YAML frontmatter block.
fn frontmatter(content: &str, key: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let prefix = format!("{key}:");
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(value) = trimmed.strip_prefix(&prefix) {
            let value = value.trim().trim_matches('"').trim_matches('\'').trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

pub fn preview(
    skill: &Skill,
    provider_id: &str,
    scope: &Scope,
    target_root: &Path,
) -> Result<ApplyPreview, SkillError> {
    scope.validate()?;
    if !skill.providers.is_empty() && !skill.providers.iter().any(|id| id == provider_id) {
        return Err(SkillError::UnsupportedProvider(provider_id.into()));
    }
    let path = provider_path(target_root, provider_id, &skill.slug)?;
    let marker = format!("terminal-ai-skill:{}", skill.id);
    let instructions = std::fs::read_to_string(&skill.instructions_path)?;
    let compiled = format!(
        "<!-- {marker} -->\n# {}\n\n{}\n",
        skill.name,
        instructions.trim()
    );
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    if !previous.is_empty() && !previous.contains(&marker) {
        return Err(SkillError::UnmanagedConflict(path));
    }
    let diff = simple_diff(&previous, &compiled);
    Ok(ApplyPreview {
        diff,
        will_create: if path.exists() {
            vec![]
        } else {
            vec![path.clone()]
        },
        artifact: AppliedArtifact {
            provider_id: provider_id.into(),
            path,
            marker,
        },
        compiled,
    })
}

pub fn apply(preview: &ApplyPreview) -> Result<AppliedArtifact, SkillError> {
    if let Some(parent) = preview.artifact.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = preview.artifact.path.with_extension("terminal-ai.tmp");
    std::fs::write(&temporary, &preview.compiled)?;
    std::fs::rename(temporary, &preview.artifact.path)?;
    Ok(preview.artifact.clone())
}

pub fn remove(artifact: &AppliedArtifact) -> Result<bool, SkillError> {
    if !artifact.path.exists() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(&artifact.path)?;
    if !contents.contains(&artifact.marker) {
        return Err(SkillError::UnmanagedConflict(artifact.path.clone()));
    }
    std::fs::remove_file(&artifact.path)?;
    Ok(true)
}

fn provider_path(root: &Path, provider: &str, slug: &str) -> Result<PathBuf, SkillError> {
    validate_slug(slug)?;
    match provider {
        "claude" => Ok(root
            .join(".claude/commands")
            .join(format!("terminal-ai-{slug}.md"))),
        "codex" => Ok(root.join(".codex/skills").join(slug).join("SKILL.md")),
        "opencode" => Ok(root
            .join(".opencode/commands")
            .join(format!("terminal-ai-{slug}.md"))),
        other => Err(SkillError::UnsupportedProvider(other.into())),
    }
}

fn validate_slug(slug: &str) -> Result<(), SkillError> {
    if slug.is_empty()
        || slug
            .chars()
            .any(|character| !character.is_ascii_alphanumeric() && character != '-')
    {
        return Err(SkillError::InvalidSlug(slug.into()));
    }
    Ok(())
}

fn field(input: &str, key: &str) -> Option<String> {
    input.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn array_field(input: &str, key: &str) -> Vec<String> {
    field(input, key)
        .map(|value| {
            value
                .trim_matches(['[', ']'])
                .split(',')
                .map(|part| part.trim().trim_matches('"'))
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn simple_diff(before: &str, after: &str) -> String {
    let removed = before.lines().map(|line| format!("- {line}"));
    let added = after.lines().map(|line| format!("+ {line}"));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("invalid skill manifest")]
    InvalidManifest,
    #[error("invalid skill slug: {0}")]
    InvalidSlug(String),
    #[error("scope requires the appropriate reference id")]
    InvalidScope,
    #[error("provider is not supported by this skill: {0}")]
    UnsupportedProvider(String),
    #[error("refusing to overwrite or remove an unmanaged file: {0}")]
    UnmanagedConflict(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_a_marked_app_managed_file() {
        let root = std::env::temp_dir().join(format!("terminal-ai-skill-{}", std::process::id()));
        let library = root.join("library/example");
        std::fs::create_dir_all(&library).expect("library");
        std::fs::write(
            library.join("skill.toml"),
            "name = \"Example\"\nversion = \"1.0.0\"\nproviders = [\"claude\"]\n",
        )
        .expect("manifest");
        std::fs::write(library.join("instructions.md"), "Always verify.").expect("instructions");
        let skill = scan(&root.join("library")).expect("scan").remove(0);
        let scope = Scope {
            level: ScopeLevel::Global,
            ref_id: None,
        };
        let preview = preview(&skill, "claude", &scope, &root.join("target")).expect("preview");
        let artifact = apply(&preview).expect("apply");
        assert!(artifact.path.is_file());
        assert!(remove(&artifact).expect("remove"));
        assert!(!artifact.path.exists());

        std::fs::write(&artifact.path, "user owned").expect("unmanaged fixture");
        assert!(matches!(
            remove(&artifact),
            Err(SkillError::UnmanagedConflict(_))
        ));
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
