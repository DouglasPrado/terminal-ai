import { EyeOff } from "lucide-react";

/**
 * The only place the invisible mode can announce itself.
 *
 * While it is on there is no Dock icon and no menu bar, so the window is the sole surface left to
 * say so — and a user who believes the mode is off when it is on has lost their app (FR-010).
 */
export function InvisibleModeBadge({ active }: { active: boolean }) {
  if (!active) return null;
  return (
    <span
      data-testid="invisible-mode-indicator"
      title="O app está oculto de compartilhamento de tela, da Dock e do Cmd+Tab"
      className="flex shrink-0 items-center gap-1.5 rounded-chip border border-accent/40 bg-accent/12 px-2 py-0.5 text-meta text-accent"
    >
      <EyeOff size={12} /> Invisível
    </span>
  );
}
