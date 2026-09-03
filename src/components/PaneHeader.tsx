import type { ReactNode } from "react";
import { ProviderIcon } from "../lib/providers";

/**
 * A pane's title bar. Provider identity is carried by the brand mark and its
 * tint; the focused pane is the one that lights up, with a neon hairline along
 * the seam where the chrome meets the terminal.
 */
export function PaneHeader({
  title,
  detail,
  providerId,
  color,
  exited,
  active,
  actions,
}: {
  title: string;
  detail?: string;
  providerId?: string;
  color?: string;
  exited?: boolean;
  active?: boolean;
  actions?: ReactNode;
}) {
  const tint = exited ? "var(--color-text-faint)" : (color ?? "var(--color-text-muted)");
  return (
    <header
      className={`scanlines relative flex h-9 shrink-0 items-center gap-2 border-b px-2.5 transition-colors ${
        active
          ? "border-accent-line bg-raised shadow-[0_1px_10px_-2px_rgb(232_121_249/0.45)]"
          : "border-border-subtle bg-panel"
      }`}
    >
      <span className="shrink-0" style={{ color: tint }}>
        <ProviderIcon id={providerId ?? "shell"} size={15} />
      </span>
      <span className="flex min-w-0 flex-1 items-baseline gap-2">
        <strong className="truncate text-title font-medium text-text-strong">{title}</strong>
        {detail && <span className="truncate font-mono text-meta text-text-faint">{detail}</span>}
      </span>
      {exited && (
        <span className="shrink-0 rounded-chip border border-border px-1.5 py-px text-meta text-text-faint">
          encerrado
        </span>
      )}
      {actions}
    </header>
  );
}
