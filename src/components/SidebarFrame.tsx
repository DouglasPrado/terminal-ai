import type { PropsWithChildren, ReactNode } from "react";

/**
 * The sidebar: a fixed 44px brand row that lines up with the workspace tab bar
 * across the seam, a scrolling body, and a pinned footer. The two top rows
 * sharing a baseline is what makes the window read as one frame rather than two
 * panels; the footer stays put so the usage readouts never scroll out of sight.
 */
export function SidebarFrame({
  header,
  footer,
  children,
}: PropsWithChildren<{ header: ReactNode; footer?: ReactNode }>) {
  return (
    <aside className="scanlines relative flex h-full w-[288px] shrink-0 flex-col border-r border-border bg-elevated">
      <div className="flex h-11 shrink-0 items-center gap-2.5 border-b border-border px-3">
        {header}
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-3 py-3">{children}</div>
      {footer && (
        <div className="shrink-0 border-t border-border bg-elevated px-3 py-3">{footer}</div>
      )}
    </aside>
  );
}
