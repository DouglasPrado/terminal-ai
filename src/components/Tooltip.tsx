import type { ReactNode } from "react";

/**
 * A styled tooltip for controls that carry no visible label. Native `title` works but appears
 * after a long delay and in the OS chrome, which reads as a different product; this one uses the
 * app's own tokens and shows quickly. Purely presentational — the accessible name still comes
 * from the control's own `aria-label`, so the tooltip is `aria-hidden`.
 */
export function Tooltip({
  label,
  side = "bottom",
  children,
}: {
  label: string;
  side?: "bottom" | "top";
  children: ReactNode;
}) {
  return (
    <span className="group/tooltip relative inline-flex">
      {children}
      <span
        role="tooltip"
        aria-hidden
        className={`pointer-events-none absolute left-1/2 z-50 -translate-x-1/2 whitespace-nowrap rounded-chip border border-border bg-elevated px-2 py-1 text-meta text-text opacity-0 shadow-popover transition-opacity delay-200 duration-100 group-hover/tooltip:opacity-100 group-focus-within/tooltip:opacity-100 ${
          side === "bottom" ? "top-[calc(100%+6px)]" : "bottom-[calc(100%+6px)]"
        }`}
      >
        {label}
      </span>
    </span>
  );
}
