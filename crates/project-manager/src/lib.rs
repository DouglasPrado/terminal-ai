//! Repository discovery and status.
#![forbid(unsafe_code)]

use git2::{Repository, RepositoryOpenFlags, StatusOptions};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
}
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub name: String,
    pub path: PathBuf,
    pub remote: Option<String>,
    pub status: GitStatus,
}

pub fn discover(roots: &[PathBuf]) -> Result<Vec<DiscoveredProject>, ProjectError> {
    let mut projects = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir()
                && Repository::open_ext(&path, RepositoryOpenFlags::NO_SEARCH, &[] as &[&Path])
                    .is_ok()
            {
                projects.push(inspect(&path)?);
            }
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}
pub fn inspect(path: &Path) -> Result<DiscoveredProject, ProjectError> {
    let repository = Repository::open(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository")
        .to_string();
    let remote = repository
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(str::to_owned));
    let status = status_for_repo(&repository)?;
    Ok(DiscoveredProject {
        name,
        path: path.canonicalize()?,
        remote,
        status,
    })
}
pub fn status(path: &Path) -> Result<GitStatus, ProjectError> {
    status_for_repo(&Repository::open(path)?)
}
fn status_for_repo(repository: &Repository) -> Result<GitStatus, ProjectError> {
    let head = repository.head()?;
    let branch = head.shorthand().unwrap_or("HEAD").to_string();
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let dirty = !repository.statuses(Some(&mut options))?.is_empty();
    let upstream = repository
        .find_branch(&branch, git2::BranchType::Local)
        .and_then(|branch| branch.upstream())
        .ok()
        .and_then(|branch| branch.get().target());
    let (ahead, behind) = match (head.target(), upstream) {
        (Some(local), Some(upstream)) => repository
            .graph_ahead_behind(local, upstream)
            .unwrap_or((0, 0)),
        _ => (0, 0),
    };
    Ok(GitStatus {
        branch,
        dirty,
        ahead,
        behind,
    })
}
pub fn clone_repository(url: &str, destination: &Path) -> Result<DiscoveredProject, ProjectError> {
    if destination.exists() {
        return Err(ProjectError::DestinationExists(destination.to_path_buf()));
    }
    Repository::clone(url, destination)?;
    inspect(destination)
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("clone destination already exists: {0}")]
    DestinationExists(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspects_this_repository() {
        let project = inspect(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .as_path(),
        )
        .expect("repository");
        assert!(!project.name.is_empty());
    }
}
