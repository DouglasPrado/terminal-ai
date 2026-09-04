import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Archive,
  Brain,
  FolderGit2,
  FolderOpen,
  Gauge,
  RefreshCw,
  Sparkles,
  TerminalSquare,
} from "lucide-react";
import { Button } from "./components/Button";
import { Tooltip } from "./components/Tooltip";
import { SidebarFrame } from "./components/SidebarFrame";
import { WorkspaceTabs } from "./components/WorkspaceTabs";
import { ProjectList } from "./features/projects/ProjectList";
import { WorktreePickerModal } from "./features/projects/WorktreePickerModal";
import { SkillsView } from "./features/skills/SkillsView";
import { MemoryView } from "./features/memory/MemoryView";
import { InvisibleModeBadge } from "./features/settings/InvisibleModeBadge";
import { SettingsMenu } from "./features/settings/SettingsMenu";
import { UsageCards } from "./features/usage/UsageCards";
import { LayoutTree, type PaneBinding } from "./features/workspaces/WorkspaceLayout";
import { Presets } from "./features/workspaces/Presets";
import { ProviderProfiles } from "./features/providers/ProviderProfiles";
import {
  closePane,
  neighborPane,
  pane,
  resizeSplit,
  splitPane,
  type FocusDirection,
} from "./features/workspaces/layoutTree";
import {
  ipc,
  type AppSettings,
  type LayoutNode,
  type ProjectSummary,
  type ResumeRef,
  type TerminalChunk,
} from "./lib/ipc";

