import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";

const MenuContext = createContext<() => void>(() => {});

/**
 * One popover for every dropdown in the app: a real button trigger, a panel
 * that closes on Escape or an outside click, and items that all share a hit
 * area. The wrapper carries `data-open` so a parent can keep hover-revealed
 * chrome visible while the menu is up.
 */
export function Menu({
  label,
  icon,
  align = "end",
  width = 200,
  size = "md",
  title,
  children,
}: {
  label?: string;
  icon?: ReactNode;
  align?: "start" | "end";
  width?: number;
  size?: "sm" | "md";
  title?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const wrapper = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (!wrapper.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <div ref={wrapper} className="relative" data-open={open || undefined}>
      <Tooltip label={title ?? label ?? ""}>
        <Button
          variant={open ? "accent" : "ghost"}
          size={size}
          icon={!label}
          aria-label={title ?? label}
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {icon}
          {label}
        </Button>
      </Tooltip>
      {open && (
        <div
          role="menu"
          style={{ width }}
          className={`absolute top-[calc(100%+4px)] z-50 rounded-panel border border-border bg-elevated p-1 shadow-popover ${
            align === "end" ? "right-0" : "left-0"
          }`}
        >
          <MenuContext.Provider value={() => setOpen(false)}>{children}</MenuContext.Provider>
        </div>
      )}
    </div>
  );
}

/** A menu row. Closes the menu after running `onClick`. */
export function MenuItem({
  children,
  onClick,
  disabled,
  tone = "default",
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  tone?: "default" | "danger";
}) {
  const close = useContext(MenuContext);
  return (
    <button
      role="menuitem"
      type="button"
      disabled={disabled}
      onClick={() => {
        onClick();
        close();
      }}
      className={`flex h-8 w-full items-center gap-2 rounded-control px-2.5 text-left text-ui transition-colors disabled:pointer-events-none disabled:opacity-40 ${
        tone === "danger"
          ? "text-text-muted hover:bg-danger/12 hover:text-danger"
          : "text-text hover:bg-raised hover:text-text-strong"
      }`}
    >
      {children}
    </button>
  );
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <p className="px-2 pb-1 pt-1.5 text-meta text-text-faint">{children}</p>;
}

export function MenuSeparator() {
  return <hr className="my-1 border-0 border-t border-border-subtle" />;
}
