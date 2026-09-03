//! One-shot import of the legacy memory store into the kernel.
//!
//! Three properties matter more than speed here, because this touches memory a user has already
//! accumulated:
//!
//! 1. **Idempotent.** Running it twice creates nothing new. Three independent layers guarantee it:
//!    the log keyed by legacy id, a body hash, and a page path derived deterministically from that
//!    id — so even a restored old `app.db` with an empty log cannot produce duplicates.
//! 2. **Resumable.** Each item is recorded the moment it lands, never batched at the end, so an
//!    interruption costs exactly the item in flight.
//! 3. **Reversible.** Every imported page is addressable from the log, so the whole thing can be
//!    undone. The legacy rows and markdown files are never touched.
//!
//! Planning is pure and takes entries the caller already loaded, which keeps this crate free of
//! `persistence` and makes the interesting cases testable without a database or a network.

use crate::kernel::{render_imported_document, ScopeDirectory};
use crate::scope::{page_path, resolve};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use terminal_ai_domain::memory::{KernelScope, MemoryError};
use terminal_ai_domain::{MemoryType, Scope, ScopeLevel};

/// A memory entry as it exists in the legacy tables, with its body already read from disk.
#[derive(Debug, Clone)]
pub struct LegacyEntry {
    pub id: String,
    pub scope: Scope,
    pub memory_type: MemoryType,
    pub title: String,
    pub body: String,
}

/// One entry that will be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedImport {
    pub entry_id: String,
    pub scope: KernelScope,
    pub page_path: String,
    pub title: String,
    pub kind: String,
    pub document: String,
    pub body_sha256: String,
}

/// An entry that will not be written, and why — surfaced to the user rather than swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedImport {
    pub entry_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationPlan {
    pub to_import: Vec<PlannedImport>,
    pub already_imported: Vec<String>,
    pub skipped: Vec<SkippedImport>,
}

impl MigrationPlan {
    #[must_use]
    pub fn total(&self) -> usize {
        self.to_import.len() + self.already_imported.len() + self.skipped.len()
    }
}

/// What the log already holds: legacy entry id → the body hash recorded for it.
pub type ImportedIndex = HashMap<String, String>;

/// Decide what to do with every legacy entry. Pure: no IO, no clock.
///
/// Entries are ordered global → project → worktree → workspace → session so that a partial run
/// leaves the broadest, most reusable scope complete rather than a random half.
#[must_use]
pub fn plan(
    entries: &[LegacyEntry],
    imported: &ImportedIndex,
    directory: &dyn ScopeDirectory,
) -> MigrationPlan {
    let mut ordered: Vec<&LegacyEntry> = entries.iter().collect();
    ordered.sort_by_key(|entry| scope_order(entry.scope.level));

    let mut plan = MigrationPlan::default();
    for entry in ordered {
        let hash = body_hash(&entry.body);

        if imported.get(&entry.id).is_some_and(|seen| *seen == hash) {
            plan.already_imported.push(entry.id.clone());
            continue;
        }
        if entry.body.trim().is_empty() {
            plan.skipped.push(SkippedImport {
                entry_id: entry.id.clone(),
                reason: "the entry has no body".into(),
            });
            continue;
        }

        let kernel_scope = match directory
            .resolve(&entry.scope)
            .and_then(|input| resolve(&input))
        {
            Ok(scope) => scope,
            Err(err) => {
                // Almost always a project that no longer exists. Reporting it beats guessing a
                // destination and filing someone's memory under the wrong project.
                plan.skipped.push(SkippedImport {
                    entry_id: entry.id.clone(),
                    reason: err.to_string(),
                });
                continue;
            }
        };

        // Deterministic in the legacy id: the third idempotency layer, and the one that still
        // holds when the log itself is gone.
        let path = page_path(&kernel_scope, entry.memory_type, &entry.title, &entry.id);
        plan.to_import.push(PlannedImport {
            entry_id: entry.id.clone(),
            document: render_imported_document(
                &entry.scope,
                entry.memory_type,
                &entry.title,
                &entry.body,
                &entry.id,
            ),
            page_path: path,
            title: entry.title.clone(),
            kind: kind_name(entry.memory_type).to_owned(),
            scope: kernel_scope,
            body_sha256: hash,
        });
    }
    plan
}

/// Writes pages into the kernel. Implemented for real by the CLI, and by a fake in tests.
#[async_trait]
pub trait PageWriter: Send + Sync {
    async fn write_page(
        &self,
        scope: &KernelScope,
        path: &str,
        title: &str,
        kind: &str,
        document: &str,
    ) -> Result<(), MemoryError>;

    async fn delete_page(&self, scope: &KernelScope, path: &str) -> Result<(), MemoryError>;
}

/// Records what landed. Implemented by the composition root over the migration log.
#[async_trait]
pub trait MigrationRecorder: Send + Sync {
    async fn record(&self, item: &PlannedImport) -> Result<(), MemoryError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub total: usize,
    pub already_imported: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedImport>,
    pub failed: Vec<SkippedImport>,
}

