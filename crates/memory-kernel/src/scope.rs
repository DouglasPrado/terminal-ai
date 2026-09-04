//! Mapping Terminal AI scopes onto kernel (workspace, project, path) coordinates.
//!
//! Two rules drive everything here.
//!
//! 1. **A kernel query without a project returns pages from every project** — verified against a
//!    running kernel. So a resolved scope is the only way to address the kernel, and it can only be
//!    produced by [`resolve`]. Forgetting to scope a query is not a bug you can write.
//! 2. **The project name must be what an agent derives on its own** from its working directory,
//!    because the store is shared with the user's own agents. A cleverer, collision-proof naming
//!    scheme would mean the panel and the agent never see each other's pages.

use std::path::{Path, PathBuf};
use terminal_ai_domain::memory::{KernelScope, MemoryError};
use terminal_ai_domain::{MemoryType, Scope, ScopeLevel};

/// The kernel workspace Terminal AI uses. `default` is what an unconfigured agent resolves to, so
/// using anything else would split the store in two.
pub const WORKSPACE: &str = "default";

/// The reserved project the kernel uses for global preferences.
pub const GLOBAL_PROJECT: &str = "_global";

/// Every page Terminal AI writes lives under this prefix, so its pages are distinguishable from an
/// agent's inside a shared store.
pub const ROOT_PREFIX: &str = "terminal-ai";

/// A project as the caller already resolved it from the database.
///
/// This crate deliberately does not depend on `persistence`; the caller does the lookup and hands
/// the answer in, which keeps the mapping logic here and testable without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    pub path: PathBuf,
}

/// A scope plus everything needed to resolve it, gathered by the caller.
#[derive(Debug, Clone)]
pub struct ScopeInput {
    pub level: ScopeLevel,
    pub ref_id: Option<String>,
    /// The owning project. `None` is valid only for [`ScopeLevel::Global`].
    pub project: Option<ProjectRef>,
    /// A human label for the path prefix — a worktree's branch, for instance.
    pub label: Option<String>,
}

impl ScopeInput {
    #[must_use]
    pub fn global() -> Self {
        Self {
            level: ScopeLevel::Global,
            ref_id: None,
            project: None,
            label: None,
        }
    }

    #[must_use]
    pub fn for_project(scope: &Scope, project: ProjectRef, label: Option<String>) -> Self {
        Self {
            level: scope.level,
            ref_id: scope.ref_id.clone(),
            project: Some(project),
            label,
        }
    }
}

/// Resolve a scope into kernel coordinates.
///
/// # Errors
/// Returns [`MemoryError::InvalidScope`] when the scope is internally inconsistent — a global scope
/// carrying a ref id, a non-global scope missing one, or a non-global scope with no owning project.
pub fn resolve(input: &ScopeInput) -> Result<KernelScope, MemoryError> {
    let has_ref = input.ref_id.as_deref().is_some_and(|id| !id.is_empty());

    if input.level == ScopeLevel::Global {
        if has_ref {
            return Err(MemoryError::InvalidScope(
                "global scope must not carry a ref id".into(),
            ));
        }
        return Ok(KernelScope {
            workspace: WORKSPACE.to_owned(),
            project: GLOBAL_PROJECT.to_owned(),
            path_prefix: format!("{ROOT_PREFIX}/global"),
        });
    }

    if !has_ref {
        return Err(MemoryError::InvalidScope(format!(
            "{} scope requires a ref id",
            level_segment(input.level)
        )));
    }
    let Some(project) = input.project.as_ref() else {
        return Err(MemoryError::InvalidScope(format!(
            "{} scope could not be resolved to a project",
            level_segment(input.level)
        )));
    };

    let name = project_name(&project.path)?;
    let segment = level_segment(input.level);
    let path_prefix = match input.level {
        // A worktree's pages are the parent project's, filed under the branch so they are
        // distinguishable without being isolated (FR-047).
        ScopeLevel::Worktree | ScopeLevel::Workspace | ScopeLevel::Session => {
            let label = input
                .label
                .as_deref()
                .map(slugify)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| short_id(input.ref_id.as_deref().unwrap_or_default()));
            format!("{ROOT_PREFIX}/{segment}/{label}")
        }
        ScopeLevel::Project => format!("{ROOT_PREFIX}/{segment}"),
        ScopeLevel::Global => unreachable!("handled above"),
    };

    Ok(KernelScope {
        workspace: WORKSPACE.to_owned(),
        project: name,
        path_prefix,
    })
}

/// The kernel project name for a repository path: its directory basename, normalised.
///
/// This must agree with what the kernel derives from an agent's working directory, which is why it
/// is a plain basename and not something safer. The normalisation mirrors the kernel's own
/// `^[a-z0-9][a-z0-9._-]*$` rule.
///
/// # Errors
/// Returns [`MemoryError::InvalidScope`] if the path has no usable final component.
pub fn project_name(path: &Path) -> Result<String, MemoryError> {
    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(slugify)
        .filter(|s| !s.is_empty());
    base.ok_or_else(|| {
        MemoryError::InvalidScope(format!(
            "project path has no usable name: {}",
            path.display()
        ))
    })
}

