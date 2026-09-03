import { ChevronDown } from "lucide-react";
import type {
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

const control =
  "w-full rounded-control border border-border bg-app px-2 text-ui text-text transition-colors " +
  "placeholder:text-text-faint hover:border-border-hover focus:border-accent-line " +
  "disabled:pointer-events-none disabled:opacity-40";

/** Label + control, so every form row shares one label rhythm. */
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-meta text-text-muted">{label}</span>
      {children}
    </label>
  );
}

/** Native select with the system arrow replaced, so it matches the buttons. */
export function Select({
  className = "",
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="relative block">
      <select {...props} className={`${control} h-8 appearance-none pr-7 ${className}`}>
        {children}
      </select>
      <ChevronDown
        size={14}
        aria-hidden
        className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-text-faint"
      />
    </span>
  );
}

export function TextInput({ className = "", ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={`${control} h-8 ${className}`} />;
}

export function TextArea({
  className = "",
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      {...props}
      className={`${control} resize-y py-1.5 font-mono leading-5 ${className}`}
    />
  );
}
