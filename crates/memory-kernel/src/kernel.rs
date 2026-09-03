//! The production [`MemoryKernel`]: an ai-memory server, supervised or attached.
//!
//! Reads go over `/api/v1`, writes go through the binary, status comes from the supervisor's
//! cache. Every method refuses immediately when the kernel is not usable, so a missing or dead
//! kernel costs a command one comparison rather than a socket timeout (Constitution VI).

use crate::cli::KernelCli;
use crate::http::ReadClient;
use crate::runtime::Supervisor;
use crate::scope::{page_path, resolve, ScopeInput};
use async_trait::async_trait;
use std::sync::Arc;
use terminal_ai_domain::memory::{
    Handoff, HandoffState, KernelScope, KernelStatus, MemoryError, MemoryKernel, MemoryPage,
    MemorySource, PageAuthor,
};
use terminal_ai_domain::{MemoryType, Scope, ScopeLevel};

/// Resolves a [`Scope`] into everything the mapper needs.
///
/// Implemented by the composition root, which owns the database. That is what keeps this crate
/// free of `persistence` while the mapping rules — and the FR-046 isolation guarantee they encode —
/// stay here where they are unit-tested.
pub trait ScopeDirectory: Send + Sync {
    /// # Errors
    /// Fails when the scope cannot be resolved to a project that still exists.
    fn resolve(&self, scope: &Scope) -> Result<ScopeInput, MemoryError>;
}

/// A page as the kernel returned it, before it becomes a domain type. A struct rather than eight
/// positional arguments, because four of them are `Option<String>` and swapping two would compile.
struct RawPage<'a> {
    path: &'a str,
    title: &'a str,
    kind: Option<&'a str>,
    frontmatter: &'a serde_json::Value,
    body: String,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub struct AiMemoryKernel {
    supervisor: Arc<Supervisor>,
    directory: Arc<dyn ScopeDirectory>,
}

impl AiMemoryKernel {
    #[must_use]
    pub fn new(supervisor: Arc<Supervisor>, directory: Arc<dyn ScopeDirectory>) -> Self {
        Self {
            supervisor,
            directory,
        }
    }

    /// Refuse early when the kernel is not usable, and hand back the pieces needed to talk to it.
    fn ready(&self) -> Result<(Arc<KernelCli>, ReadClient), MemoryError> {
        let status = self.supervisor.status();
        if !status.state.is_usable() {
            return Err(MemoryError::Unavailable(status.state));
        }
        let cli = self
            .supervisor
            .cli()
            .ok_or(MemoryError::Unavailable(status.state))?;
        let config = cli.config();
        let read = ReadClient::new(&config.server_url, config.token.clone())?;
        Ok((cli, read))
    }

    fn kernel_scope(&self, scope: &Scope) -> Result<KernelScope, MemoryError> {
        resolve(&self.directory.resolve(scope)?)
    }

    /// Turn a kernel page into a domain page, tolerating pages an agent wrote.
    ///
    /// A page with no `terminal_ai_*` frontmatter is an agent's. Refusing to show it would blind
    /// the panel to exactly the content this feature exists to surface, so it degrades to a fact
    /// with a path-derived title and is marked as agent-authored.
    fn to_page(scope: &Scope, raw: RawPage<'_>) -> MemoryPage {
        let RawPage {
            path,
            title,
            kind,
            frontmatter,
            body,
            created_at,
            updated_at,
        } = raw;

        let ours = frontmatter.get("terminal_ai_type").is_some();
        let memory_type = frontmatter
            .get("terminal_ai_type")
            .and_then(serde_json::Value::as_str)
            .or(kind)
            .and_then(parse_memory_type)
            .unwrap_or(MemoryType::Fact);

        let title = if title.trim().is_empty() {
            path.rsplit('/')
                .next()
                .unwrap_or(path)
                .trim_end_matches(".md")
                .replace('-', " ")
        } else {
            title.to_owned()
        };

        MemoryPage {
            id: path.to_owned(),
            scope: scope.clone(),
            memory_type,
            title,
            body,
            author: if ours {
                PageAuthor::TerminalAi
            } else {
                PageAuthor::Agent
            },
            created_at: created_at.unwrap_or_default(),
            updated_at: updated_at.unwrap_or_default(),
        }
    }
}

#[async_trait]
impl MemoryKernel for AiMemoryKernel {
    async fn status(&self) -> KernelStatus {
        self.supervisor.status()
    }

