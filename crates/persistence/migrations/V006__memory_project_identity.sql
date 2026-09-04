-- FR-064: a project is identified in the kernel by its repository directory's basename, because
-- that is what an agent derives from its working directory on its own. The cost of that choice is
-- that renaming or moving the directory silently re-points the project: new memory lands under the
-- new name and the old memory becomes unreachable from the panel, looking like data loss.
--
-- Recording the name and path we last used lets the app notice and say so, instead of showing an
-- empty panel and leaving the user to guess.
CREATE TABLE memory_project_identity (
    project_id     TEXT PRIMARY KEY,
    kernel_project TEXT NOT NULL,
    repo_path      TEXT NOT NULL,
    recorded_at    TEXT NOT NULL
);
