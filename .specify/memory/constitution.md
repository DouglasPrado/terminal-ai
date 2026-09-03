<!--
Sync Impact Report
- Version change: 2.0.0 → 2.1.0
- Bump rationale: MINOR. The governance policy reserves MINOR for "a new principle or
  materially expanded guidance". Both edits are additive expansions: Principle III's capture
  rule is widened from raw terminal output to agent-lifecycle capture, and Principle VII's
  swappable-host seam is widened from sessions to memory. Nothing was removed and no existing
  requirement was redefined — every rule an implementation obeyed under 2.0.0 still holds
  verbatim. New requirements necessarily leave current code short of the bar; that is what
  MINOR means here, and is not the backward-incompatible redefinition MAJOR is reserved for.
  Renaming Principle VII is a title widening, not a redefinition of its existing clause.
- Modified principles:
  III. Non-Destructive & Credential-Safe by Default — capture rule extended to automatic
     agent-lifecycle capture (user prompts, tool calls, session start/end, subagent events)
     collected by third-party hooks: OFF by default, explicit per-project opt-in with informed
     consent, and no events at all from a repository that has not opted in.
  VII. Swappable Session Host → Swappable Session & Memory Hosts — the `SessionHost` clause is
     unchanged; a parallel `MemoryKernel` clause is added so the memory host can be a
     supervised sidecar, an attached external server, or a future daemon without touching the
     UI or the command/event contracts. Adds the ownership rule: supervise only the process
     the app started; never kill or restart a server it merely found running.
- Added principles: none
- Added sections: none
- Removed sections: none
- Templates requiring updates:
  ✅ .specify/templates/plan-template.md (Constitution Check is generic; no principle named)
  ✅ .specify/templates/spec-template.md (no principle reference)
  ✅ .specify/templates/tasks-template.md (no principle reference)
  ✅ CLAUDE.md §1 (principle 3 and 7 summaries updated to match)
  ✅ docs/design-tokens.md (cites Principle IV only — unaffected)
  ⚠ specs/001-ai-terminal-workspace/plan.md — left as-is on purpose: its Constitution Check
    table is the dated record of the gate feature 001 passed under 2.0.0. Rewriting row VII to
    claim a `MemoryKernel` that 001 never had would be false. The new clauses are satisfied by
    feature 002, which supersedes 001's memory subsystem.
  ⚠ docs/validation-2026-07-14.md — dated observation record, not live guidance (unchanged).
- Follow-up TODOs: feature 002 (ai-memory as memory kernel) MUST introduce the `MemoryKernel`
  abstraction and the per-project lifecycle-capture consent gate; until it lands, the memory
  subsystem is knowingly short of Principles III and VII, and its plan's Constitution Check
  MUST say so rather than claim a pass.
-->
# Terminal AI Constitution

Terminal AI is a macOS desktop workspace for AI-assisted development: a sidebar of cloned
projects, skills, memory and usage cards, plus a main area where the user opens and splits
multiple terminals running `claude`, `codex`, `opencode` or a shell — each optionally
pointing at a different project, branch or git worktree. This constitution defines the
non-negotiable principles that govern how the product is built.

## Core Principles

### I. Typed Rust Boundary
All privileged operations — spawning processes, PTY I/O, filesystem access, git actions,
credential reads and usage polling — MUST live in Rust and be exposed to the WebView only
through explicit, typed commands (e.g. `create_session`, `write_input`, `resize_session`,
`clone_project`, `create_worktree`). The frontend MUST NEVER receive a generic
shell-execution primitive such as `execute_any_command(string)`. Every command MUST
validate, before acting: allowed path — every `path`/`cwd` MUST canonicalize to a location
under a configured project root or one of its worktrees — allowed provider, working
directory, and environment. Rationale: the WebView renders untrusted terminal output; a
narrow, typed boundary is the only defensible attack surface. The project root constraint
is what bounds where a session may launch; there is no per-project trust flag, because the
roots are nominated by the user and a second per-repo gate on top of them earned nothing.

### II. Native PTY Fidelity
Every agent or shell session MUST run in a real pseudo-terminal (PTY). Sessions receive raw
input/output, window resize, and signals, so `claude`/`codex`/`opencode` behave exactly as
in a native terminal. Capturing `stdout` through a non-PTY pipe wrapper is PROHIBITED for
interactive sessions, because it breaks interactivity and TUI rendering. Terminal output
MUST be streamed to the WebView in time-batched blocks (never one message per byte).

### III. Non-Destructive & Credential-Safe by Default
The app MUST NOT blindly overwrite provider configuration. Skill and memory sync MUST
follow generate → preview (diff) → apply → record-what-was-created → remove-only-what-it-
created. Credentials and API keys MUST remain in the macOS Keychain or the official
CLI-managed files (`~/.claude/.credentials.json`, `~/.codex/auth.json`, …) and MUST NEVER be
written to `app.db` or `config.toml`. Terminal output is untrusted: no automatic link
execution, URLs confirmed before opening, clipboard controlled, scrollback bounded, window
titles sanitized, HTML never interpreted. Automatic capture of terminal output into memory
is OFF by default and strictly opt-in (output may contain tokens, secrets, env vars). This
capture rule extends beyond raw terminal output to automatic agent-lifecycle capture — user
prompts, tool calls, session start/end and subagent events collected by third-party hooks —
because that content carries the same secrets. Lifecycle capture MUST be OFF by default and
enabled explicitly per project, behind informed consent that states what will be captured; a
repository that has not opted in MUST NOT emit any event at all.

