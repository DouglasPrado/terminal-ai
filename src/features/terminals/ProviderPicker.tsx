import { useEffect, useState } from "react";
import { GitBranch, History } from "lucide-react";
import {
  ipc,
  type ProjectSummary,
  type ProviderSummary,
  type WorktreeSummary,
} from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Field, Select } from "../../components/Field";
import { ProviderIcon } from "../../lib/providers";

const fallback: ProviderSummary[] = [
  {
    id: "claude",
    label: "Claude",
    kind: "builtin",
    color: "var(--color-accent)",
    detected: true,
    auth: "unknown",
  },
  {
    id: "codex",
    label: "Codex",
    kind: "builtin",
    color: "var(--color-cyan)",
    detected: true,
    auth: "unknown",
  },
  {
    id: "opencode",
    label: "OpenCode",
    kind: "builtin",
    color: "var(--color-provider-opencode)",
    detected: true,
    auth: "unknown",
  },
  {
    id: "shell",
    label: "Shell",
    kind: "builtin",
    color: "var(--color-text-muted)",
    detected: true,
    auth: "unknown",
  },
];
type Recent = { id: string; title: string; providerId: string };

/**
 * What an unbound pane shows. The one screen in the app that is mostly empty,
 * so it carries the HUD treatment: wireframe ground, bracketed console, and the
 * single lit control being the action you almost always want.
 */