/// Execute a plan.
///
/// Sequential on purpose. The kernel serialises writes through a single writer actor anyway, and a
/// predictable order makes a partial run comprehensible: everything before the failure landed,
/// everything after did not.
pub async fn run(
    plan: MigrationPlan,
    writer: &dyn PageWriter,
    recorder: &dyn MigrationRecorder,
) -> MigrationReport {
    let mut report = MigrationReport {
        total: plan.total(),
        already_imported: plan.already_imported.len(),
        skipped: plan.skipped,
        ..MigrationReport::default()
    };

    for item in plan.to_import {
        match writer
            .write_page(
                &item.scope,
                &item.page_path,
                &item.title,
                &item.kind,
                &item.document,
            )
            .await
        {
            Ok(()) => {
                // Recorded immediately. Batching this at the end would mean a crash re-imports
                // everything, which is exactly the duplicate the whole design avoids.
                if let Err(err) = recorder.record(&item).await {
                    report.failed.push(SkippedImport {
                        entry_id: item.entry_id,
                        reason: format!("written, but not recorded: {err}"),
                    });
                } else {
                    report.imported += 1;
                }
            }
            Err(err) => report.failed.push(SkippedImport {
                entry_id: item.entry_id,
                reason: err.to_string(),
            }),
        }
    }
    report
}

/// Remove everything a previous import wrote. The legacy data is untouched by construction: this
/// only ever addresses pages the log names.
pub async fn undo(
    pages: &[(KernelScope, String)],
    writer: &dyn PageWriter,
) -> Result<Vec<String>, MemoryError> {
    let mut removed = Vec::new();
    for (scope, path) in pages {
        writer.delete_page(scope, path).await?;
        removed.push(path.clone());
    }
    Ok(removed)
}

#[must_use]
pub fn body_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

const fn scope_order(level: ScopeLevel) -> u8 {
    match level {
        ScopeLevel::Global => 0,
        ScopeLevel::Project => 1,
        ScopeLevel::Worktree => 2,
        ScopeLevel::Workspace => 3,
        ScopeLevel::Session => 4,
    }
}