    async fn list(&self, scope: &Scope, limit: usize) -> Result<Vec<MemoryPage>, MemoryError> {
        let (_, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let pages = read.list_pages(&kernel_scope).await?;
        Ok(pages
            .into_iter()
            .filter(|p| p.path.starts_with(&kernel_scope.path_prefix))
            .take(limit)
            .map(|p| {
                Self::to_page(
                    scope,
                    RawPage {
                        path: &p.path,
                        title: &p.title,
                        kind: p.kind.as_deref(),
                        frontmatter: &serde_json::Value::Null,
                        body: String::new(),
                        created_at: None,
                        updated_at: p.updated_at.clone(),
                    },
                )
            })
            .collect())
    }

    async fn search(
        &self,
        query: &str,
        scope: &Scope,
        limit: usize,
    ) -> Result<Vec<MemoryPage>, MemoryError> {
        let (_, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let hits = read.search(&kernel_scope, query, limit).await?;
        Ok(hits
            .into_iter()
            .map(|hit| {
                Self::to_page(
                    scope,
                    RawPage {
                        path: &hit.path,
                        title: &hit.title,
                        kind: hit.kind.as_deref(),
                        frontmatter: &serde_json::Value::Null,
                        // Snippets arrive with <mark> markup around the match. Kept verbatim here
                        // and sanitised at render time — the renderer is the one place that has to
                        // know this text is untrusted.
                        body: hit.snippet.clone(),
                        created_at: None,
                        updated_at: None,
                    },
                )
            })
            .collect())
    }

    async fn read(&self, scope: &Scope, path: &str) -> Result<MemoryPage, MemoryError> {
        let (_, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let page = read.read_page(&kernel_scope, path).await?;
        Ok(Self::to_page(
            scope,
            RawPage {
                path: &page.path,
                title: &page.title,
                kind: page.kind.as_deref(),
                frontmatter: &page.frontmatter,
                body: page.body_markdown.clone(),
                created_at: page.created_at.clone(),
                updated_at: page.updated_at.clone(),
            },
        ))
    }

    async fn write(
        &self,
        scope: &Scope,
        memory_type: MemoryType,
        title: &str,
        body: &str,
    ) -> Result<String, MemoryError> {
        let (cli, _) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let id = uuid_like(title, body);
        let path = page_path(&kernel_scope, memory_type, title, &id);
        let document = render_document(scope, memory_type, title, body, None);
        cli.write_page(
            &kernel_scope,
            &path,
            title,
            type_name(memory_type),
            &document,
        )
        .await
        .map(|outcome| outcome.path)
    }

    async fn update(
        &self,
        scope: &Scope,
        path: &str,
        title: Option<&str>,
        body: &str,
    ) -> Result<(), MemoryError> {
        let (cli, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let existing = read.read_page(&kernel_scope, path).await?;
        let title = title.unwrap_or(&existing.title);
        let memory_type = existing
            .frontmatter
            .get("terminal_ai_type")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_memory_type)
            .unwrap_or(MemoryType::Fact);
        let document = render_document(scope, memory_type, title, body, None);
        cli.write_page(
            &kernel_scope,
            path,
            title,
            type_name(memory_type),
            &document,
        )
        .await
        .map(|_| ())
    }

    async fn delete(&self, scope: &Scope, path: &str) -> Result<(), MemoryError> {
        let (cli, _) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        cli.delete_page(&kernel_scope, path).await
    }

    async fn compose_context(
        &self,
        scope: &Scope,
        max_bytes: usize,
    ) -> Result<(String, Vec<MemorySource>), MemoryError> {
        // The requested scope plus global, in precedence order — global first so the more specific
        // pages read as refinements of it.
        let mut scopes = vec![Scope {
            level: ScopeLevel::Global,
            ref_id: None,
        }];
        if scope.level != ScopeLevel::Global {
            scopes.push(scope.clone());
        }

        let mut composed = String::new();
        let mut sources = Vec::new();
        for current in scopes {
            let Ok(pages) = self.list(&current, 50).await else {
                continue;
            };
            for summary in pages {
                let Ok(page) = self.read(&current, &summary.id).await else {
                    continue;
                };
                let block = format!("## {}\n\n{}\n\n", page.title, page.body.trim());
                if composed.len() + block.len() > max_bytes {
                    return Ok((composed, sources));
                }
                composed.push_str(&block);
                sources.push(MemorySource {
                    entry_id: page.id,
                    scope: current.clone(),
                });
            }
        }
        Ok((composed, sources))
    }

    async fn briefing(&self, scope: &Scope) -> Result<String, MemoryError> {
        let (_, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let recent = read.recent(&kernel_scope, 10).await?;
        let mut out = format!("{} recent pages\n", recent.len());
        for page in recent {
            out.push_str(&format!("- {} ({})\n", page.title, page.path));
        }
        Ok(out)
    }

    async fn handoffs(
        &self,
        scope: &Scope,
        state: Option<HandoffState>,
    ) -> Result<Vec<Handoff>, MemoryError> {
        let (_, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let filter = state.map(|s| match s {
            HandoffState::Open => "open",
            HandoffState::Accepted => "accepted",
            HandoffState::Expired => "expired",
        });
        let records = read.handoffs(&kernel_scope, filter).await?;
        Ok(records
            .into_iter()
            .map(|record| Handoff {
                id: record.id,
                agent: record.agent,
                state: match record.state.as_str() {
                    "accepted" => HandoffState::Accepted,
                    "expired" => HandoffState::Expired,
                    _ => HandoffState::Open,
                },
                summary: record.summary,
                open_questions: record.open_questions,
                next_steps: record.next_steps,
                created_at: record.at.unwrap_or_default(),
                accepted_at: record.accepted_at,
            })
            .collect())
    }

    async fn expire_handoffs(
        &self,
        scope: &Scope,
        older_than_days: u32,
    ) -> Result<u32, MemoryError> {
        let (cli, read) = self.ready()?;
        let kernel_scope = self.kernel_scope(scope)?;
        let before = read.handoffs(&kernel_scope, Some("open")).await?.len();
        cli.expire_handoffs(&kernel_scope, older_than_days).await?;
        let after = read.handoffs(&kernel_scope, Some("open")).await?.len();
        Ok(u32::try_from(before.saturating_sub(after)).unwrap_or(0))
    }
}

/// The markdown document written to the kernel.
///
/// Every key is prefixed `terminal_ai_` so it can never collide with ai-memory's own frontmatter,
/// and the H1 is what the kernel derives its title from.
fn render_document(
    scope: &Scope,
    memory_type: MemoryType,
    title: &str,
    body: &str,
    legacy_entry_id: Option<&str>,
) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("terminal_ai_type: {}\n", type_name(memory_type)));
    out.push_str(&format!("terminal_ai_scope: {}\n", scope_name(scope.level)));
    if let Some(ref_id) = scope.ref_id.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("terminal_ai_ref_id: {ref_id}\n"));
    }
    if let Some(entry_id) = legacy_entry_id {
        out.push_str(&format!("terminal_ai_entry_id: {entry_id}\n"));
    }
    out.push_str(&format!(
        "terminal_ai_created_at: {}\n",
        chrono::Utc::now().to_rfc3339()
    ));
    out.push_str("---\n\n");
    out.push_str(&format!("# {}\n\n", title.trim()));
    out.push_str(body.trim());
    out.push('\n');
    out
}

/// Public so the migration can write imported pages with the same shape.
#[must_use]
pub fn render_imported_document(
    scope: &Scope,
    memory_type: MemoryType,
    title: &str,
    body: &str,
    legacy_entry_id: &str,
) -> String {
    render_document(scope, memory_type, title, body, Some(legacy_entry_id))
}

const fn type_name(memory_type: MemoryType) -> &'static str {
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

fn parse_memory_type(name: &str) -> Option<MemoryType> {
    Some(match name {
        "fact" => MemoryType::Fact,
        "decision" => MemoryType::Decision,
        "constraint" => MemoryType::Constraint,
        "preference" => MemoryType::Preference,
        "glossary" => MemoryType::Glossary,
        "known_issue" | "known-issue" => MemoryType::KnownIssue,
        "command" => MemoryType::Command,
        "todo" => MemoryType::Todo,
        _ => return None,
    })
}

const fn scope_name(level: ScopeLevel) -> &'static str {
    match level {
        ScopeLevel::Global => "global",
        ScopeLevel::Project => "project",
        ScopeLevel::Worktree => "worktree",
        ScopeLevel::Workspace => "workspace",
        ScopeLevel::Session => "session",
    }
}

/// A stable-enough id for a new page. Content-derived so an accidental double-submit of the same
/// entry lands on the same path instead of creating a twin.
fn uuid_like(title: &str, body: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"\0");
    hasher.update(body.as_bytes());
    hasher.update(chrono::Utc::now().date_naive().to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}
