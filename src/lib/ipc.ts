import { Channel, invoke } from "@tauri-apps/api/core";

export type LayoutNode =
  | { type: "pane"; paneId: string }
  | {
      type: "split";
      direction: "horizontal" | "vertical";
      sizes: number[];
      children: LayoutNode[];
    };
export type SessionState = "starting" | "running" | "exited" | "error";
export interface TerminalChunk {
  sessionId: string;
  seq: number;
  bytes: string;
}
export interface SessionSummary {
  sessionId: string;
  providerId: string;
  projectId?: string;
  worktreeId?: string;
  title: string;
  state: SessionState;
  pid: number;
}
export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  remote?: string;
  branch?: string;
  dirty: boolean;
  ahead: number;
  behind: number;
  activeSessions: number;
  archived: boolean;
  color?: string;
}
export interface WorktreeSummary {
  id: string;
  branch: string;
  path: string;
  dirty: boolean;
}
export interface UsageLine {
  label: string;
  value: string;
  pct?: number;
  resetsAt?: string;
}
export interface UsageCard {
  label: string;
  lines: UsageLine[];
  auth: "ok" | "unknown" | "expired" | "rejected";
  stale: boolean;
}
export interface UsageSnapshot {
  providers: Record<string, UsageCard>;
  updatedAt: string;
  offline: boolean;
}
export type Scope = {
  level: "global" | "project" | "worktree" | "workspace" | "session";
  refId?: string;
};
export interface SkillSummary {
  id: string;
  name: string;
  version: string;
  providers: string[];
}
export type MemoryType =
  | "fact"
  | "decision"
  | "constraint"
  | "preference"
  | "glossary"
  | "known_issue"
  | "command"
  | "todo";
export interface MemoryEntry {
  id: string;
  scope: Scope;
  type: MemoryType;
  title: string;
  body: string;
  /** Whether Terminal AI wrote this page, or an agent did in the shared store. */
  author: "terminal-ai" | "agent";
  createdAt: string;
  updatedAt: string;
}
export type KernelState =
  | "notInstalled"
  | "probing"
  | "starting"
  | "ready"
  | "attached"
  | "degraded"
  | "portConflict"
  | "failed";
export interface KernelStatus {
  state: KernelState;
  /** False for a server that was already running. The app never stops one of those. */
  owned: boolean;
  serverUrl: string;
  dataDir?: string | null;
  version?: string | null;
  versionMatchesPin: boolean;
  hasToken: boolean;
  pages?: number | null;
  pendingMigration: number;
  hybridSearch: boolean;
  lastCheckedAt: string;
  lastError?: string | null;
  guidance?: string | null;
}
export interface AppSettings {
  projectRoots: string[];
  keybindings: Record<string, string>;
  scrollbackLines: number;
  memoryAutoCapture: boolean;
  usageRefreshSeconds: number;
}
export interface MigrationSkip {
  entryId: string;
  reason: string;
}
export interface MigrationReport {
  total: number;
  alreadyImported: number;
  imported: number;
  skipped: MigrationSkip[];
  failed: MigrationSkip[];
  completedAt?: string | null;
}
export interface Handoff {
  id: string;
  agent: string;
  state: "open" | "accepted" | "expired";
  summary: string;
  openQuestions: string[];
  nextSteps: string[];
  createdAt: string;
  acceptedAt?: string | null;
}
export type WiringKind = "mcp" | "hooks";
export interface WiringPlan {
  agent: string;
  kind: WiringKind;
  path?: string | null;
  diff: string;
  willCreate: boolean;
  /** Set when something is already configured that Terminal AI did not create. */
  conflict?: string | null;
  /** Exactly which lifecycle events capture would record. Consent has to be informed. */
  captureEvents: string[];
  warnings: string[];
}
export interface WiringBinding {
  id: string;
  agent: string;
  kind: WiringKind;
  scopeRefId?: string | null;
  status: "applied" | "stale";
  path: string;
  appliedAt: string;
}
export type ResumeRef = { kind: "continue" } | { kind: "byId"; ref: string };
export interface PaneBinding {
  providerId?: string;
  projectId?: string;
  worktreeId?: string;
  title?: string;
}
export interface ProviderSummary {
  id: string;
  label: string;
  kind: "builtin" | "custom";
  color?: string;
  detected: boolean;
  auth: "ok" | "expired" | "unknown";
}