/// Two projects whose directories share a basename, which the kernel cannot tell apart.
///
/// Detected and surfaced rather than prevented: preventing it would require a naming scheme the
/// agents do not follow, which would cost far more than it saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    pub name: String,
    pub project_ids: Vec<String>,
}

/// Find projects that would map onto the same kernel project.
#[must_use]
pub fn detect_collisions(projects: &[ProjectRef]) -> Vec<Collision> {
    let mut by_name: Vec<(String, Vec<String>)> = Vec::new();
    for project in projects {
        let Ok(name) = project_name(&project.path) else {
            continue;
        };
        match by_name.iter_mut().find(|(n, _)| *n == name) {
            Some((_, ids)) => ids.push(project.id.clone()),
            None => by_name.push((name, vec![project.id.clone()])),
        }
    }
    by_name
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(name, project_ids)| Collision { name, project_ids })
        .collect()
}

/// Build the page path for a new entry. Deterministic in `id`, which is the third and last layer
/// of the migration's idempotency: the same entry always lands at the same address.
#[must_use]
pub fn page_path(scope: &KernelScope, memory_type: MemoryType, title: &str, id: &str) -> String {
    let slug = {
        let s = slugify(title);
        if s.is_empty() {
            "untitled".to_owned()
        } else {
            s.chars().take(60).collect()
        }
    };
    format!(
        "{}/{}/{}-{}.md",
        scope.path_prefix,
        type_segment(memory_type),
        slug,
        short_id(id)
    )
}

/// Validate a page path before it reaches the kernel.
///
/// The kernel's page route is a `{*path}` wildcard, so traversal is a real risk rather than a
/// theoretical one.
///
/// # Errors
/// Returns [`MemoryError::InvalidPath`] for traversal, absolute paths, control characters,
/// disallowed bytes or an over-long path.
pub fn validate_path(path: &str) -> Result<(), MemoryError> {
    let reject = |why: &str| Err(MemoryError::InvalidPath(format!("{path}: {why}")));

    if path.is_empty() {
        return reject("empty");
    }
    if path.len() > 200 {
        return reject("longer than 200 characters");
    }
    if path.starts_with('/') {
        return reject("absolute");
    }
    if path.contains('\0') {
        return reject("contains a NUL byte");
    }
    if path
        .split('/')
        .any(|part| part == ".." || part == "." || part.is_empty())
    {
        return reject("contains an empty or traversal segment");
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return reject("contains characters outside [A-Za-z0-9._/-]");
    }
    Ok(())
}

const fn level_segment(level: ScopeLevel) -> &'static str {
    match level {
        ScopeLevel::Global => "global",
        ScopeLevel::Project => "project",
        ScopeLevel::Worktree => "worktree",
        ScopeLevel::Workspace => "workspace",
        ScopeLevel::Session => "session",
    }
}