export default function App() {
  const [tabs, setTabs] = useState<Array<{ id: string; title: string }>>([]);
  const [activeId, setActiveId] = useState<string>();
  const [layout, setLayout] = useState<LayoutNode>();
  const [bindings, setBindings] = useState<Record<string, PaneBinding>>({});
  const [maximized, setMaximized] = useState<string>();
  const [focusedPaneId, setFocusedPaneId] = useState<string>();
  const [toast, setToast] = useState<{ text: string; tone: "error" | "info" }>();
  const [worktreePicker, setWorktreePicker] = useState<{ paneId: string }>();
  const [view, setView] = useState<"workspace" | "skills" | "memory">("workspace");
  const [showArchived, setShowArchived] = useState(false);
  const [workspaceRoots, setWorkspaceRoots] = useState<Record<string, string | undefined>>({});
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string>();
  const [selectedWorktreeId, setSelectedWorktreeId] = useState<string>();
  const [settings, setSettings] = useState<AppSettings>({
    projectRoots: ["~/www"],
    keybindings: {
      newWorkspace: "Meta+N",
      splitRight: "Meta+D",
      splitDown: "Meta+Shift+D",
      maximizePane: "Meta+Shift+Enter",
      focusLeft: "Meta+Shift+ArrowLeft",
      focusRight: "Meta+Shift+ArrowRight",
      focusUp: "Meta+Shift+ArrowUp",
      focusDown: "Meta+Shift+ArrowDown",
    },
    scrollbackLines: 10_000,
    memoryAutoCapture: false,
    usageRefreshSeconds: 300,
    invisibleMode: false,
  });
  // Per-workspace layout+bindings kept in memory so switching workspaces preserves live
  // sessionIds (which the persisted layout omits) — enables reattaching a running terminal
  // when switching back to a workspace (FR-011 / T060).
  const workspaceCache = useRef<
    Record<string, { layout: LayoutNode; bindings: Record<string, PaneBinding> }>
  >({});

  useEffect(() => {
    void ipc.resolveEnv().catch(() => {});
    void ipc.getSettings().then((result) => setSettings(result.settings));
    void ipc
      .listProjects()
      .then((result) => setProjects(result.projects))
      .catch(() => {});
    void ipc
      .listWorkspaces()
      .then(async ({ workspaces }) => {
        if (workspaces.length === 0) {
          const created = await ipc.createWorkspace("Workspace");
          workspaces = [{ id: created.workspaceId, title: "Workspace", active: true }];
        }
        setTabs(workspaces.map(({ id, title }) => ({ id, title })));
        setWorkspaceRoots(Object.fromEntries(workspaces.map(({ id, rootPath }) => [id, rootPath])));
        setActiveId(workspaces[0].id);
      })
      .catch(() => {
        setTabs([{ id: "preview", title: "Workspace" }]);
        setActiveId("preview");
        setLayout(pane(crypto.randomUUID()));
      });
  }, []);

  // A workspace is defined by the folder it watches, so creating one asks for that folder
  // straight away via the native chooser. Cancelling is fine — the workspace then falls back to
  // the globally configured project roots.
  const chooseWorkspaceFolder = useCallback(async (workspaceId: string) => {
    const picked = await ipc.pickDirectory().catch(() => null);
    if (!picked) return;
    const result = await ipc.setWorkspaceRoot(workspaceId, picked).catch(() => undefined);
    setWorkspaceRoots((current) => ({ ...current, [workspaceId]: result?.rootPath ?? picked }));
    window.dispatchEvent(new CustomEvent("projects-refresh"));
  }, []);

  const createWorkspace = useCallback(async () => {
    const created = await ipc.createWorkspace();
    setTabs((current) => [
      ...current,
      { id: created.workspaceId, title: `Workspace ${current.length + 1}` },
    ]);
    setActiveId(created.workspaceId);
    await chooseWorkspaceFolder(created.workspaceId);
  }, [chooseWorkspaceFolder]);

  // Refresh the sidebar's data in place (FR-032). A full WebView reload would drop every pane's
  // sessionId and orphan the still-running in-process PTYs, so we re-fetch instead of reloading:
  // ProjectList listens for `projects-refresh`; UsageCards listens for `sidebar-refresh`.
  const refreshSidebar = useCallback(() => {
    window.dispatchEvent(new CustomEvent("projects-refresh"));
    window.dispatchEvent(new CustomEvent("sidebar-refresh"));
    setToast({ text: "Sidebar atualizada — terminais preservados", tone: "info" });
  }, []);

  // Intercept the platform reload shortcut so it refreshes the sidebar instead of wiping the
  // terminals (FR-032). Shift is the escape hatch: Cmd/Ctrl+Shift+R does reload the WebView for
  // real, which is what you want while iterating on the UI. It costs the panes' session bindings
  // — the processes keep running, but reattaching to them after a reload is still deferred
  // (docs/deferred.md, T089), so the panes come back showing the picker.
  useEffect(() => {
    const onReload = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "r") {
        event.preventDefault();
        // Reload explicitly rather than letting the event through: the WebView does not
        // reliably bind Cmd+Shift+R to a reload of its own.
        if (event.shiftKey) window.location.reload();
        else refreshSidebar();
      }
    };
    window.addEventListener("keydown", onReload);
    return () => window.removeEventListener("keydown", onReload);
  }, [refreshSidebar]);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const shortcut = eventShortcut(event);
      const action = Object.entries(settings.keybindings).find(
        ([, value]) => value === shortcut,
      )?.[0];
      if (!action) return;
      const paneId = maximized ?? focusedPaneId ?? (layout ? firstPaneId(layout) : undefined);
      if (action === "newWorkspace") {
        event.preventDefault();
        void createWorkspace();
      } else if (layout && paneId && (action === "splitRight" || action === "splitDown")) {
        event.preventDefault();
        const next = splitPane(
          layout,
          paneId,
          action === "splitRight" ? "horizontal" : "vertical",
          crypto.randomUUID(),
        );
        setLayout(next);
        if (activeId && activeId !== "preview") void ipc.saveLayout(activeId, next, bindings);
      } else if (paneId && action === "maximizePane") {
        event.preventDefault();
        setMaximized((current) => (current === paneId ? undefined : paneId));
      } else if (layout && paneId && focusDirections[action]) {
        // Move keyboard focus to the spatially adjacent pane (FR-031). Always preventDefault so
        // Cmd/Ctrl+Arrow never triggers a WebView default (e.g. history navigation) at an edge.
        event.preventDefault();
        const target = neighborPane(layout, paneId, focusDirections[action]);
        if (target) {
          setFocusedPaneId(target);
          setMaximized((current) => (current ? target : current));
        }
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [activeId, bindings, createWorkspace, layout, maximized, focusedPaneId, settings.keybindings]);

  useEffect(() => {
    if (!activeId) return;
    const cached = workspaceCache.current[activeId];
    if (cached) {
      setLayout(cached.layout);
      // Reconcile against live sessions: drop sessionIds whose process died in the background.
      void ipc
        .listSessions()
        .then(({ sessions }) => {
          const alive = new Set(sessions.map((session) => session.sessionId));
          const reconciled = Object.fromEntries(
            Object.entries(cached.bindings).map(([paneId, binding]) => [
              paneId,
              binding.sessionId && !alive.has(binding.sessionId)
                ? { ...binding, sessionId: undefined }
                : binding,
            ]),
          );
          setBindings(reconciled);
        })
        .catch(() => setBindings(cached.bindings));
      return;
    }
    void ipc
      .loadLayout(activeId)
      .then(({ layout: restored, paneBindings }) => {
        setLayout(restored);
        setBindings(paneBindings);
      })
      .catch(() => {
        const initial = pane(crypto.randomUUID());
        setLayout(initial);
        if (activeId !== "preview") void ipc.saveLayout(activeId, initial, {});
      });
  }, [activeId]);

  // Keep the in-memory cache current so a workspace switch can restore live sessions.
  useEffect(() => {
    if (activeId && layout) {
      workspaceCache.current[activeId] = { layout, bindings };
    }
  }, [activeId, layout, bindings]);

  // Backend event stream: mark exited panes (T058/T066), surface host errors (T070),
  // and refresh project activity when a session ends (T069).
  useEffect(() => {
    const exited = listen<{ sessionId: string }>("process-exited", ({ payload }) => {
      setBindings((current) => {
        const entry = Object.entries(current).find(
          ([, binding]) => binding.sessionId === payload.sessionId,
        );
        if (!entry) return current;
        const [paneId, binding] = entry;
        return { ...current, [paneId]: { ...binding, exited: true } };
      });
      window.dispatchEvent(new CustomEvent("projects-refresh"));
    });
    const hostError = listen<{ code: string; message: string }>("host-error", ({ payload }) => {
      setToast({ text: `${payload.code}: ${payload.message}`, tone: "error" });
    });
    return () => {
      void exited.then((unlisten) => unlisten());
      void hostError.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(undefined), 6000);
    return () => clearTimeout(timer);
  }, [toast]);

  // Dynamic terminal titles (T068): a pane's OSC title (emitted by xterm) becomes its header title.
  useEffect(() => {
    const handler = (event: Event) => {
      const { sessionId, title } = (event as CustomEvent<{ sessionId: string; title: string }>)
        .detail;
      if (!title) return;
      setBindings((current) => {
        const entry = Object.entries(current).find(
          ([, binding]) => binding.sessionId === sessionId,
        );
        if (!entry) return current;
        const [paneId, binding] = entry;
        return { ...current, [paneId]: { ...binding, title } };
      });
    };
    window.addEventListener("terminal-title", handler);
    return () => window.removeEventListener("terminal-title", handler);
  }, []);

  const persist = (next: LayoutNode, nextBindings = bindings) => {
    setLayout(next);
    if (activeId && activeId !== "preview") void ipc.saveLayout(activeId, next, nextBindings);
  };
  const output = (chunk: TerminalChunk) =>
    window.dispatchEvent(
      new CustomEvent(`terminal-output:${chunk.sessionId}`, {
        detail: { seq: chunk.seq, bytes: chunk.bytes },
      }),
    );
  const startProvider = async (
    paneId: string,
    providerId: string,
    targetLayout = layout,
    context = bindings[paneId],
    resume?: ResumeRef,
  ) => {
    try {
      const session = await ipc.createSession(
        {
          providerId,
          projectId: context?.projectId,
          worktreeId: context?.worktreeId,
          cols: 80,
          rows: 24,
        },
        output,
        resume,
      );
      const project = projects.find((entry) => entry.id === context?.projectId);
      const detail = project
        ? `${project.name}${project.branch ? ` · ${project.branch}` : ""}${
            context?.worktreeId ? " · wt" : ""
          }`
        : undefined;
      setBindings((current) => {
        const next = {
          ...current,
          [paneId]: {
            ...context,
            providerId,
            sessionId: session.sessionId,
            title: providerId,
            detail,
            color: providerColor(providerId),
            exited: false,
          },
        };
        if (targetLayout) persist(targetLayout, next);
        return next;
      });
      window.dispatchEvent(new CustomEvent("projects-refresh"));
    } catch (error) {
      setBindings((current) => ({
        ...current,
        [paneId]: {
          providerId,
          title: `${providerId}: ${String(error)}`,
          color: "var(--color-danger)",
        },
      }));
    }
  };

  const applyWorktree = (paneId: string, worktreeId?: string) => {
    const current = bindings[paneId] ?? {};
    if (current.sessionId) void ipc.closeSession(current.sessionId);
    const next = {
      ...bindings,
      [paneId]: { ...current, worktreeId, sessionId: undefined, exited: false },
    };
    setBindings(next);
    if (layout) persist(layout, next);
    if (current.providerId) void startProvider(paneId, current.providerId, layout, next[paneId]);
    setWorktreePicker(undefined);
  };

  return (
    <main className="flex h-full bg-app text-text">
      <SidebarFrame
        header={
          <>
            <span className="grid size-6 shrink-0 place-items-center rounded-control border border-accent-line bg-accent-background text-accent-strong shadow-glow">
              <TerminalSquare size={14} strokeWidth={2} />
            </span>
            <h1 className="min-w-0 flex-1 truncate text-title font-semibold tracking-tight text-text-strong">
              Terminal AI
            </h1>
            <Button
              variant="ghost"
              size="sm"
              icon
              onClick={refreshSidebar}
              title="Atualizar sidebar (Cmd/Ctrl+R) — mantém os terminais"
              aria-label="Atualizar sidebar"
            >
              <RefreshCw size={14} />
            </Button>
          </>
        }
        footer={
          <SidebarSection title="Uso" icon={<Gauge size={13} />}>
            <UsageCards />
          </SidebarSection>
        }
      >
        <SidebarSection
          title={showArchived ? "Arquivados" : "Projetos"}
          icon={<FolderGit2 size={13} />}
          count={projects.filter((project) => project.archived === showArchived).length}
          action={
            <>
              <Button
                variant={showArchived ? "accent" : "ghost"}
                size="sm"
                icon
                aria-pressed={showArchived}
                title={showArchived ? "Voltar aos projetos ativos" : "Ver projetos arquivados"}
                aria-label={showArchived ? "Voltar aos projetos ativos" : "Ver projetos arquivados"}
                onClick={() => setShowArchived((value) => !value)}
              >
                <Archive size={13} />
              </Button>
              {activeId && (
                <Button
                  variant="ghost"
                  size="sm"
                  icon
                  title={workspaceRoots[activeId] ?? "Escolher a pasta deste workspace"}
                  aria-label="Escolher a pasta deste workspace"
                  onClick={() => void chooseWorkspaceFolder(activeId)}
                >
                  <FolderOpen size={13} />
                </Button>
              )}
            </>
          }
        >
          <ProjectList
            workspaceId={activeId}
            archived={showArchived}
            selectedId={selectedProjectId}
            onProjects={setProjects}
            onSelect={(projectId) => {
              setSelectedProjectId(projectId);
              setSelectedWorktreeId(undefined);
            }}
          />
        </SidebarSection>
      </SidebarFrame>
      <section className="flex min-w-0 flex-1 flex-col">
        <WorkspaceTabs
          tabs={tabs}
          activeId={view === "workspace" ? activeId : undefined}
          onSelect={(id) => {
            setActiveId(id);
            setView("workspace");
          }}
          onAdd={() => void createWorkspace()}
          onRename={(id, title) => {
            setTabs((current) => current.map((tab) => (tab.id === id ? { ...tab, title } : tab)));
            void ipc.renameWorkspace(id, title);
          }}
          onClose={(id) => {
            void ipc.closeWorkspace(id);
            delete workspaceCache.current[id];
            setTabs((current) => current.filter((tab) => tab.id !== id));
            if (activeId === id) setActiveId(tabs.find((tab) => tab.id !== id)?.id);
          }}
          tools={
            <div className="flex items-center gap-1">
              {/* Five controls, one shape: 32px ghost icon buttons that light up in accent while
                  they are the active thing — the view you are in, or the menu you have open. */}
              <Tooltip label="Skills">
                <Button
                  variant={view === "skills" ? "accent" : "ghost"}
                  icon
                  aria-pressed={view === "skills"}
                  aria-label="Skills"
                  onClick={() => setView(view === "skills" ? "workspace" : "skills")}
                >
                  <Sparkles size={15} />
                </Button>
              </Tooltip>
              <Tooltip label="Memória">
                <Button
                  variant={view === "memory" ? "accent" : "ghost"}
                  icon
                  aria-pressed={view === "memory"}
                  aria-label="Memória"
                  onClick={() => setView(view === "memory" ? "workspace" : "memory")}
                >
                  <Brain size={15} />
                </Button>
              </Tooltip>
              <span className="mx-1 h-5 w-px shrink-0 bg-border" />
              <Presets
                layout={layout}
                bindings={bindings}
                projectId={selectedProjectId}
                onCreated={(workspaceId, title) => {
                  setTabs((current) => [...current, { id: workspaceId, title }]);
                  setActiveId(workspaceId);
                }}
              />
              <ProviderProfiles />
              <InvisibleModeBadge active={settings.invisibleMode} />
              <SettingsMenu settings={settings} onChange={setSettings} />
            </div>
          }
        />
        <div className="min-h-0 flex-1 overflow-hidden bg-app p-2.5">
          {view === "skills" ? (
            <SkillsView
              projectId={selectedProjectId}
              worktreeId={selectedWorktreeId}
              workspaceId={activeId}
              sessionId={focusedPaneId ? bindings[focusedPaneId]?.sessionId : undefined}
            />
          ) : view === "memory" ? (
            <MemoryView projectId={selectedProjectId} />
          ) : layout ? (
            <LayoutTree
              node={layout}
              bindings={bindings}
              defaultProjectId={selectedProjectId}
              defaultWorktreeId={selectedWorktreeId}
              workspaceId={activeId}
              maximizedPaneId={maximized}
              activePaneId={focusedPaneId}
              onFocus={setFocusedPaneId}
              onResize={(path, sizes) => persist(resizeSplit(layout, path, sizes))}
              onSplit={(paneId, direction) =>
                persist(splitPane(layout, paneId, direction, crypto.randomUUID()))
              }
              onClose={(paneId) => {
                const sessionId = bindings[paneId]?.sessionId;
                if (sessionId) void ipc.closeSession(sessionId);
                const next = closePane(layout, paneId);
                const nextBindings = { ...bindings };
                delete nextBindings[paneId];
                setBindings(nextBindings);
                if (next) persist(next, nextBindings);
              }}
              onMaximize={(paneId) =>
                setMaximized((current) => (current === paneId ? undefined : paneId))
              }
              onProvider={(paneId, providerId, projectId, worktreeId) => {
                const next = {
                  ...bindings,
                  [paneId]: {
                    ...bindings[paneId],
                    projectId: projectId ?? selectedProjectId,
                    worktreeId: worktreeId ?? selectedWorktreeId,
                  },
                };
                setBindings(next);
                void startProvider(paneId, providerId, layout, next[paneId]);
              }}
              onResumeProvider={(paneId) => {
                // "Resume {provider}" continues the pane's prior conversation in its own cwd
                // (claude --continue / --resume <id>) instead of opening a blank session (FR-030).
                const binding = bindings[paneId];
                if (binding?.providerId)
                  void startProvider(paneId, binding.providerId, layout, binding, {
                    kind: "continue",
                  });
              }}
              onResume={(paneId, historyId) => {
                void ipc.resumeSession(historyId, 80, 24, output).then((session) => {
                  const next = {
                    ...bindings,
                    [paneId]: {
                      ...bindings[paneId],
                      sessionId: session.sessionId,
                      title: session.resumed ? "Resumed session" : "Fresh session",
                    },
                  };
                  setBindings(next);
                  persist(layout, next);
                });
              }}
              onRestart={(paneId) => {
                const sessionId = bindings[paneId]?.sessionId;
                if (sessionId)
                  void ipc.restartSession(sessionId).then((result) =>
                    setBindings((current) => ({
                      ...current,
                      [paneId]: { ...current[paneId], sessionId: result.sessionId },
                    })),
                  );
              }}
              onExport={(paneId) => {
                const sessionId = bindings[paneId]?.sessionId;
                if (sessionId) void exportScrollback(sessionId, paneId);
              }}
              onDuplicate={(paneId) => {
                const newPaneId = crypto.randomUUID();
                const next = splitPane(layout, paneId, "horizontal", newPaneId);
                persist(next);
                const source = bindings[paneId];
                if (source?.providerId)
                  void startProvider(newPaneId, source.providerId, next, source);
              }}
              onChangeProvider={(paneId) => {
                const sessionId = bindings[paneId]?.sessionId;
                if (sessionId) void ipc.closeSession(sessionId);
                const next = {
                  ...bindings,
                  [paneId]: { ...bindings[paneId], providerId: undefined, sessionId: undefined },
                };
                setBindings(next);
                persist(layout, next);
              }}
              onChangeWorktree={(paneId) => setWorktreePicker({ paneId })}
              onRename={(paneId) => {
                const title = window.prompt("Pane title", bindings[paneId]?.title ?? "");
                if (title === null) return;
                const next = { ...bindings, [paneId]: { ...bindings[paneId], title } };
                setBindings(next);
                persist(layout, next);
              }}
            />
          ) : (
            <div className="hud-grid grid h-full place-items-center rounded-panel border border-border text-ui text-text-muted">
              Carregando workspace…
            </div>
          )}
        </div>
      </section>
      {worktreePicker && (
        <WorktreePickerModal
          projectId={bindings[worktreePicker.paneId]?.projectId}
          onPick={(worktreeId) => applyWorktree(worktreePicker.paneId, worktreeId)}
          onClose={() => setWorktreePicker(undefined)}
        />
      )}
      {toast && (
        <div
          role="alert"
          className={`fixed bottom-5 left-1/2 z-50 max-w-md -translate-x-1/2 rounded-control border bg-elevated px-3 py-2 text-ui shadow-popover ${
            toast.tone === "error"
              ? "border-danger/60 text-danger"
              : "border-accent-line text-text shadow-glow"
          }`}
        >
          {toast.text}
        </div>
      )}
    </main>
  );
}

