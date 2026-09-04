import { AlertTriangle, Brain, Loader2 } from "lucide-react";
import { describeKernel } from "../../stores/memory";
import { useKernelStatus } from "./useKernelStatus";

const TONE_CLASS: Record<string, string> = {
  ok: "text-accent border-border",
  idle: "text-neutral-400 border-border",
  warn: "text-amber-400 border-amber-900/60",
  bad: "text-rose-400 border-rose-900/60",
};

/** A one-line, non-blocking indicator. Losing the kernel is a banner, never a modal. */
export function KernelStatusChip() {
  const status = useKernelStatus();
  const { label, tone } = describeKernel(status);
  const Icon = tone === "idle" ? Loader2 : tone === "ok" ? Brain : AlertTriangle;

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-lg border px-2 py-1 text-xs ${TONE_CLASS[tone]}`}
      title={status?.guidance ?? status?.lastError ?? undefined}
    >
      <Icon className={`h-3.5 w-3.5 ${tone === "idle" ? "animate-spin" : ""}`} />
      {label}
      {status?.owned === false && status.state === "attached" ? (
        <span className="text-neutral-500">· externo</span>
      ) : null}
    </span>
  );
}

/**
 * The explanation shown when memory cannot be used. It states what is wrong and what to do, and it
 * never blocks the rest of the app — every other feature keeps working without a kernel.
 */
export function KernelBanner() {
  const status = useKernelStatus();
  if (!status) return null;

  // A server running a version this build was not tested against is worth saying out loud. Response
  // shapes are an external contract we observe, not one we own, so a mismatch is the most likely
  // explanation for anything odd that follows — better to name it than to let it surface as a
  // confusing parse error somewhere else.
  if (!status.versionMatchesPin && status.version) {
    return (
      <div className="rounded-lg border border-amber-900/60 bg-amber-950/20 p-3 text-xs text-amber-200">
        <p className="font-medium">Versão inesperada da memória</p>
        <p className="mt-1 text-amber-200/80">
          O servidor em uso é a versão {status.version}, diferente da que este app foi testado
          contra. Deve funcionar, mas se algo parecer estranho, é o primeiro lugar para olhar.
        </p>
      </div>
    );
  }

  if (status.state === "ready" || status.state === "attached") return null;
  const transient = status.state === "probing" || status.state === "starting";
  if (transient) return null;

  return (
    <div className="rounded-lg border border-amber-900/60 bg-amber-950/20 p-3 text-xs text-amber-200">
      <p className="font-medium">{describeKernel(status).label}</p>
      {status.guidance ? <p className="mt-1 text-amber-200/80">{status.guidance}</p> : null}
      {status.lastError ? (
        <p className="mt-1 font-mono text-[11px] text-amber-200/60">{status.lastError}</p>
      ) : null}
    </div>
  );
}
