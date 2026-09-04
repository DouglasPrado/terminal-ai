import { Sparkles } from "lucide-react";
import { SkillsPanel } from "./SkillsPanel";

/** Full-page Skills listing shown from the fixed "Skills" tab. */
export function SkillsView(props: {
  projectId?: string;
  worktreeId?: string;
  workspaceId?: string;
  sessionId?: string;
}) {
  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="mb-3 flex items-baseline gap-2.5">
        <span className="translate-y-0.5 text-accent">
          <Sparkles size={15} />
        </span>
        <h2 className="text-heading font-semibold tracking-tight text-text-strong">Skills</h2>
        <p className="min-w-0 truncate text-meta text-text-faint">
          bibliotecas compartilhadas por escopo
        </p>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-panel border border-border bg-panel p-3.5">
        <SkillsPanel {...props} />
      </div>
    </div>
  );
}