function SidebarSection({
  title,
  icon,
  count,
  action,
  className = "",
  children,
}: {
  title: string;
  icon?: ReactNode;
  count?: number;
  action?: ReactNode;
  className?: string;
  children?: ReactNode;
}) {
  return (
    <section className={className}>
      <header className="mb-2 flex h-7 items-center gap-2">
        {icon && <span className="text-accent/60">{icon}</span>}
        <h2 className="flex-1 font-mono text-readout uppercase tracking-[0.2em] text-text-faint">
          {title}
        </h2>
        {count !== undefined && (
          <span className="font-mono text-readout tabular-nums text-text-faint">
            {String(count).padStart(2, "0")}
          </span>
        )}
        {action && <span className="flex shrink-0 items-center gap-0.5">{action}</span>}
      </header>
      {children ?? (
        <p className="rounded-control border border-dashed border-border px-3 py-4 text-center text-meta text-text-faint">
          Nada aqui ainda
        </p>
      )}
    </section>
  );
}
function providerColor(id: string) {
  return (
    {
      claude: "var(--color-accent)",
      codex: "var(--color-cyan)",
      opencode: "var(--color-provider-opencode)",
      shell: "var(--color-text-muted)",
    } as Record<string, string>
  )[id];
}
async function exportScrollback(sessionId: string, paneId: string) {
  const result = await ipc.getScrollback(sessionId);
  const bytes = Uint8Array.from(atob(result.data), (character) => character.charCodeAt(0));
  const link = document.createElement("a");
  link.href = URL.createObjectURL(new Blob([bytes], { type: "text/plain" }));
  link.download = `terminal-${paneId}.txt`;
  link.click();
  URL.revokeObjectURL(link.href);
}
function firstPaneId(node: LayoutNode): string {
  return node.type === "pane" ? node.paneId : firstPaneId(node.children[0]);
}
const focusDirections: Record<string, FocusDirection> = {
  focusLeft: "left",
  focusRight: "right",
  focusUp: "up",
  focusDown: "down",
};
function eventShortcut(event: KeyboardEvent) {
  const keys = [
    event.metaKey && "Meta",
    event.ctrlKey && "Control",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.key.length === 1 ? event.key.toUpperCase() : event.key,
  ].filter(Boolean);
  return keys.join("+");
}