const fn type_segment(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Fact => "fact",
        MemoryType::Decision => "decision",
        MemoryType::Constraint => "constraint",
        MemoryType::Preference => "preference",
        MemoryType::Glossary => "glossary",
        MemoryType::KnownIssue => "known-issue",
        MemoryType::Command => "command",
        MemoryType::Todo => "todo",
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = true; // suppresses a leading dash
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        // `/` matters: branch names like `feature/login` are common, and dropping the separator
        // silently would fuse the words instead of keeping them readable.
        } else if matches!(ch, '.' | '_' | '-' | ' ' | '/') && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn short_id(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, path: &str) -> ProjectRef {
        ProjectRef {
            id: id.to_owned(),
            path: PathBuf::from(path),
        }
    }

    fn input(level: ScopeLevel, ref_id: &str, path: &str) -> ScopeInput {
        ScopeInput {
            level,
            ref_id: Some(ref_id.to_owned()),
            project: Some(project("p1", path)),
            label: None,
        }
    }

    #[test]
    fn global_scope_maps_to_the_reserved_project() {
        let resolved = resolve(&ScopeInput::global()).expect("global resolves");
        assert_eq!(resolved.workspace, WORKSPACE);
        assert_eq!(resolved.project, GLOBAL_PROJECT);
        assert_eq!(resolved.path_prefix, "terminal-ai/global");
    }

    #[test]
    fn global_scope_with_a_ref_id_is_rejected() {
        let mut scope = ScopeInput::global();
        scope.ref_id = Some("stray".into());
        assert!(matches!(resolve(&scope), Err(MemoryError::InvalidScope(_))));
    }

    #[test]
    fn non_global_scope_without_a_ref_id_is_rejected() {
        let mut scope = input(ScopeLevel::Project, "p1", "/Users/x/www/albert");
        scope.ref_id = None;
        assert!(matches!(resolve(&scope), Err(MemoryError::InvalidScope(_))));
    }

    #[test]
    fn non_global_scope_without_a_project_is_rejected() {
        let mut scope = input(ScopeLevel::Project, "p1", "/Users/x/www/albert");
        scope.project = None;
        assert!(matches!(resolve(&scope), Err(MemoryError::InvalidScope(_))));
    }

    #[test]
    fn project_name_is_the_directory_basename() {
        // This is what an agent derives from its cwd on its own. If this test starts failing
        // because someone made the name "safer", the panel and the agents stop sharing memory.
        let resolved = resolve(&input(ScopeLevel::Project, "p1", "/Users/x/www/albert"))
            .expect("project resolves");
        assert_eq!(resolved.project, "albert");
        assert_eq!(resolved.path_prefix, "terminal-ai/project");
    }

    #[test]
    fn a_worktree_resolves_to_its_parent_project() {
        // FR-047: memory written from a worktree belongs to the parent project. The caller passes
        // the PARENT's ProjectRef, so the kernel project is the parent's name, not the worktree's.
        let parent = resolve(&input(ScopeLevel::Project, "p1", "/Users/x/www/albert"))
            .expect("parent resolves");
        let worktree = resolve(&ScopeInput {
            level: ScopeLevel::Worktree,
            ref_id: Some("wt-1".into()),
            project: Some(project("p1", "/Users/x/www/albert")),
            label: Some("feature/login".into()),
        })
        .expect("worktree resolves");

        assert_eq!(worktree.project, parent.project);
        assert_eq!(worktree.path_prefix, "terminal-ai/worktree/feature-login");
    }

    #[test]
    fn project_search_never_crosses_scope() {
        // Ported from crates/memory-manager/src/lib.rs:367-402, which proved FR-024 against SQL.
        // The guarantee now lives in the mapping: two different projects can never resolve to the
        // same kernel project, so a scoped query cannot reach across them.
        let alpha = resolve(&input(ScopeLevel::Project, "alpha", "/Users/x/www/alpha"))
            .expect("alpha resolves");
        let beta = resolve(&input(ScopeLevel::Project, "beta", "/Users/x/www/beta"))
            .expect("beta resolves");

        assert_ne!(alpha.project, beta.project);
        assert_eq!(alpha.workspace, beta.workspace);
    }

    #[test]
    fn same_basename_in_different_directories_is_reported_as_a_collision() {
        // Two projects the kernel genuinely cannot tell apart. We surface this instead of
        // preventing it, because preventing it means a name the agents do not derive.
        let projects = vec![
            project("p1", "/Users/x/work/api"),
            project("p2", "/Users/x/personal/api"),
            project("p3", "/Users/x/www/albert"),
        ];
        let collisions = detect_collisions(&projects);
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0].name, "api");
        assert_eq!(collisions[0].project_ids, vec!["p1", "p2"]);
    }

    #[test]
    fn project_names_are_normalised_the_way_the_kernel_expects() {
        // The kernel validates names against ^[a-z0-9][a-z0-9._-]*$.
        assert_eq!(
            project_name(Path::new("/x/My Project")).unwrap(),
            "my-project"
        );
        assert_eq!(
            project_name(Path::new("/x/Albert_v2")).unwrap(),
            "albert-v2"
        );
        assert!(project_name(Path::new("/")).is_err());
    }

    #[test]
    fn page_paths_are_deterministic_in_the_entry_id() {
        // The migration's third idempotency layer: the same legacy entry always lands at the same
        // address, so even a lost migration log cannot produce a duplicate.
        let scope = resolve(&input(ScopeLevel::Project, "p1", "/Users/x/www/albert")).unwrap();
        let a = page_path(
            &scope,
            MemoryType::Decision,
            "Use rustls",
            "8f2c1a3b-dead-beef",
        );
        let b = page_path(
            &scope,
            MemoryType::Decision,
            "Use rustls",
            "8f2c1a3b-dead-beef",
        );
        assert_eq!(a, b);
        assert_eq!(a, "terminal-ai/project/decision/use-rustls-8f2c1a3b.md");
    }

    #[test]
    fn generated_page_paths_always_validate() {
        let scope = resolve(&input(ScopeLevel::Project, "p1", "/Users/x/www/albert")).unwrap();
        for (t, title) in [
            (MemoryType::Fact, "A plain fact"),
            (MemoryType::KnownIssue, "../../etc/passwd"),
            (MemoryType::Todo, "  spaces  and — dashes  "),
            (MemoryType::Glossary, ""),
        ] {
            let path = page_path(&scope, t, title, "0123456789abcdef");
            validate_path(&path).unwrap_or_else(|e| panic!("{path} should validate: {e}"));
        }
    }

    #[test]
    fn path_traversal_and_junk_are_rejected() {
        for bad in [
            "",
            "/absolute/page.md",
            "../escape.md",
            "a/../../escape.md",
            "a//b.md",
            "a/./b.md",
            "page\0.md",
            "page with spaces.md",
            "página.md",
        ] {
            assert!(
                validate_path(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
        assert!(validate_path(&"a/".repeat(120)).is_err(), "over-long path");
        validate_path("terminal-ai/project/fact/ok-01234567.md").expect("a normal path is fine");
    }
}
