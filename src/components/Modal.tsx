import { useEffect, type ReactNode } from "react";

/**
 * One dialog shell for every modal: scrim, panel, titled header and a footer
 * whose actions are real buttons. Escape and a scrim click both dismiss.
 */
export function Modal({
  title,
  description,
  onClose,
  footer,
  width = "sm",
  children,
}: {
  title: string;
  description?: string;
  onClose: () => void;
  footer?: ReactNode;
  width?: "xs" | "sm" | "lg";
  children: ReactNode;
}) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const max = { xs: "max-w-xs", sm: "max-w-md", lg: "max-w-2xl" }[width];
  return (
    <div
      className="fixed inset-0 z-100 grid place-items-center bg-black/60 p-8 backdrop-blur-[2px]"
      onMouseDown={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(event) => event.stopPropagation()}
        className={`flex max-h-[80vh] w-full ${max} flex-col overflow-hidden rounded-modal border border-border bg-elevated shadow-modal`}
      >
        <header className="shrink-0 border-b border-border-subtle px-4 py-3">
          <h2 className="text-title font-semibold text-text-strong">{title}</h2>
          {description && <p className="mt-0.5 text-meta text-text-muted">{description}</p>}
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">{children}</div>
        {footer && (
          <footer className="flex shrink-0 items-center justify-end gap-2 border-t border-border-subtle px-4 py-3">
            {footer}
          </footer>
        )}
      </div>
    </div>
  );
}
