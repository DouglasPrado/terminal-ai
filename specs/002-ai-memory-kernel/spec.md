# Feature Specification: ai-memory as the Memory Kernel

**Feature Branch**: `002-ai-memory-kernel`

**Created**: 2026-09-03

**Status**: Draft

**Input**: User description: "sabe lá na memória? quero que faça um plano para essa memória ser
utilizado como kernel esse projeto https://github.com/akitaonrails/ai-memory"

## Clarifications

### Session 2026-09-03

- Q: How should ai-memory run alongside Terminal AI? → A: **Managed sidecar** — the app bundles or
  locates the `ai-memory` binary and supervises `ai-memory serve`; when a server is already
  listening at the configured URL the app merely attaches to it and never manages its lifecycle.
- Q: What happens to the existing local memory store (`memory_entries` + `memory_fts` + markdown)?
  → A: **Replace with migration** — a one-shot, idempotent import of existing entries into the
  ai-memory wiki. The legacy tables become read-only legacy and are not dropped.
- Q: Should Terminal AI wire agent terminals to ai-memory automatically? → A: **Yes, behind
  preview → diff → consent** — dry-run the wiring, show the diff of what would be written to the
  Claude/Codex/OpenCode config, apply only after confirmation, record what was created, remove only
  that.
- Q: New Spec Kit feature or amend 001? → A: **New feature `002-ai-memory-kernel`** with its own
  branch.

### Session 2026-09-03 (design decisions)

- Q: Should the app share the memory store the user runs (or would run) outside the app? → A:
  **Share it** — use ai-memory's own default data location, so what the panel writes is what the
  user's agents read, in the app or outside it. One brain is the premise of the feature.
- Q: How is a project named inside the kernel? → A: **The repository directory's basename**, which
  is what an agent derives from its working directory on its own — so panel and agent agree with no
  file written into the user's repository. Two projects with the same basename are detected and the
  user disambiguates once.
- Q: What about the ~87 MB embedding model the kernel fetches on first run? → A: **Off by default,
  opt-in** — the kernel starts without embeddings (full-text, entity and graph ranking already
  work); a settings toggle enables hybrid search and only then fetches the model, after saying so.

### Session 2026-09-03 (contract probe against ai-memory v2.0.2)

A real `ai-memory serve` was run before writing this spec. Findings that shaped the requirements:

- Q: Is `/api/v1` always available? → A: **No** — it is mounted only with `--enable-web`; without it
  every `/api/v1` route answers 404. Detection therefore cannot probe `/api/v1`.
- Q: Does writing require an MCP handshake? → A: **No** — `POST /mcp` is stateless
  (`stateful=false`); `tools/call` answers directly. It does require
  `Accept: application/json, text/event-stream`. A CLI write path (`write-page`, `delete-page`)
  exists independently.
- Q: Does an unscoped search leak across projects? → A: **Yes** — a search without
  `workspace`+`project` returns pages from every project. Scope must be mandatory in the app's own
  client, not optional.
- Q: Can the app remove exactly what it wrote? → A: **Yes** — `ai-memory uninstall` is dry-run by
  default, takes `--only hooks|mcp|instructions|skills`, and identifies the MCP entry by **URL**,
  never by name alone.
- Q: Does the first run reach the network unprompted? → A: **Yes** — the first `serve` downloads a
  ~87 MB local embedding model (`all-MiniLM-L6-v2`) in the background without asking.

## User Scenarios & Testing *(mandatory)*

### User Story 8 - A memory kernel the app owns (Priority: P1)

The developer opens Terminal AI. The memory kernel comes up on its own — or attaches to one already
running — and the memory panel keeps reading and writing exactly as before, now backed by a
git-versioned markdown wiki with hybrid search. When the kernel is missing, stopped or broken, the
panel says so plainly and everything else in the app keeps working.

**Why this priority**: Without a supervised, observable kernel there is nothing to build on, and an
unsupervised external dependency that can hang is a liability rather than a feature.

**Independent Test**: Kill the kernel process with six terminals running. No session dies, no pane
freezes, the UI stays interactive, and the memory panel shows an "unavailable" banner within 60s.
Start it again and the panel recovers without restarting the app.

**Acceptance Scenarios**:

1. **Given** no memory server running, **When** the app starts, **Then** the kernel is spawned from
   the bundled binary and reaches a ready state, and the memory panel lists entries.
2. **Given** a memory server the user started themselves, **When** the app starts, **Then** the app
   attaches to it, reports it as not-owned, and on app quit that server is **still running**.
