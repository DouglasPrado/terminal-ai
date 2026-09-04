import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { PaneHeader } from "../../components/PaneHeader";
import type { LayoutNode as Node } from "../../lib/ipc";
import { PaneActions } from "../terminals/PaneActions";
import { ProviderPicker } from "../terminals/ProviderPicker";
import { TerminalPane } from "../terminals/TerminalPane";

export interface PaneBinding {
  providerId?: string;
  sessionId?: string;
  title?: string;
  color?: string;
  detail?: string;
  projectId?: string;
  worktreeId?: string;
  exited?: boolean;
}
export function LayoutTree({
  node,
  bindings,
  maximizedPaneId,
  activePaneId,
  defaultProjectId,
  defaultWorktreeId,
  workspaceId,
  onResize,
  onSplit,
  onClose,
  onMaximize,
  onProvider,
  onResumeProvider,
  onRestart,
  onExport,
  onDuplicate,
  onChangeProvider,
  onChangeWorktree,
  onRename,
  onResume,
  onFocus,
}: {
  node: Node;
  bindings: Record<string, PaneBinding>;
  maximizedPaneId?: string;
  activePaneId?: string;
  /** What the sidebar has selected, used to seed a pane that has no binding yet. */
  defaultProjectId?: string;
  defaultWorktreeId?: string;
  /** The active workspace, so a pane lists only the projects that workspace watches. */
  workspaceId?: string;
  onResize: (path: number[], sizes: number[]) => void;
  onSplit: (paneId: string, direction: "horizontal" | "vertical") => void;
  onClose: (paneId: string) => void;
  onMaximize: (paneId: string) => void;
  onProvider: (paneId: string, providerId: string, projectId?: string, worktreeId?: string) => void;
  onResumeProvider: (paneId: string) => void;
  onResume: (paneId: string, historyId: string) => void;
  onRestart: (paneId: string) => void;
  onExport: (paneId: string) => void;
  onDuplicate: (paneId: string) => void;
  onChangeProvider: (paneId: string) => void;
  onChangeWorktree: (paneId: string) => void;
  onRename: (paneId: string) => void;
  onFocus: (paneId: string) => void;
}) {
  const actions: Actions = {
    defaultProjectId,
    defaultWorktreeId,
    workspaceId,
    onSplit,
    onClose,
    onMaximize,
    onProvider,
    onResumeProvider,
    onRestart,
    onExport,
    onDuplicate,
    onChangeProvider,
    onChangeWorktree,
    onRename,
    onResume,
    onFocus,
  };
  if (maximizedPaneId) {
    const target = findPane(node, maximizedPaneId);
    if (target)
      return (
        <Leaf
          paneId={target.paneId}
          binding={bindings[target.paneId]}
          active={activePaneId === target.paneId}
          actions={actions}
        />
      );
  }
  return (
    <TreeNode
      node={node}
      path={[]}
      bindings={bindings}
      activePaneId={activePaneId}
      onResize={onResize}
      actions={actions}
    />
  );
}
type Actions = {
  defaultProjectId?: string;
  defaultWorktreeId?: string;
  workspaceId?: string;
  onSplit: (id: string, d: "horizontal" | "vertical") => void;
  onClose: (id: string) => void;
  onMaximize: (id: string) => void;
  onProvider: (id: string, p: string, projectId?: string, worktreeId?: string) => void;
  onResumeProvider: (id: string) => void;
  onResume: (id: string, historyId: string) => void;
  onRestart: (id: string) => void;
  onExport: (id: string) => void;
  onDuplicate: (id: string) => void;
  onChangeProvider: (id: string) => void;
  onChangeWorktree: (id: string) => void;
  onRename: (id: string) => void;
  onFocus: (id: string) => void;
};
function TreeNode({
  node,
  path,
  bindings,
  activePaneId,
  onResize,
  actions,
}: {
  node: Node;
  path: number[];
  bindings: Record<string, PaneBinding>;
  activePaneId?: string;
  onResize: (path: number[], sizes: number[]) => void;
  actions: Actions;
}) {
  if (node.type === "pane")
    return (
      <Leaf
        paneId={node.paneId}
        binding={bindings[node.paneId]}
        active={activePaneId === node.paneId}
        actions={actions}
      />
    );
  return (
    <PanelGroup
      direction={node.direction}
      className="h-full"
      onLayout={(sizes) => onResize(path, sizes)}
    >
      {node.children.map((child, index) => (
        <span
          key={child.type === "pane" ? child.paneId : `split-${path.join("-")}-${index}`}
          className="contents"
        >
          <Panel defaultSize={node.sizes[index]} minSize={8}>
            <TreeNode
              node={child}
              path={[...path, index]}
              bindings={bindings}
              activePaneId={activePaneId}
              onResize={onResize}
              actions={actions}
            />
          </Panel>
          {index < node.children.length - 1 && (
            <PanelResizeHandle
              className="relative z-10 bg-border-subtle transition-colors hover:bg-accent-line data-[resize-handle-active]:bg-accent data-[resize-handle-active]:shadow-glow"
              style={node.direction === "horizontal" ? { width: 2 } : { height: 2 }}
            />
          )}
        </span>
      ))}
    </PanelGroup>
  );
}
function Leaf({
  paneId,
  binding = {},
  active,
  actions,
}: {
  paneId: string;
  binding?: PaneBinding;
  active: boolean;
  actions: Actions;
}) {
  return (
    <article
      data-active={active}
      onMouseDownCapture={() => actions.onFocus(paneId)}
      className={`group/pane flex h-full min-h-0 flex-col overflow-hidden rounded-panel border bg-app transition-shadow ${
        active ? "border-accent-line shadow-glow" : "border-border"
      }`}
    >
      <PaneHeader
        title={binding.title ?? providerLabel(binding.providerId) ?? "Painel vazio"}
        detail={binding.detail}
        providerId={binding.providerId}
        color={binding.color}
        exited={binding.exited}
        active={active}
        actions={
          <PaneActions
            onSplitRight={() => actions.onSplit(paneId, "horizontal")}
            onSplitDown={() => actions.onSplit(paneId, "vertical")}
            onMaximize={() => actions.onMaximize(paneId)}
            onRestart={() => actions.onRestart(paneId)}
            onClose={() => actions.onClose(paneId)}
            onExport={() => actions.onExport(paneId)}
            onDuplicate={() => actions.onDuplicate(paneId)}
            onChangeProvider={() => actions.onChangeProvider(paneId)}
            onChangeWorktree={() => actions.onChangeWorktree(paneId)}
            onRename={() => actions.onRename(paneId)}
          />
        }
      />
      <div className="min-h-0 flex-1">
        {binding.sessionId ? (
          <TerminalPane sessionId={binding.sessionId} active={active} />
        ) : (
          <ProviderPicker
            defaultProvider={binding.providerId}
            defaultProjectId={binding.projectId ?? actions.defaultProjectId}
            defaultWorktreeId={binding.worktreeId ?? actions.defaultWorktreeId}
            workspaceId={actions.workspaceId}
            onSelect={(provider, projectId, worktreeId) =>
              actions.onProvider(paneId, provider, projectId, worktreeId)
            }
            onResumeProvider={() => actions.onResumeProvider(paneId)}
            onResume={(historyId) => actions.onResume(paneId, historyId)}
          />
        )}
      </div>
    </article>
  );
}
function providerLabel(id?: string) {
  if (!id) return undefined;
  return { claude: "Claude", codex: "Codex", opencode: "OpenCode", shell: "Shell" }[id] ?? id;
}
function findPane(node: Node, paneId: string): Extract<Node, { type: "pane" }> | undefined {
  if (node.type === "pane") return node.paneId === paneId ? node : undefined;
  for (const child of node.children) {
    const found = findPane(child, paneId);
    if (found) return found;
  }
  return undefined;
}
