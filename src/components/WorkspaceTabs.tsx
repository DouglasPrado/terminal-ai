import { Plus, X } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Button } from "./Button";

export interface WorkspaceTab {
  id: string;
  title: string;
}

/** Workspace switcher. The active tab is the only lit surface in the bar. */
export function WorkspaceTabs({
  tabs,
  activeId,
  onSelect,
  onAdd,
  onClose,
  onRename,
  tools,
}: {
  tabs: WorkspaceTab[];
  activeId?: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onClose: (id: string) => void;
  onRename: (id: string, title: string) => void;
  tools?: ReactNode;
}) {
  // Double-click to rename, the way tabs behave everywhere else — no extra chrome in a bar
  // whose job is to stay quiet.
  const [editingId, setEditingId] = useState<string>();
  const [draft, setDraft] = useState("");
  const commit = () => {
    if (editingId && draft.trim()) onRename(editingId, draft.trim());
    setEditingId(undefined);
  };
  return (
    <nav
      aria-label="Workspaces"
      className="scanlines relative flex h-11 shrink-0 items-center gap-2 border-b border-border bg-elevated px-3"
    >
      {/* The strip scrolls so neither the new-workspace button nor the tools can be pushed out
          of the window; tabs shrink to their floor first, so it rarely comes up. The button sits
          just outside the scrolling area so it stays glued after the last tab without ever
          scrolling away. */}
      <div className="flex min-w-0 items-center gap-2">
        <div className="flex min-w-0 items-center gap-1.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {tabs.map((tab) => {
            const active = tab.id === activeId;
            return (
              <div
                key={tab.id}
                /* Three columns — a spacer exactly the width of the close button, the label, then
                 the button itself — so the title is genuinely centred in the tab instead of
                 being nudged by padding chosen to look about right. */
                className={`group grid h-8 min-w-[96px] max-w-[200px] shrink grid-cols-[20px_minmax(0,1fr)_20px] items-center gap-1 rounded-control border px-1.5 transition-colors ${
                  active
                    ? "border-accent-line bg-accent-background text-accent-strong text-shadow-neon shadow-glow"
                    : "border-transparent text-text-muted hover:bg-raised hover:text-text"
                }`}
              >
                <span aria-hidden />
                {editingId === tab.id ? (
                  <input
                    autoFocus
                    value={draft}
                    aria-label={`Renomear ${tab.title}`}
                    onChange={(event) => setDraft(event.target.value)}
                    onBlur={commit}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") commit();
                      if (event.key === "Escape") setEditingId(undefined);
                    }}
                    className="min-w-0 bg-transparent text-center text-ui font-medium text-text-strong outline-none"
                  />
                ) : (
                  <button
                    type="button"
                    aria-current={active ? "page" : undefined}
                    title={`${tab.title} — duplo clique para renomear`}
                    onClick={() => onSelect(tab.id)}
                    onDoubleClick={() => {
                      setEditingId(tab.id);
                      setDraft(tab.title);
                    }}
                    className="min-w-0 truncate text-center text-ui font-medium"
                  >
                    {tab.title}
                  </button>
                )}
                <button
                  type="button"
                  aria-label={`Fechar ${tab.title}`}
                  title={`Fechar ${tab.title}`}
                  onClick={() => onClose(tab.id)}
                  className="grid size-5 place-items-center rounded-chip text-current opacity-0 transition-opacity hover:bg-white/10 hover:text-danger focus-visible:opacity-100 group-hover:opacity-60"
                >
                  <X size={12} strokeWidth={2.5} />
                </button>
              </div>
            );
          })}
        </div>
        <Button
          variant="ghost"
          icon
          aria-label="Novo workspace"
          title="Novo workspace"
          onClick={onAdd}
        >
          <Plus size={15} />
        </Button>
      </div>
      {tools && <div className="ml-auto flex shrink-0 items-center gap-1">{tools}</div>}
    </nav>
  );
}