3. **Given** an unrelated process holding the kernel's port, **When** the app starts, **Then** the
   app neither attaches nor spawns over it, and reports a port conflict with actionable guidance.
4. **Given** no `ai-memory` binary anywhere, **When** the app starts, **Then** the memory panel
   shows install guidance and every other feature works normally.
5. **Given** a running kernel, **When** N memory panels/cards are open, **Then** the kernel is
   polled by exactly one central poller and never once per panel.

---

### User Story 9 - The existing memory survives the move (Priority: P2)

The developer already has memory entries in the app. After upgrading, the panel offers to import
them into the kernel. They preview the report, run the import, and the entries appear as wiki pages
under the right project. Running it a second time imports nothing new.

**Why this priority**: Losing or silently duplicating a user's accumulated memory would be a defect,
and nothing downstream is trustworthy until the old acervo is accounted for.

**Independent Test**: With ≥50 legacy entries, run the import, confirm the page count matches, then
run it again and confirm zero new pages.

**Acceptance Scenarios**:

1. **Given** legacy entries exist, **When** the app starts, **Then** nothing is imported
   automatically and the panel reports how many entries are pending.
2. **Given** a pending import, **When** the user runs it in preview mode, **Then** they see what
   would be imported, skipped and why — and nothing is written.
3. **Given** a completed import, **When** it is run again, **Then** no page is created or
   duplicated.
4. **Given** an import interrupted midway, **When** it is run again, **Then** it resumes and
   completes without re-importing what already landed.
5. **Given** a completed import, **When** the user undoes it, **Then** the imported pages are
   removed and the legacy entries are still intact on disk.

---

### User Story 10 - Agents that share the same memory (Priority: P3)

The developer enables memory for a project. The app shows exactly what it would write into the
Claude/Codex/OpenCode configuration, they approve, and from then on agents launched in that project
can read and write the same memory the panel shows. Later they turn it off and the configuration
goes back to what it was.

**Why this priority**: This is the whole point of the feature — memory the agents actually use — but
it writes into files the app does not own, so it must come after the kernel is trustworthy.

**Independent Test**: Enable wiring for one project, open a Claude pane, ask it to call
`memory_status`, and confirm it reports the same page count the panel shows. Then remove the wiring
and diff the configuration files against the pre-apply snapshot.

**Acceptance Scenarios**:

1. **Given** a project without wiring, **When** the user opens the wiring flow, **Then** they see
   the target file path and a diff of the change, and nothing has been written yet.
2. **Given** a shown diff, **When** the user confirms, **Then** the change is applied, recorded, and
   the panel shows the project as wired.
3. **Given** applied wiring, **When** the user removes it, **Then** only what the app created is
   removed and unrelated configuration is untouched.
4. **Given** a configuration file edited by the user after the wiring was applied, **When** removal
   runs, **Then** the app refuses to clobber it, explains why, and points at the backup.
5. **Given** an ai-memory entry the user configured themselves, **When** the app previews wiring,
   **Then** it reports it as pre-existing and not managed, and does not overwrite it.
6. **Given** a wired project, **When** lifecycle capture has not been consented to, **Then** no
   lifecycle event is captured for that repository.
7. **Given** an agent whose capture cannot be confined to one project, **When** the user opens the
   wiring flow, **Then** on-demand memory access is offered and capture is not, with the reason
   stated — not silently omitted and not silently installed machine-wide.

---

### User Story 11 - Search that respects the project boundary (Priority: P4)

The developer searches memory. Results come from the project they are in — never from another
project — and clicking a result opens the page body.

**Why this priority**: Cross-project leakage would be a privacy defect, and the current UI cannot
even open a result.

**Independent Test**: Put the same keyword in two projects' memory; search from one and confirm only
that project's entry returns.

**Acceptance Scenarios**:

1. **Given** the same keyword in two projects, **When** the user searches from one, **Then** only
   that project's pages are returned.
2. **Given** a worktree of a project, **When** the user writes memory from it, **Then** the entry is
   available to the parent project and to that project's other worktrees.
3. **Given** a search result, **When** the user selects it, **Then** the page body is shown.
4. **Given** a page written by an agent rather than the app, **When** the user lists memory,
   **Then** it appears and is marked as agent-authored.
5. **Given** kernel content containing markup, **When** it is displayed, **Then** it is rendered as
   sanitized text and never interpreted as HTML.

---

### User Story 12 - Continuity when switching agents (Priority: P5)

The developer ends a Claude session in a project and opens a Codex pane in the same project. The
pending handoff — what was being done, open questions, next steps — is offered to them.