export const ipc = {
  resolveEnv: (force = false) =>
    invoke<{ path: string; env: Record<string, string>; cachedAt: string }>("resolve_env", {
      force,
    }),
  listSessions: () => invoke<{ sessions: SessionSummary[] }>("list_sessions"),
  createSession: (
    request: {
      projectId?: string;
      worktreeId?: string;
      providerId: string;
      cols: number;
      rows: number;
    },
    onOutput: (chunk: TerminalChunk) => void,
    // When set, the provider CLI resumes an existing conversation (e.g. `claude --continue`
    // / `--resume <id>`) instead of starting a blank session (FR-030).
    resume?: ResumeRef,
  ) => {
    const onOutputChannel = new Channel<TerminalChunk>();
    onOutputChannel.onmessage = onOutput;
    return invoke<{ sessionId: string; pid: number; title: string; state: SessionState }>(
      "create_session",
      { ...request, onOutput: onOutputChannel, resume },
    );
  },
  writeInput: (sessionId: string, data: string) =>
    invoke<{ ok: true }>("write_input", { sessionId, data }),
  resizeSession: (sessionId: string, cols: number, rows: number) =>
    invoke<{ ok: true }>("resize_session", { sessionId, cols, rows }),
  sendSignal: (sessionId: string, signal: "SIGINT" | "SIGTERM" | "SIGKILL" | "SIGHUP") =>
    invoke<{ ok: true }>("send_signal", { sessionId, signal }),
  closeSession: (sessionId: string) =>
    invoke<{ ok: true; exitCode?: number }>("close_session", { sessionId }),
  restartSession: (sessionId: string) =>
    invoke<{ sessionId: string; pid: number }>("restart_session", { sessionId }),
  getScrollback: (sessionId: string, maxBytes = 1_000_000) =>
    invoke<{ data: string; truncated: boolean }>("get_scrollback", { sessionId, maxBytes }),
  getSessionHistory: (projectId: string, limit = 100) =>
    invoke<{
      entries: Array<{
        id: string;
        providerId: string;
        worktreeId?: string;
        cwd: string;
        startedAt: string;
        endedAt?: string;
        title: string;
        resume?: { kind: "continue" } | { kind: "byId"; ref: string };
      }>;
    }>("get_session_history", { projectId, limit }),
  resumeSession: (
    historyId: string,
    cols: number,
    rows: number,
    onMessage: (chunk: TerminalChunk) => void,
  ) => {
    const onOutput = new Channel<TerminalChunk>();
    onOutput.onmessage = onMessage;
    return invoke<{ sessionId: string; pid: number; resumed: boolean }>("resume_session", {
      historyId,
      cols,
      rows,
      onOutput,
    });
  },
  listWorkspaces: () =>
    invoke<{
      workspaces: Array<{
        id: string;
        title: string;
        projectId?: string;
        active: boolean;
        rootPath?: string;
      }>;
    }>("list_workspaces"),
  createWorkspace: (title?: string, projectId?: string, worktreeId?: string) =>
    invoke<{ workspaceId: string }>("create_workspace", { title, projectId, worktreeId }),
  closeWorkspace: (workspaceId: string) => invoke<{ ok: true }>("close_workspace", { workspaceId }),
  saveLayout: (
    workspaceId: string,
    layout: LayoutNode,
    paneBindings: Record<string, PaneBinding> = {},
  ) => invoke<{ ok: true }>("save_layout", { workspaceId, layout, paneBindings }),
  loadLayout: (workspaceId: string) =>
    invoke<{ layout: LayoutNode; paneBindings: Record<string, PaneBinding> }>("load_layout", {
      workspaceId,
    }),
  listPresets: () => invoke<{ presets: Array<{ id: string; name: string }> }>("list_presets"),
  savePreset: (name: string, layout: LayoutNode, paneProviders: Record<string, string>) =>
    invoke<{ presetId: string }>("save_preset", { name, layout, paneProviders }),
  createWorkspaceFromPreset: (presetId: string, projectId?: string) =>
    invoke<{ workspaceId: string }>("create_workspace_from_preset", { presetId, projectId }),
  listProjects: (workspaceId?: string) =>
    invoke<{ projects: ProjectSummary[] }>("list_projects", { workspaceId }),
  pickDirectory: () => invoke<string | null>("pick_directory"),
  setProjectName: (projectId: string, name?: string) =>
    invoke<{ ok: true }>("set_project_name", { projectId, name }),
  setProjectArchived: (projectId: string, archived: boolean) =>
    invoke<{ ok: true }>("set_project_archived", { projectId, archived }),
  renameWorkspace: (workspaceId: string, title: string) =>
    invoke<{ ok: true }>("rename_workspace", { workspaceId, title }),
  setWorkspaceRoot: (workspaceId: string, path?: string) =>
    invoke<{ rootPath?: string }>("set_workspace_root", { workspaceId, path }),
  addProjectFolder: (path: string) =>
    invoke<{ project: ProjectSummary }>("add_project_folder", { path }),
  cloneProject: (url: string, destRoot: string, name?: string) =>
    invoke<{ project: ProjectSummary }>("clone_project", { url, destRoot, name }),
  removeProject: (projectId: string) =>
    invoke<{ ok: true }>("remove_project", { projectId, deleteFiles: false }),
  listWorktrees: (projectId: string) =>
    invoke<{ worktrees: WorktreeSummary[] }>("list_worktrees", { projectId }),
  createWorktree: (projectId: string, branch: string, createBranch: boolean) =>
    invoke<{ worktree: WorktreeSummary }>("create_worktree", {
      projectId,
      branch,
      createBranch,
    }),
  removeWorktree: (worktreeId: string) => invoke<{ ok: true }>("remove_worktree", { worktreeId }),
  listProviders: () => invoke<{ providers: ProviderSummary[] }>("list_providers"),
  detectProvider: (providerId: string) =>
    invoke<{ detected: boolean; path?: string; version?: string; auth: string; message?: string }>(
      "detect_provider",
      { providerId },
    ),
  upsertProviderProfile: (profile: {
    id: string;
    label: string;
    command: string;
    args: string[];
    color?: string;
    env?: Record<string, string>;
  }) => invoke<{ ok: true }>("upsert_provider_profile", profile),
  getUsage: () => invoke<UsageSnapshot>("get_usage"),
  refreshUsage: (providerId?: string) =>
    invoke<{ scheduled: boolean; nextAllowedAt: string }>("refresh_usage", { providerId }),
  listSkills: (projectId?: string) =>
    invoke<{
      skills: SkillSummary[];
      bindings: Array<{
        id: string;
        skillId: string;
        scope: string;
        scopeRefId?: string | null;
        enabled: boolean;
        appliedArtifacts: Array<{ providerId: string; path: string; marker: string }>;
      }>;
    }>("list_skills", { projectId }),
  previewSkillApply: (skillId: string, providerId: string, scope: Scope) =>
    invoke<{ diff: string; willCreate: string[] }>("preview_skill_apply", {
      skillId,
      providerId,
      scope,
    }),
  applySkill: (skillId: string, providerId: string, scope: Scope) =>
    invoke<{ created: string[] }>("apply_skill", { skillId, providerId, scope }),
  removeSkill: (skillId: string, providerId: string, scope: Scope) =>
    invoke<{ removed: string[] }>("remove_skill", { skillId, providerId, scope }),
  deleteSkill: (skillId: string) => invoke<{ ok: true }>("delete_skill", { skillId }),
  setSkillBinding: (skillId: string, scope: Scope, active: boolean) =>
    invoke<{ ok: true }>("set_skill_binding", { skillId, scope, active }),
  listMemory: (scope: Scope) => invoke<{ entries: MemoryEntry[] }>("list_memory", { scope }),
  // `scope` is required, not optional: an unscoped kernel query returns pages from every
  // project, so an optional parameter here would be a cross-project leak waiting to happen.
  searchMemory: (query: string, scope: Scope) =>
    invoke<{ entries: MemoryEntry[] }>("search_memory", { query, scope }),
  addMemory: (scope: Scope, type: MemoryType, title: string, body: string) =>
    invoke<{ entryId: string }>("add_memory", { scope, type, title, body }),
  captureSelectionToMemory: (
    sessionId: string,
    text: string,
    scope: Scope,
    type: MemoryType,
    title?: string,
  ) =>
    invoke<{ entryId: string }>("capture_selection_to_memory", {
      sessionId,
      text,
      scope,
      type,
      title,
    }),
  updateMemory: (scope: Scope, path: string, title: string | undefined, body: string) =>
    invoke<{ entryId: string }>("update_memory", { scope, path, title, body }),
  deleteMemory: (scope: Scope, path: string) =>
    invoke<{ ok: true }>("delete_memory", { scope, path }),
  readMemoryPage: (scope: Scope, path: string) =>
    invoke<{ page: MemoryEntry }>("read_memory_page", { scope, path }),
  getMemoryKernelStatus: () => invoke<{ status: KernelStatus }>("get_memory_kernel_status"),
  startMemoryKernel: () => invoke<{ status: KernelStatus }>("start_memory_kernel"),
  stopMemoryKernel: () => invoke<{ status: KernelStatus }>("stop_memory_kernel"),
  restartMemoryKernel: () => invoke<{ status: KernelStatus }>("restart_memory_kernel"),
  setMemoryKernelSettings: (patch: {
    serverUrl?: string;
    binaryPath?: string;
    autoStart?: boolean;
    hybridSearch?: boolean;
  }) => invoke<{ status: KernelStatus }>("set_memory_kernel_settings", patch),
  setMemoryKernelToken: (token: string | null) =>
    invoke<{ ok: true }>("set_memory_kernel_token", { token }),
  previewMemoryWiring: (agent: string, scope: Scope, kinds: WiringKind[]) =>
    invoke<{ plans: WiringPlan[] }>("preview_memory_wiring", { agent, scope, kinds }),
  applyMemoryWiring: (agent: string, scope: Scope, kinds: WiringKind[]) =>
    invoke<{ applied: string[] }>("apply_memory_wiring", { agent, scope, kinds }),
  removeMemoryWiring: (agent: string, scope: Scope) =>
    invoke<{ removed: string[] }>("remove_memory_wiring", { agent, scope }),
  listMemoryWiring: () =>
    invoke<{ bindings: WiringBinding[]; captureUnavailable: Array<[string, string]> }>(
      "list_memory_wiring",
    ),
  listMemoryHandoffs: (scope: Scope, state?: "open" | "all") =>
    invoke<{ handoffs: Handoff[] }>("list_memory_handoffs", { scope, state }),
  expireMemoryHandoffs: (scope: Scope, olderThanDays: number) =>
    invoke<{ expired: number }>("expire_memory_handoffs", { scope, olderThanDays }),
  getMemoryBriefing: (scope: Scope) =>
    invoke<{ briefing: string }>("get_memory_briefing", { scope }),
  checkMemoryProjectIdentity: (projectId: string) =>
    invoke<{
      stale: boolean;
      currentProject: string;
      previousProject?: string | null;
      previousPath?: string | null;
    }>("check_memory_project_identity", { projectId }),
  runMemoryMigration: (dryRun: boolean) =>
    invoke<MigrationReport>("run_memory_migration", { dryRun }),
  undoMemoryMigration: () =>
    invoke<{ deleted: string[] }>("undo_memory_migration", { confirm: true }),
  previewMemoryContext: (scope: Scope) =>
    invoke<{
      composed: string;
      sources: Array<{ entryId: string; scope: Scope }>;
    }>("preview_memory_context", { scope }),
  getSettings: () => invoke<{ settings: AppSettings }>("get_settings"),
  setSettings: (patch: Partial<AppSettings>) =>
    invoke<{ settings: AppSettings }>("set_settings", { patch }),
  notify: (title: string, body: string) =>
    invoke<{ ok: true; delivered: boolean }>("notify", { title, body }),
};
