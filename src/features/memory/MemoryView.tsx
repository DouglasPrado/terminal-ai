import { Brain } from "lucide-react";
import { MemoryPanel } from "./MemoryPanel";

/** Full-page Memory listing shown from the fixed "Memória" tab. */
export function MemoryView({ projectId }: { projectId?: string }) {
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="mb-3 flex items-baseline gap-2.5">
        <span className="translate-y-0.5 text-accent">
          <Brain size={15} />
        </span>
        <h2 className="text-heading font-semibold tracking-tight text-text-strong">Memória</h2>
        <p className="min-w-0 truncate text-meta text-text-faint">
          fatos e decisões escopados, com busca
        </p>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-panel border border-border bg-panel p-3.5">
        <MemoryPanel projectId={projectId} />
      </div>
    </div>
  );
}