**Why this priority**: The highest-value capability the kernel unlocks, but it depends on wiring and
capture being in place first.

**Independent Test**: End a wired Claude session, open a Codex pane in the same project, and confirm
the handoff is offered with its summary.

**Acceptance Scenarios**:

1. **Given** a finished agent session, **When** the user opens the project's memory panel, **Then**
   any pending handoff is listed with its summary, open questions and next steps.
2. **Given** a pending handoff, **When** the next agent starts in that project, **Then** it
   receives the handoff — the app has not consumed it.
3. **Given** handoffs older than a chosen age, **When** the user expires them, **Then** they stop
   being offered.

---

### Edge Cases

- The kernel binary exists but is quarantined by Gatekeeper: it dies immediately on spawn; the app
  must recognize this and say how to clear the quarantine rather than reporting a generic failure.
- The kernel's port is taken by a second instance of Terminal AI: only one may own the process.
- The app is updated and the bundled binary moves: previously written agent hooks embed the old
  absolute path and would break silently.
- The kernel is upgraded out from under the app to a version whose output shape changed.
- The user's disk is full or the wiki's git repository is corrupt: writes fail while reads may still
  work.
- The user is offline on first run, when the kernel wants to fetch its embedding model.
- A project is renamed or moved on disk after its memory was written under the old identity.
- Two agents in different worktrees of the same repository write memory concurrently.

## Requirements *(mandatory)*

### Functional Requirements

**Memory kernel**

- **FR-038**: Every memory operation MUST go through a memory-kernel abstraction; the WebView MUST
  NEVER address the kernel's network endpoint directly.
- **FR-039**: The app MUST detect a memory server already listening at the configured address and
  attach to it, and MUST supervise — restart or terminate — only a process it started itself.
- **FR-040**: The app MUST positively identify a listening process as the memory kernel before
  attaching, and MUST NOT start a kernel over a port held by an unrelated process.
- **FR-041**: Memory-kernel unavailability MUST NOT block the UI, prevent opening or running
  sessions, or delay app start.
- **FR-042**: Kernel status MUST come from one central poller with a local cache; polling per panel
  or per card is PROHIBITED.
- **FR-043**: The app MUST recover a kernel process it started but lost track of (for example after
  a crash) rather than starting a second one.
- **FR-044**: The app MUST report a distinguishable kernel state for: not installed, starting,
  ready, attached-not-owned, degraded, port-conflict and failed — each with actionable guidance.

**Memory content**

- **FR-045**: Memory entries MUST remain scoped to global, project, worktree, workspace and session,
  and MUST be creatable, readable, editable and removable from the UI.
- **FR-046**: Project-scoped memory MUST NOT leak across projects: every read and write MUST carry
  an explicit project scope resolved from the requested scope; an unscoped query MUST NOT be
  issuable by the app.
- **FR-047**: Memory written from a worktree MUST belong to the worktree's parent project and be
  available to that project and its other worktrees.
- **FR-048**: Content returned by the kernel MUST be treated as untrusted: rendered as sanitized
  text or markdown, never interpreted as HTML, and bounded in length.
- **FR-049**: Pages authored outside the app (by an agent) MUST be listable and distinguishable from
  pages the app wrote.
- **FR-050**: The user MUST be able to see what memory would be composed into an agent's context
  before it is used.

**Migration**

- **FR-051**: Legacy memory entries MUST be importable into the kernel by explicit user action, and
  MUST NOT be imported automatically at startup.
- **FR-052**: The import MUST be previewable without writing, and MUST be idempotent — re-running it
  creates no duplicate.
- **FR-053**: The import MUST be resumable after interruption and MUST be undoable.
- **FR-054**: Legacy memory data MUST remain intact on disk after the import.

**Agent wiring**

- **FR-055**: Writing memory configuration into an agent CLI's files MUST follow preview → diff →
  apply → record-what-was-created → remove-only-what-it-created.
- **FR-056**: The app MUST NOT overwrite a memory configuration entry it did not create; it MUST
  report it as pre-existing and unmanaged.
- **FR-057**: Removal MUST NOT clobber a configuration file changed after the app applied its
  wiring; it MUST refuse, explain, and preserve a recoverable copy.
- **FR-058**: Automatic capture of agent lifecycle events MUST be OFF by default and enabled
  explicitly per project, behind consent that states what will be captured; a project that has not
  opted in MUST emit no events.
