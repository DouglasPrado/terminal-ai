//! Safe git worktree lifecycle built on libgit2.
#![forbid(unsafe_code)]

use git2::{BranchType, Repository, WorktreeAddOptions, WorktreePruneOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub id: String,
    pub branch: String,
    pub path: PathBuf,
    pub dirty: bool,
}

pub fn create(
    project_path: &Path,
    branch: &str,
    create_branch: bool,
) -> Result<WorktreeSummary, WorktreeError> {
    validate_branch(branch)?;
    let repo = Repository::open(project_path)?;
    let name = worktree_name(branch);
    if repo
        .worktrees()?
        .iter()
        .flatten()
        .any(|entry| entry == name)
    {
        return Err(WorktreeError::AlreadyExists(branch.into()));
    }
    let reference = if create_branch {
        let head = repo.head()?.peel_to_commit()?;
        repo.branch(branch, &head, false)?.into_reference()
    } else {
        repo.find_branch(branch, BranchType::Local)?
            .into_reference()
    };
    if reference.is_branch()
        && repo
            .head()
            .ok()
            .and_then(|head| head.name().map(str::to_owned))
            == reference.name().map(str::to_owned)
    {
        return Err(WorktreeError::BranchInUse(branch.into()));
    }
    let root = worktree_root(project_path)?;
    std::fs::create_dir_all(&root)?;
    let path = root.join(&name);
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree(&name, &path, Some(&options))?;
    inspect(&name, branch, path)
}

pub fn list(project_path: &Path) -> Result<Vec<WorktreeSummary>, WorktreeError> {
    let repo = Repository::open(project_path)?;
    let mut result = Vec::new();
    for name in repo.worktrees()?.iter().flatten() {
        let worktree = repo.find_worktree(name)?;
        let path = worktree.path().to_path_buf();
        let branch = Repository::open(&path)
            .ok()
            .and_then(|repo| repo.head().ok()?.shorthand().map(str::to_owned))
            .unwrap_or_else(|| name.to_owned());
        result.push(inspect(name, &branch, path)?);
    }
    result.sort_by(|left, right| left.branch.cmp(&right.branch));
    Ok(result)
}

pub fn remove(project_path: &Path, id: &str) -> Result<(), WorktreeError> {
    let repo = Repository::open(project_path)?;
    let worktree = repo.find_worktree(id)?;
    let path = worktree.path().to_path_buf();
    let root = std::fs::canonicalize(worktree_root(project_path)?)?;
    let canonical_path = std::fs::canonicalize(&path)?;
    if !canonical_path.starts_with(&root) {
        return Err(WorktreeError::UnsafePath(path));
    }
    if path.exists() && is_dirty(&path)? {
        return Err(WorktreeError::Dirty(id.into()));
    }
    if path.exists() {
        std::fs::remove_dir_all(&path)?;
    }
    let mut options = WorktreePruneOptions::new();
    options.valid(true).working_tree(true);
    worktree.prune(Some(&mut options))?;
    Ok(())
}

fn inspect(id: &str, branch: &str, path: PathBuf) -> Result<WorktreeSummary, WorktreeError> {
    let dirty = path.exists() && is_dirty(&path)?;
    Ok(WorktreeSummary {
        id: id.into(),
        branch: branch.into(),
        path,
        dirty,
    })
}

fn is_dirty(path: &Path) -> Result<bool, WorktreeError> {
    let repo = Repository::open(path)?;
    let statuses = repo.statuses(None)?;
    let dirty = statuses.iter().any(|entry| {
        let status = entry.status();
        !status.is_ignored() && status != git2::Status::CURRENT
    });
    Ok(dirty)
}

fn worktree_root(project_path: &Path) -> Result<PathBuf, WorktreeError> {
    let project = project_path
        .file_name()
        .ok_or_else(|| WorktreeError::UnsafePath(project_path.into()))?;
    let parent = project_path
        .parent()
        .ok_or_else(|| WorktreeError::UnsafePath(project_path.into()))?;
    Ok(parent.join(".terminal-ai-worktrees").join(project))
}

fn worktree_name(branch: &str) -> String {
    branch
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn validate_branch(branch: &str) -> Result<(), WorktreeError> {
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.contains("..")
        || branch.chars().any(char::is_whitespace)
    {
        return Err(WorktreeError::InvalidBranch(branch.into()));
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("invalid branch name: {0}")]
    InvalidBranch(String),
    #[error("branch is already checked out: {0}")]
    BranchInUse(String),
    #[error("worktree already exists for branch: {0}")]
    AlreadyExists(String),
    #[error("worktree contains uncommitted changes: {0}")]
    Dirty(String),
    #[error("refusing unsafe worktree path: {0}")]
    UnsafePath(PathBuf),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_lists_and_removes_an_isolated_worktree() {
        let root =
            std::env::temp_dir().join(format!("terminal-ai-worktree-{}", std::process::id()));
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("project directory");
        let repo = Repository::init(&project).expect("repository");
        std::fs::write(project.join("README.md"), "main\n").expect("fixture");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("README.md")).expect("add");
        let tree_id = index.write_tree().expect("tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = git2::Signature::now("Terminal AI", "test@terminal.ai").expect("signature");
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .expect("commit");
        drop(tree);
        drop(repo);

        let created = create(&project, "feature/test", true).expect("create worktree");
        assert!(created.path.join("README.md").is_file());
        assert_eq!(list(&project).expect("list").len(), 1);
        remove(&project, &created.id).expect("remove");
        assert!(list(&project).expect("list after remove").is_empty());
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    /// Why the memory kernel never writes configuration into a worktree.
    ///
    /// Feature 002 wanted to pin each worktree's memory project with a file in its working tree.
    /// This is what stops that: `is_dirty` counts *any* non-ignored status entry, and an untracked
    /// file is one — so the worktree becomes undeletable until the user finds and removes a file
    /// they never created. Capture is therefore configured at project scope, and worktrees are
    /// folded into the parent project by the kernel's own `--project-strategy repo-root`.
    ///
    /// If this test ever starts failing because `is_dirty` learned to ignore untracked files, that
    /// design constraint is gone and the simpler approach becomes available again.
    #[test]
    fn an_untracked_file_makes_a_worktree_undeletable() {
        let root = std::env::temp_dir().join(format!(
            "terminal-ai-worktree-dirty-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("project directory");
        let repo = Repository::init(&project).expect("repository");
        std::fs::write(project.join("README.md"), "main\n").expect("fixture");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("README.md")).expect("add");
        let tree_id = index.write_tree().expect("tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = git2::Signature::now("Terminal AI", "test@terminal.ai").expect("signature");
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .expect("commit");
        drop(tree);
        drop(repo);

        let created = create(&project, "feature/dirty", true).expect("create worktree");
        // Exactly what pinning the memory project inside the tree would have written.
        std::fs::write(created.path.join(".ai-memory.toml"), "project = \"x\"\n")
            .expect("write marker");

        let error =
            remove(&project, &created.id).expect_err("a dirty worktree must not be removed");
        assert!(
            matches!(error, WorktreeError::Dirty(_)),
            "expected Dirty, got {error:?}"
        );

        std::fs::remove_file(created.path.join(".ai-memory.toml")).expect("undo the marker");
        remove(&project, &created.id).expect("removable once the stray file is gone");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