### IV. Single Source of Truth
Usage MUST be polled once per provider by one central poller, honoring a ≥300s refresh
floor and a ~60s local cache with locking; every UI element reads the same snapshot. Polling
per-terminal or per-card is PROHIBITED. Design tokens (colors, fonts, spacing) MUST have a
single source — one Tailwind `@theme` block — and MUST NOT be redefined ad hoc in components.
Rationale: the Anthropic/Codex usage endpoints are undocumented and rate-limit aggressively;
duplicated polling or duplicated tokens create both technical and visual drift.

### V. Layout as a Persisted Tree
A workspace layout MUST be modeled as an arbitrary tree of splits (pane / horizontal split /
vertical split), never a fixed grid — so asymmetric arrangements are first-class. Layout
MUST persist and restore losslessly across app restarts. Zero layout loss is a hard
requirement; a lost or corrupted layout is a defect, not an inconvenience.

### VI. Isolation & Resilience
The failure of one session (crash, hang, flood of output) MUST NOT affect any other session
or block the UI. Agents that may edit the same repository concurrently MUST be isolatable
into separate git worktrees to prevent write conflicts. Selecting a project in the sidebar
MUST NOT terminate sessions of other projects; they continue running in the background with
a visible activity indicator.

### VII. Swappable Session & Memory Hosts
The command layer MUST sit behind a `SessionHost` abstraction. The v1 in-process runtime
(PTYs inside the Tauri process) MUST be replaceable by a persistent daemon (LaunchAgent +
Unix socket) in a later phase WITHOUT changing the UI or the command/event contracts.
Memory operations MUST likewise sit behind a `MemoryKernel` abstraction, so the memory host
can be a sidecar binary the app supervises, an already-running external server the app merely
attaches to, or a future daemon — again WITHOUT changing the UI or the command/event
contracts. Ownership rule: the app MUST supervise — restart, terminate — only a process it
started itself; a server it merely found running MUST NEVER be killed or restarted by the app.
Rationale: session persistence across window close is deferred and the memory kernel is an
external program on its own release cycle, but the seams that absorb both MUST exist from day
one so neither becomes a rewrite.

## Additional Constraints & Security

- **Platform**: macOS desktop, Tauri 2 (Rust core + WebView UI).
- **Environment resolution**: because Finder-launched apps do not inherit the login shell
  `PATH`, the app MUST resolve the login-shell environment once, cache it, and allow editing
  in settings; otherwise `claude`/`codex`/`opencode` are "not found".
- **Design system**: the visual language is ported from `github-visualize` — near-black
  `#0b0a10` background, translucent `neutral-950/60` panels, thin `neutral-800` borders, low
  `rounded-lg` corners, all-monospace type, fuchsia (`#e879f9`) primary accent with a pink
  `#f472b6` / cyan `#22d3ee` data pairing. Per-agent color appears only as small accents
  (top strip, status dot, icon, active border), never filling a pane.
- **State vs content**: SQLite (`app.db`) holds structured state; Markdown holds portable
  memory/skill content; secrets live only in Keychain / CLI files.
- **`.claude/` hygiene**: agent folders may store credentials/tokens; they MUST be
  git-ignored to prevent leakage.

## Development Workflow & Quality Gates

- **Spec-Driven Development**: every feature flows through the Spec Kit pipeline
  (constitution → specify → clarify → plan → tasks → analyze → implement). Code is not
  written before its spec, plan and tasks exist.
- **Verification-first for runtime behavior**: each phase defines an executable acceptance
  criterion driven end-to-end (open ≥12 terminals, reproduce the four wireframe layouts and
  restore them after restart, open a CLI in a project's cwd, prove one poll per provider),
  not merely unit tests. A phase is "done" only when its acceptance criterion is observed.
- **Performance budgets (enforced, not aspirational)**: UI boot < 2s; typing latency below
  one frame; ≥12 simultaneous terminals; heavy output never blocks the UI; automatic
  reconnection to the session host; one poller per provider.
- **Security review**: any change touching the command boundary, credential handling, or
  terminal-output rendering requires explicit review against Principles I–III.

## Governance

This constitution supersedes ad-hoc practices for Terminal AI. Amendments MUST be documented
in the Sync Impact Report at the top of this file, versioned semantically, and propagated to
the dependent templates (`plan-template.md`, `spec-template.md`, `tasks-template.md`).

- **Versioning policy**: MAJOR for backward-incompatible principle removals/redefinitions;
  MINOR for a new principle or materially expanded guidance; PATCH for clarifications and
  wording.
- **Compliance review**: every plan MUST pass the Constitution Check gate; unavoidable
  deviations MUST be justified in the plan's Complexity Tracking and, if they violate a
  principle, either be redesigned or trigger a constitution amendment.
- **Precedence**: when guidance conflicts, this constitution wins; the approved plan file is
  secondary; convenience is never a justification to violate Principles I–III.

**Version**: 2.1.0 | **Ratified**: 2026-07-14 | **Last Amended**: 2026-09-03
