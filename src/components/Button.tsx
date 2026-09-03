import type { ButtonHTMLAttributes } from "react";

type Variant = "default" | "ghost" | "accent" | "danger";
type Size = "sm" | "md";

/**
 * The one control surface in the app. `default` is a real raised button — fill,
 * hairline, a 1px inner highlight along the top edge, and a press that sinks —
 * so a clickable thing never reads as loose text. `accent` is the only variant
 * that lights up: neon is reserved for the primary action and live state.
 */
export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  /** Square control holding a single icon; pair with `aria-label`. */
  icon?: boolean;
  block?: boolean;
}

const base =
  "inline-flex shrink-0 select-none items-center justify-center gap-1.5 rounded-control " +
  "text-ui font-medium leading-none transition-colors duration-150 " +
  "active:translate-y-px disabled:pointer-events-none disabled:opacity-40";

const variants: Record<Variant, string> = {
  default:
    "border border-border bg-raised text-text shadow-raised " +
    "hover:border-border-hover hover:bg-raised-hover hover:text-text-strong active:shadow-none",
  ghost:
    "border border-transparent text-text-muted " +
    "hover:bg-raised hover:text-text-strong active:bg-raised-hover",
  accent:
    "border border-accent-line bg-accent-background text-accent-strong text-shadow-neon shadow-glow " +
    "hover:bg-accent-background-hover active:translate-y-0",
  danger:
    "border border-transparent text-text-muted hover:bg-danger/15 hover:text-danger active:bg-danger/25",
};

const sizes: Record<Size, string> = { sm: "h-7 px-2.5", md: "h-8 px-3" };
const iconSizes: Record<Size, string> = { sm: "size-7", md: "size-8" };

export function Button({
  variant = "default",
  size = "md",
  icon = false,
  block = false,
  className = "",
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`${base} ${variants[variant]} ${icon ? iconSizes[size] : sizes[size]} ${
        block ? "w-full" : ""
      } ${className}`}
      {...props}
    />
  );
}