- **FR-065**: Capture wiring MUST be offered only for agents whose capture can actually be confined
  to one project. Where it cannot, the app MUST NOT install capture at all and MUST say why, rather
  than installing machine-wide capture behind a per-project consent. (As of the kernel version this
  feature pins, that means capture is offered for Claude Code only; Codex has no automatic hook
  installation and OpenCode's plugin is machine-wide. Both still get on-demand memory access.)
- **FR-059**: The app MUST detect wiring that has gone stale (for example because the kernel binary
  moved) and offer to re-apply it, rather than leaving it silently broken.

**Project identity**

- **FR-064**: A project is identified in the kernel by its repository directory name. When that
  directory is renamed or moved, the app MUST detect that its memory now lives under a stale
  identity and offer to re-point it; it MUST NOT silently split one project's memory in two.

**Continuity**

- **FR-060**: Pending handoffs for a project MUST be listable from the UI, and stale ones MUST be
  expirable. Creating a handoff is an agent action and MUST NOT be initiated by the app on the
  user's behalf. **Neither is accepting one**: a handoff is consumed by the next agent at session
  start, so an app that accepted it would silently deprive that agent of the context it was waiting
  for. The app's job here is to show that continuity is pending, and to clear it when it has gone
  stale.

**Safety & boundaries**

- **FR-061**: No kernel secret may be written to `app.db` or `config.toml`; a bearer token, when
  one exists, MUST live only in the OS keychain and MUST NEVER be passed on a command line or
  echoed back to the frontend.
- **FR-062**: Any network fetch the kernel performs on first run MUST be disclosed to the user
  before it happens. It MUST be declinable — by a kernel setting where upstream provides one, and
  otherwise by declining to start the kernel at all. The app MUST NOT reach the network on the
  user's behalf without having said so first.
- **FR-063**: The kernel MUST bind to loopback only; a non-loopback address MUST be refused.

### Key Entities *(include if feature involves data)*

- **Memory Kernel**: the supervised or attached memory server — its address, data location, version,
  ownership and current state.
- **Memory Page**: a scoped, versioned markdown record with a kind, a title, a body and an author
  (app or agent); replaces the legacy entry/revision pair.
- **Wiring Binding**: the record of what the app wrote into which agent configuration file for which
  project, with enough information to remove exactly that and to detect drift.
- **Migration Record**: the per-legacy-entry record of what was imported and where, making the
  import idempotent and undoable.
- **Handoff**: a typed continuity record between agent sessions in a project, with a state.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-012**: The memory kernel reaches a usable state within 5 seconds of app start on a typical
  run, and within 15 seconds on the very first run.
- **SC-013**: With six sessions running, killing the kernel process closes no session, freezes no
  pane, and the UI reflects the unavailability within 60 seconds.
- **SC-014**: A search issued from one project never returns a page belonging to another project.
- **SC-015**: An agent launched by the app in a wired project reports the same memory page count the
  app's own panel shows, and can read a page the panel created.
- **SC-016**: Running the legacy import twice produces the same number of pages as running it once.
- **SC-017**: Removing wiring restores every configuration file the app modified to its pre-apply
  content, and leaves unrelated entries in those files byte-identical.
- **SC-018**: A handoff created at the end of a session in one agent is offered at the start of a
  session in a different agent in the same project.
- **SC-019**: Quitting the app leaves an attached (not-owned) memory server still running.
- **SC-020**: Kernel status is fetched once per poll interval regardless of how many memory views
  are open.
- **SC-021**: Renaming a project's directory surfaces a stale-identity notice rather than an
  apparently empty memory.

## Assumptions

- **Platform**: macOS only, matching feature 001. The kernel binary is distributed for
  Apple Silicon and Intel.
- **Shared store**: the kernel's data location is ai-memory's own default, not a location private to
  Terminal AI, so the app and the user's own agents converge on one acervo.
- **Kernel identity**: the memory kernel is the `ai-memory` project, pinned to an exact version the
  app ships and verifies; it is not auto-upgraded underneath the app.
- **Zero-LLM default**: the kernel is used without any LLM or embedding provider configured; hybrid
  ranking beyond full-text search is a bonus, never a requirement.
- **Single user, local machine**: no multi-user server, no remote deployment in this feature. The
  ability to attach to a server the user runs themselves is in scope; running one for others is not.
- **Delivery order**: US8 → US9 → US10 → US11 → US12, each independently demonstrable. US10 is the
  first story that writes outside the app's own data directory.
- **Superseded scope**: this feature supersedes the memory half of feature 001 (FR-022, FR-023,
  FR-024 and SC-010). Those requirements remain true; their implementation moves to the kernel.