export function ProviderPicker({
  onSelect,
  onResumeProvider,
  onResume,
  defaultProvider,
  defaultProjectId,
  defaultWorktreeId,
  workspaceId,
}: {
  onSelect: (providerId: string, projectId?: string, worktreeId?: string) => void;
  onResumeProvider?: () => void;
  onResume?: (historyId: string) => void;
  defaultProvider?: string;
  defaultProjectId?: string;
  defaultWorktreeId?: string;
  /** Scopes the working-directory list to the folder this workspace watches. */
  workspaceId?: string;
}) {
  const [providers, setProviders] = useState(fallback);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectId, setProjectId] = useState(defaultProjectId);
  const [worktreeId, setWorktreeId] = useState(defaultWorktreeId);
  const [worktrees, setWorktrees] = useState<WorktreeSummary[]>([]);
  const [recent, setRecent] = useState<Recent[]>([]);
  useEffect(() => {
    void ipc
      .listProviders()
      .then((result) => setProviders(result.providers))
      .catch(() => {});
    void ipc
      .listProjects(workspaceId)
      .then((result) => setProjects(result.projects.filter((project) => !project.archived)))
      .catch(() => {});
  }, [workspaceId]);
  useEffect(() => {
    setWorktreeId(projectId === defaultProjectId ? defaultWorktreeId : undefined);
    if (projectId) {
      void ipc.getSessionHistory(projectId, 4).then((result) => setRecent(result.entries));
      void ipc.listWorktrees(projectId).then((result) => setWorktrees(result.worktrees));
    } else {
      setRecent([]);
      setWorktrees([]);
    }
  }, [projectId, defaultProjectId, defaultWorktreeId]);
  const project = projects.find((entry) => entry.id === projectId);
  const worktree = worktrees.find((entry) => entry.id === worktreeId);
  const branch = worktree?.branch ?? project?.branch;
  const dirty = worktree?.dirty ?? project?.dirty;
  const saved = defaultProvider
    ? providers.find((provider) => provider.id === defaultProvider)
    : undefined;
  return (
    <div className="hud-grid grid h-full place-items-center overflow-auto bg-app p-5">
      <div className="relative w-full max-w-[380px] rounded-panel border border-border bg-panel/90 p-4 backdrop-blur-sm">
        <Brackets />
        <p className="mb-3 font-mono text-readout uppercase tracking-[0.2em] text-text-faint">
          {saved ? `sessão · ${saved.label}` : "sessão · nova"}
        </p>

        {saved && (
          <div className="mb-4">
            <Button variant="accent" block onClick={() => onResumeProvider?.()}>
              <ProviderIcon id={saved.id} size={14} />
              Retomar {saved.label}
            </Button>
            <button
              type="button"
              onClick={() => onSelect(saved.id, projectId, worktreeId)}
              className="mt-1.5 w-full text-center text-meta text-text-faint underline-offset-2 transition-colors hover:text-text hover:underline"
            >
              ou começar do zero
            </button>
          </div>
        )}

        <div className="space-y-2">
          <Field label="Diretório de trabalho">
            <Select
              value={projectId ?? ""}
              onChange={(event) => setProjectId(event.target.value || undefined)}
            >
              <option value="">Home</option>
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                  {project.branch ? ` — ${project.branch}` : ""}
                </option>
              ))}
            </Select>
          </Field>
          {projectId && worktrees.length > 0 && (
            <Field label="Worktree">
              <Select
                value={worktreeId ?? ""}
                onChange={(event) => setWorktreeId(event.target.value || undefined)}
              >
                <option value="">Raiz do projeto</option>
                {worktrees.map((worktree) => (
                  <option key={worktree.id} value={worktree.id}>
                    {worktree.branch}
                  </option>
                ))}
              </Select>
            </Field>
          )}
        </div>

        {project && (
          <p className="mt-2 flex items-center gap-1.5 font-mono text-readout">
            <GitBranch
              size={11}
              className={dirty ? "shrink-0 text-warning" : "shrink-0 text-success"}
            />
            <span className="min-w-0 flex-1 truncate text-text-muted">
              {branch ?? "sem branch"}
            </span>
            {(project.ahead > 0 || project.behind > 0) && (
              <span className="shrink-0 tabular-nums text-text-faint">
                ↑{project.ahead} ↓{project.behind}
              </span>
            )}
            <span className={dirty ? "shrink-0 text-warning" : "shrink-0 text-success"}>
              {dirty ? "alterado" : "limpo"}
            </span>
          </p>
        )}

        <p className="mb-1.5 mt-4 text-meta text-text-muted">Agente</p>
        <div className="grid grid-cols-2 gap-1.5">
          {providers.map((provider) => (
            <button
              key={provider.id}
              type="button"
              disabled={!provider.detected}
              title={provider.detected ? provider.kind : "CLI não encontrada no PATH resolvido"}
              onClick={() => onSelect(provider.id, projectId, worktreeId)}
              className="group flex h-9 items-center gap-2 rounded-control border border-border bg-raised px-2.5 text-ui font-medium text-text shadow-raised transition-colors hover:border-border-hover hover:bg-raised-hover hover:text-text-strong active:translate-y-px disabled:pointer-events-none disabled:opacity-35"
            >
              <span
                className="shrink-0 transition-[filter]"
                style={{ color: provider.color ?? "var(--color-text-muted)" }}
              >
                <ProviderIcon id={provider.id} size={15} />
              </span>
              <span className="truncate">{provider.label}</span>
            </button>
          ))}
        </div>

        {recent.length > 0 && (
          <div className="mt-4 border-t border-border-subtle pt-2.5">
            <p className="mb-1 flex items-center gap-1.5 text-meta text-text-muted">
              <History size={12} /> Sessões recentes
            </p>
            {recent.map((entry) => (
              <button
                key={entry.id}
                type="button"
                className="flex h-7 w-full items-center justify-between gap-2 rounded-control px-1.5 text-meta text-text-muted transition-colors hover:bg-raised hover:text-text"
                onClick={() => onResume?.(entry.id)}
              >
                <span className="truncate">{entry.title}</span>
                <span className="shrink-0 font-mono text-readout text-text-faint">
                  {entry.providerId}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** HUD corner ticks — the one deliberately theatrical detail in the app. */
function Brackets() {
  const arm = "pointer-events-none absolute size-3 border-accent/45";
  return (
    <>
      <span className={`${arm} -left-px -top-px rounded-tl-panel border-l border-t`} />
      <span className={`${arm} -right-px -top-px rounded-tr-panel border-r border-t`} />
      <span className={`${arm} -bottom-px -left-px rounded-bl-panel border-b border-l`} />
      <span className={`${arm} -bottom-px -right-px rounded-br-panel border-b border-r`} />
    </>
  );
}
