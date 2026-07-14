# Feature Specification: AI Terminal Workspace

**Feature Branch**: `001-ai-terminal-workspace`

**Created**: 2026-07-14

**Status**: Draft

**Input**: User description: "Terminal macOS para agentes de IA — sidebar com projetos
clonados, área principal com terminais que o usuário adiciona e divide para Claude, Codex,
OpenCode e shell; sidebar também com skills, memória e uso (Claude/Codex/OpenCode); usar as
cores e o design system do github-visualize."

## Clarifications

### Session 2026-07-14

- Q: Which usage cards should v1 include? → A: Claude + Codex + OpenCode (the OpenCode card
  reflects its underlying configured provider).
- Q: Which provider backs OpenCode in this setup? → A: OpenRouter (balance/usage read via
  OpenRouter's documented API).
- Q: How should reopening/restarting an agent pane behave in v1? → A: Keep a per-project
  history of sessions; clicking a past session resumes it via the CLI's native
  resume/continue; a brand-new pane always starts fresh.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compose a workspace of AI-agent terminals (Priority: P1)

A developer opens the app and, in a workspace, adds one or more terminals — each running an
AI coding agent (Claude, Codex, OpenCode) or a plain shell. They split panes to the right or
below into an arbitrary layout, resize the divisions by dragging, temporarily maximize one
pane, and close panes. Each terminal is fully interactive (the agent behaves exactly as in a
native terminal). When the app is reopened, the workspace layout is restored exactly.

**Why this priority**: This is the core value proposition and a viable standalone MVP — being
able to run and arrange several AI agents side by side is the reason the product exists.

**Independent Test**: Launch the app, add a Claude pane, split it into the four wireframe
arrangements (single, 2×2 grid, two columns, asymmetric), type into each agent and confirm
interactive behavior, restart the app, and confirm the layout returns identically.

**Acceptance Scenarios**:

1. **Given** an empty workspace, **When** the user adds a terminal and picks "Claude", **Then**
   an interactive Claude session appears filling the workspace.
2. **Given** a single pane, **When** the user splits it right and then splits the new pane
   below, **Then** three independently sized panes exist and each can run a different agent.
3. **Given** a multi-pane layout, **When** the user drags a divider, **Then** the adjacent
   panes resize continuously and the agents re-render to the new size.
4. **Given** a pane in focus, **When** the user maximizes it, **Then** it temporarily fills
   the workspace and restoring returns the previous layout unchanged.
5. **Given** any layout, **When** the user quits and relaunches the app, **Then** the split
   tree, pane sizes, and each pane's provider are restored.
6. **Given** an empty region with a "+" affordance, **When** the user activates it, **Then**
   they can create a new terminal, choose a provider, choose a project/worktree, or restore a
   recent session into that space.

---

### User Story 2 - Organize work by cloned project (Priority: P2)

A developer sees their cloned projects in a sidebar, each showing its current branch and
whether it has uncommitted changes. They select a project and open a terminal or agent whose
working directory is that project. Sessions belonging to other projects keep running in the
background, with a visible activity indicator, and are not terminated by switching projects.

**Why this priority**: Agents are only useful in the context of a real repository; project
context turns the terminal grid into a development workspace.

**Independent Test**: With the existing local repos (`albert`, `dashboard`, `genfoot`) listed
in the sidebar with correct branch/status, open a shell in `albert` and confirm the working
directory equals the project path; start a session in a second project and confirm the first
keeps running.

**Acceptance Scenarios**:

1. **Given** configured project root directories, **When** the app scans them, **Then** each
   git repository is listed with name, branch, and clean/dirty state.
2. **Given** a selected project, **When** the user opens an agent, **Then** the agent starts
   with its working directory set to that project.
3. **Given** running sessions in project A, **When** the user selects project B, **Then**
   project A's sessions continue running and show an activity indicator.
4. **Given** the sidebar, **When** the user adds a folder or clones a repository by URL,
   **Then** it appears as a project without affecting existing files.
5. **Given** a project's session history, **When** the user clicks a past agent session,
   **Then** a pane opens that resumes that session via the agent's native resume/continue,
   while creating a brand-new pane always starts the agent fresh.

---

### User Story 3 - Track AI usage and limits (Priority: P3)

A developer glances at the sidebar to see how much of each provider's quota is consumed —
Claude (session/weekly/model), Codex (5-hour/weekly/code-review), and the OpenCode provider's
balance (OpenRouter, in this setup) — with reset timers. The values refresh on a sensible cadence and, when offline or
rate-limited, show the last known values rather than errors.

**Why this priority**: Usage awareness prevents surprise throttling but is secondary to being
able to run the agents in the first place.

**Independent Test**: With existing Claude/Codex authentication present, confirm the sidebar
cards populate; observe that a refresh happens once per provider per refresh window (not once
per terminal); disconnect the network and confirm the cards show the last snapshot.

**Acceptance Scenarios**:

1. **Given** valid provider authentication, **When** the workspace is open, **Then** each
   provider's usage card shows current consumption and reset timers.
2. **Given** multiple terminals of the same provider, **When** usage refreshes, **Then** only
   one refresh per provider occurs per window (no per-terminal polling).
3. **Given** the network is unavailable or the provider rate-limits, **When** a refresh is due,
   **Then** the card shows the last known values and a quiet status, not a hard error.
4. **Given** a narrow sidebar, **When** space is constrained, **Then** each card collapses to a
   single compact line.
5. **Given** expired provider authentication, **When** detected, **Then** the card indicates
   re-authentication is needed.

---

### User Story 4 - Isolate concurrent agents with worktrees (Priority: P4)

Because two agents editing the same repository at once causes conflicts, the developer creates
a git worktree (a branch on its own directory) and points a pane at it. Two agents can then
work on the same project in separate worktrees without stepping on each other's files.

**Why this priority**: Enables the multi-agent scenario safely, but depends on projects (P2)
existing first.

**Independent Test**: For one project, create two worktrees on different branches, open an
agent in each, have both modify files, and confirm neither sees the other's uncommitted
changes.

**Acceptance Scenarios**:

1. **Given** a project, **When** the user creates a worktree for a new or existing branch,
   **Then** a dedicated directory is created and available to assign to a pane.
2. **Given** two panes on two worktrees of the same project, **When** each agent edits files,
   **Then** the edits are isolated to their respective worktree directories.
3. **Given** a worktree is no longer needed, **When** the user removes it, **Then** it is
   detached cleanly without harming the main working copy.

---

### User Story 5 - Save and restore layout presets (Priority: P5)

The developer saves a useful arrangement as a named preset (e.g. "Review", "Implementation",
"Debug", "Multi-agent") and later creates a new workspace from that preset in one step.

**Why this priority**: A convenience multiplier on the core layout capability; valuable but not
required for first use.

**Independent Test**: Save a 2×2 layout as a preset, create a new workspace from it, and
confirm the split tree and provider assignments are reproduced.

**Acceptance Scenarios**:

1. **Given** a layout, **When** the user saves it as a named preset, **Then** the preset is
   available when creating a workspace.
2. **Given** a saved preset, **When** the user creates a workspace from it, **Then** the split
   tree is reproduced and each pane offers to start its assigned provider.
3. **Given** an existing layout, **When** the user duplicates it, **Then** an independent copy
   is created.

---

### User Story 6 - Share skills across agents (Priority: P6)

The developer maintains a single library of skills (reusable instruction sets) and activates a
skill for one or more scopes — globally, for a project, worktree, workspace, or a single
session — with a defined precedence. Applying a skill to an agent shows a preview and diff
first, records exactly what the app created, and removes only what it created.

**Why this priority**: A power feature that improves agent quality; it layers on top of working
agents and projects.

**Independent Test**: Activate one global skill for both Claude and Codex, confirm both agents
receive it without the user manually duplicating files, and confirm removal reverts only the
app-created content.

**Acceptance Scenarios**:

1. **Given** the skill library, **When** the user activates a skill at a scope, **Then** the
   skill applies to agents in that scope following the precedence session > workspace >
   worktree > project > global.
2. **Given** a skill to apply, **When** the user applies it, **Then** a preview and diff are
   shown before any change is written.
3. **Given** an applied skill, **When** the user deactivates it, **Then** only the app-created
   content is removed and the provider's own configuration is left intact.

---

### User Story 7 - Scoped project memory (Priority: P7)

The developer keeps searchable memory (facts, decisions, constraints) scoped to global,
project, worktree, workspace, or session. They can capture a selected snippet from a terminal
into memory. Automatic capture of terminal output is off by default. Memory of one project is
made available to that project's agents without leaking into other projects.

**Why this priority**: Deepens continuity across sessions; the most advanced layer, valuable but
last.

**Independent Test**: Add a memory entry scoped to `albert`, confirm it is offered to `albert`
agents and never to `dashboard` agents, and search returns it by keyword.

**Acceptance Scenarios**:

1. **Given** a memory entry scoped to a project, **When** an agent starts in that project,
   **Then** the entry is available to compose into context, and not for other projects.
2. **Given** a selection in a terminal, **When** the user chooses "save as memory", **Then**
   they pick a scope and the snippet is stored — and no automatic full-output capture occurs.
3. **Given** stored memory, **When** the user searches by keyword, **Then** matching entries are
   returned across the chosen scope.
4. **Given** memory to inject, **When** composing agent context, **Then** a preview is shown
   before injection.

---

### Edge Cases

- **CLI not found**: a chosen provider's executable is not on the resolved PATH → the pane shows
  a clear, actionable message (which command, how to install), not a silent failure.
- **Not authenticated / auth expired**: an agent starts but the provider is logged out → the
  session still opens (agent shows its own login prompt) and the usage card flags re-auth.
- **Usage endpoint offline or rate-limited** → last known snapshot shown; refresh backs off and
  never falls below the safe minimum interval.
- **Project moved or deleted on disk** → the project is flagged as unavailable; sessions bound to
  it surface an error without crashing the app.
- **Worktree/branch conflict** (branch already checked out elsewhere) → creation is refused with
  a clear reason.
- **Output flood** (an agent prints megabytes rapidly) → the pane stays responsive, scrollback is
  bounded, and other panes and the UI are unaffected.
- **Pane resized very small / window resized** → agents receive the new dimensions and re-render;
  no layout corruption.
- **App closed with active sessions** (v1): running processes end on close — expected in v1 — but
  each session is recorded in the project's history so it can be resumed on next launch via the
  agent's native resume (see Assumptions). The user is not led to believe the live process persists.

## Requirements *(mandatory)*

### Functional Requirements

**Terminals & layout**
- **FR-001**: The system MUST let a user add a terminal pane bound to a provider (Claude, Codex,
  OpenCode, shell, or a user-defined profile).
- **FR-002**: Each session MUST be fully interactive, indistinguishable from running the same
  program in a native terminal (input, output, resize, signals).
- **FR-003**: Users MUST be able to split any pane horizontally or vertically, producing an
  arbitrary split tree (not limited to a fixed grid), and resize divisions by dragging.
- **FR-004**: Users MUST be able to maximize a pane temporarily and restore the prior layout.
- **FR-005**: An empty region MUST offer to create a terminal, choose provider, choose
  project/worktree, or restore a recent session from the project's session history.
- **FR-006**: The system MUST persist each workspace's layout tree and restore it losslessly on
  relaunch.
- **FR-007**: The top bar MUST represent workspaces (not individual processes); users MUST be
  able to create, switch, and close workspaces.

**Projects & worktrees**
- **FR-008**: The system MUST discover git repositories under configured root directories and let
  users add a folder or clone by URL.
- **FR-009**: For each project the system MUST display name, current branch, and clean/dirty
  status; and MUST show ahead/behind where available.
- **FR-010**: Users MUST be able to open a session whose working directory is a selected project
  or worktree.
- **FR-011**: Selecting a project MUST NOT terminate sessions of other projects; background
  sessions MUST show an activity indicator.
- **FR-012**: Users MUST be able to create, list, and remove git worktrees for a project, and
  assign a pane to a specific worktree.

**Providers**
- **FR-013**: The system MUST detect each provider's executable and authentication state and show
  a clear message when a CLI is missing.
- **FR-014**: The system MUST start each provider in the selected working directory with a
  correctly resolved environment (so CLIs installed outside the default GUI PATH are found).
- **FR-015**: Users MUST be able to define custom provider profiles (label, command, arguments,
  color).
- **FR-016**: Each pane MUST show a compact header with provider, project, branch/worktree, and
  process state, plus context actions (split, duplicate, restart, change provider, change
  worktree, rename, export output, terminate).

**Usage**
- **FR-017**: The system MUST poll each supported provider's usage at most once per provider per
  refresh window (never per terminal/card) and share one snapshot across the UI. For v1 the
  supported providers are Claude, Codex, and OpenCode (whose card reflects its underlying
  provider, OpenRouter in this setup).
- **FR-018**: The system MUST honor a minimum refresh interval and cache results briefly to avoid
  aggressive requests to rate-limited endpoints.
- **FR-019**: The system MUST display usage cards with consumption and reset timers, a compact
  single-line mode, an offline/last-known state, and an expired-auth state.

**Skills & memory**
- **FR-020**: The system MUST maintain a single skill library and let users activate a skill at
  global/project/worktree/workspace/session scope with defined precedence.
- **FR-021**: Applying or removing a skill to a provider MUST show a preview/diff, record exactly
  what the app created, and on removal delete only app-created content.
- **FR-022**: The system MUST store memory entries scoped to global/project/worktree/workspace/
  session, support keyword search, and support capturing a selected terminal snippet into a
  chosen scope.
- **FR-023**: Automatic capture of terminal output into memory MUST be OFF by default and
  strictly opt-in.
- **FR-024**: Project-scoped memory MUST be available only to that project's agents and MUST NOT
  leak into other projects.

**Session history & resume**
- **FR-029**: The system MUST record a per-project history of agent sessions (provider, working
  directory, worktree, start/end time, and the agent's own resume reference where the CLI
  provides one), without persisting the live process across app close.
- **FR-030**: Users MUST be able to resume a past session from the history using the agent's
  native resume/continue capability when available; creating a brand-new pane MUST always start
  the agent fresh. When a provider offers no resume capability, the history entry MUST reopen a
  fresh session in the same working directory and indicate resume was unavailable.

**Safety & boundaries**
- **FR-025**: The frontend MUST NOT be able to execute arbitrary commands; all privileged actions
  MUST go through validated, typed operations (trusted project, allowed path, provider, cwd, env).
- **FR-026**: Credentials and API keys MUST remain in the OS keychain or the provider CLIs' own
  files and MUST NOT be stored in the app's database or config.
- **FR-027**: Terminal output MUST be treated as untrusted: no automatic execution of links,
  confirm external URLs, bounded scrollback, sanitized window titles, no HTML interpretation.
- **FR-028**: The failure of one session MUST NOT affect other sessions or block the UI.

### Key Entities *(include if feature involves data)*

- **Project**: a cloned git repository with path, remote, branch, status, color, default provider
  and default layout.
- **Worktree**: a branch of a project checked out in its own directory.
- **Workspace**: a working tab associated with a project or worktree, owning one layout tree.
- **Pane**: a leaf of the layout tree with position/size and visual configuration.
- **Session**: a process in a PTY (Claude/Codex/OpenCode/shell/custom) with provider, working
  directory, and state; also persisted as a per-project history record (with the agent's native
  resume reference where available) so a past session can be reopened/resumed.
- **Provider Profile**: an executable, arguments, environment, and color for an agent or command.
- **Skill**: a reusable instruction set with metadata and supported providers.
- **Skill Binding**: where a skill is active (scope) and its precedence.
- **Memory Entry / Revision**: scoped content with type and version history.
- **Usage Snapshot**: the last known consumption per provider with reset timers.
- **Layout**: the persisted split tree for a workspace, and named reusable presets.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can go from launch to an interactive AI-agent terminal in a chosen project
  in no more than three interactions.
- **SC-002**: The four wireframe layouts (single, 2×2, two-column, asymmetric) can each be built
  and, after an app restart, are restored identically (zero layout loss).
- **SC-003**: At least 12 terminals run simultaneously with the interface staying responsive.
- **SC-004**: Typing into any terminal feels instantaneous (no perceptible input lag), even while
  another pane produces heavy output.
- **SC-005**: The interface becomes usable within about 2 seconds of launch.
- **SC-006**: Each provider's usage is refreshed at most once per refresh window regardless of how
  many terminals or cards are open, verifiable from activity logs.
- **SC-007**: With the network disconnected, usage cards continue to show the last known values
  rather than errors.
- **SC-008**: Two agents can work on the same project concurrently via separate worktrees with no
  file conflicts between them.
- **SC-009**: A global skill activated for two providers reaches both agents without the user
  manually duplicating any files, and deactivation reverts only app-created content.
- **SC-010**: Project-scoped memory appears for that project's agents and never for another
  project's agents.
- **SC-011**: A user can reopen a past agent session from a project's history and continue where
  it left off (when the agent supports resume), in no more than two interactions.

## Assumptions

- **Platform**: macOS desktop only for v1; other operating systems are out of scope.
- **Session persistence (v1 boundary)**: the live process is NOT required to survive closing the
  app window in v1 — closing the app ends running processes. However, session *metadata* is
  persisted as a per-project history, and a past session can be resumed via the agent's native
  resume/continue (which relies on the CLI's own stored transcript), so "resume on click" works
  without keeping a process alive. Keeping live processes running in the background via a separate
  daemon is a deliberately deferred later phase; the architecture keeps the seam for it.
- **Provider CLIs**: `claude`, `codex`, and `opencode` are installed and authenticated by the
  user through their own tools; the app reuses that authentication and does not manage login.
- **Usage source of truth**: usage figures are read directly from the providers' own
  credential/usage mechanisms; there is no generic "OpenCode" limit — the OpenCode card reflects
  the underlying configured provider's balance/cost (OpenRouter, in this setup).
- **Delivery order**: the four post-core capabilities (worktrees, shared skills, scoped memory,
  layout presets) are all in scope and are delivered after the core terminal + projects + usage
  slices (P1–P3).
- **Single user** on a personal machine; no multi-user accounts or remote/networked server.
- **Design language** follows the `github-visualize` system (dark, monospace, thin borders,
  fuchsia accent), applied so that per-agent color appears only as small accents.