const fn kind_name(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Fact => "fact",
        MemoryType::Decision => "decision",
        MemoryType::Constraint => "constraint",
        MemoryType::Preference => "preference",
        MemoryType::Glossary => "glossary",
        MemoryType::KnownIssue => "known_issue",
        MemoryType::Command => "command",
        MemoryType::Todo => "todo",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::{ProjectRef, ScopeInput};
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct Directory;
    impl ScopeDirectory for Directory {
        fn resolve(&self, scope: &Scope) -> Result<ScopeInput, MemoryError> {
            match scope.level {
                ScopeLevel::Global => Ok(ScopeInput::global()),
                _ => {
                    let ref_id = scope.ref_id.as_deref().unwrap_or_default();
                    if ref_id == "gone" {
                        return Err(MemoryError::InvalidScope(
                            "this scope no longer points at a project".into(),
                        ));
                    }
                    Ok(ScopeInput::for_project(
                        scope,
                        ProjectRef {
                            id: ref_id.to_owned(),
                            path: PathBuf::from(format!("/Users/x/www/{ref_id}")),
                        },
                        None,
                    ))
                }
            }
        }
    }

    /// Records writes instead of performing them, and can be told to fail at a given index —
    /// which is how the resume case is exercised without killing a real process mid-run.
    #[derive(Default)]
    struct FakeKernel {
        written: Mutex<Vec<String>>,
        deleted: Mutex<Vec<String>>,
        fail_after: Option<usize>,
    }

    #[async_trait]
    impl PageWriter for FakeKernel {
        async fn write_page(
            &self,
            _scope: &KernelScope,
            path: &str,
            _title: &str,
            _kind: &str,
            _document: &str,
        ) -> Result<(), MemoryError> {
            let mut written = self.written.lock().expect("lock");
            if self.fail_after.is_some_and(|limit| written.len() >= limit) {
                return Err(MemoryError::Transport("kernel went away".into()));
            }
            written.push(path.to_owned());
            Ok(())
        }

        async fn delete_page(&self, _scope: &KernelScope, path: &str) -> Result<(), MemoryError> {
            self.deleted.lock().expect("lock").push(path.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeRecorder {
        recorded: Mutex<ImportedIndex>,
    }

    #[async_trait]
    impl MigrationRecorder for FakeRecorder {
        async fn record(&self, item: &PlannedImport) -> Result<(), MemoryError> {
            self.recorded
                .lock()
                .expect("lock")
                .insert(item.entry_id.clone(), item.body_sha256.clone());
            Ok(())
        }
    }

    fn entry(id: &str, level: ScopeLevel, ref_id: Option<&str>, body: &str) -> LegacyEntry {
        LegacyEntry {
            id: id.to_owned(),
            scope: Scope {
                level,
                ref_id: ref_id.map(str::to_owned),
            },
            memory_type: MemoryType::Fact,
            title: format!("Entry {id}"),
            body: body.to_owned(),
        }
    }

    fn corpus() -> Vec<LegacyEntry> {
        vec![
            entry("s1", ScopeLevel::Session, Some("sess"), "session note"),
            entry("p1", ScopeLevel::Project, Some("albert"), "project note"),
            entry("g1", ScopeLevel::Global, None, "global note"),
        ]
    }

    #[tokio::test]
    async fn a_second_run_imports_nothing() {
        // SC-016. The single most important property: a user who clicks import twice must not end
        // up with two copies of their memory.
        let writer = FakeKernel::default();
        let recorder = FakeRecorder::default();
        let entries = corpus();

        let first = run(
            plan(&entries, &ImportedIndex::new(), &Directory),
            &writer,
            &recorder,
        )
        .await;
        assert_eq!(first.imported, 3);

        let seen = recorder.recorded.lock().expect("lock").clone();
        let second = run(plan(&entries, &seen, &Directory), &writer, &recorder).await;

        assert_eq!(second.imported, 0);
        assert_eq!(second.already_imported, 3);
        assert_eq!(writer.written.lock().expect("lock").len(), 3);
    }

    #[tokio::test]
    async fn an_interrupted_run_resumes_where_it_stopped() {
        // FR-053. Recording per item is what makes this true; batching at the end would re-import
        // everything after a crash.
        let failing = FakeKernel {
            fail_after: Some(2),
            ..FakeKernel::default()
        };
        let recorder = FakeRecorder::default();
        let entries = corpus();

        let first = run(
            plan(&entries, &ImportedIndex::new(), &Directory),
            &failing,
            &recorder,
        )
        .await;
        assert_eq!(first.imported, 2);
        assert_eq!(first.failed.len(), 1);

        let seen = recorder.recorded.lock().expect("lock").clone();
        let writer = FakeKernel::default();
        let second = run(plan(&entries, &seen, &Directory), &writer, &recorder).await;

        assert_eq!(second.imported, 1, "only the remainder");
        assert_eq!(second.already_imported, 2);
    }

    #[test]
    fn the_page_path_is_stable_even_with_the_log_lost() {
        // The third idempotency layer. Restoring an older app.db empties the log; the same entry
        // must still map to the same address so the write upserts instead of creating a twin.
        let entries = corpus();
        let first = plan(&entries, &ImportedIndex::new(), &Directory);
        let second = plan(&entries, &ImportedIndex::new(), &Directory);

        let paths = |p: &MigrationPlan| {
            p.to_import
                .iter()
                .map(|i| i.page_path.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(paths(&first), paths(&second));
    }

    #[test]
    fn a_changed_body_is_re_imported() {
        let mut entries = corpus();
        let seen: ImportedIndex = entries
            .iter()
            .map(|e| (e.id.clone(), body_hash(&e.body)))
            .collect();
        assert_eq!(plan(&entries, &seen, &Directory).to_import.len(), 0);

        entries[0].body = "edited after the first import".into();
        let replanned = plan(&entries, &seen, &Directory);
        assert_eq!(replanned.to_import.len(), 1);
        assert_eq!(replanned.already_imported.len(), 2);
    }

    #[test]
    fn broad_scopes_are_imported_first() {
        // A partial run should leave global memory complete rather than an arbitrary half.
        let plan = plan(&corpus(), &ImportedIndex::new(), &Directory);
        let ids: Vec<&str> = plan.to_import.iter().map(|i| i.entry_id.as_str()).collect();
        assert_eq!(ids, vec!["g1", "p1", "s1"]);
    }

    #[test]
    fn unresolvable_and_empty_entries_are_reported_not_dropped() {
        let entries = vec![
            entry("orphan", ScopeLevel::Project, Some("gone"), "text"),
            entry("blank", ScopeLevel::Global, None, "   "),
            entry("ok", ScopeLevel::Global, None, "text"),
        ];
        let plan = plan(&entries, &ImportedIndex::new(), &Directory);

        assert_eq!(plan.to_import.len(), 1);
        assert_eq!(plan.skipped.len(), 2);
        assert_eq!(plan.total(), 3, "nothing vanishes from the accounting");
        // The user needs to know *why*, not just that something was left behind.
        assert!(plan.skipped.iter().all(|s| !s.reason.is_empty()));
    }

    #[tokio::test]
    async fn imported_pages_carry_their_legacy_id() {
        // The frontmatter link back to the legacy entry is what makes an undo, or a later audit,
        // possible at all.
        let plan = plan(&corpus(), &ImportedIndex::new(), &Directory);
        assert!(plan.to_import.iter().all(|item| item
            .document
            .contains(&format!("terminal_ai_entry_id: {}", item.entry_id))));
    }

    #[tokio::test]
    async fn undo_removes_only_what_was_imported() {
        let writer = FakeKernel::default();
        let recorder = FakeRecorder::default();
        let planned = plan(&corpus(), &ImportedIndex::new(), &Directory);
        let pages: Vec<(KernelScope, String)> = planned
            .to_import
            .iter()
            .map(|i| (i.scope.clone(), i.page_path.clone()))
            .collect();
        run(planned, &writer, &recorder).await;

        let removed = undo(&pages, &writer).await.expect("undo succeeds");
        assert_eq!(removed.len(), 3);
        assert_eq!(*writer.deleted.lock().expect("lock"), removed);
    }
}
