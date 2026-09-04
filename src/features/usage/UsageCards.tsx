import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { ipc, type UsageCard, type UsageSnapshot } from "../../lib/ipc";
import { useUsageStore } from "../../stores/usage";
import { ProviderIcon } from "../../lib/providers";

const order = ["claude", "codex", "opencode"];
// Below this container width the cards collapse to a single line (FR-019 / AS3.4).
const COMPACT_WIDTH = 200;

export function UsageCards() {
  const snapshot = useUsageStore((state) => state.snapshot);
  const setSnapshot = useUsageStore((state) => state.setSnapshot);
  const containerRef = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(false);
  const [busy, setBusy] = useState<Record<string, boolean>>({});

  // A user click is an explicit refresh (bounded by the ~60s cache, not the 300s poller floor).
  // Always re-read the snapshot afterwards so even a throttled click (scheduled:false, no event)
  // still shows the freshest cached values with feedback instead of appearing inert (FR-018).
  const refresh = useCallback(
    (id: string) => {
      setBusy((prev) => ({ ...prev, [id]: true }));
      void ipc
        .refreshUsage(id)
        .then(() => ipc.getUsage())
        .then((value) => setSnapshot(value))
        .catch(() => {})
        .finally(() => setBusy((prev) => ({ ...prev, [id]: false })));
    },
    [setSnapshot],
  );

  useEffect(() => {
    let disposed = false;
    void ipc.getUsage().then((value) => !disposed && setSnapshot(value));
    void ipc
      .refreshUsage()
      .then(() => ipc.getUsage())
      .then((value) => !disposed && setSnapshot(value))
      .catch(() => {});
    const unlisten = listen<UsageSnapshot>("usage-updated", ({ payload }) => {
      if (!disposed) setSnapshot(payload);
    });
    // In-place sidebar refresh (FR-032): re-poll usage without a page reload.
    const onSidebarRefresh = () =>
      void ipc
        .refreshUsage()
        .then(() => ipc.getUsage())
        .then((value) => !disposed && setSnapshot(value))
        .catch(() => {});
    window.addEventListener("sidebar-refresh", onSidebarRefresh);
    return () => {
      disposed = true;
      window.removeEventListener("sidebar-refresh", onSidebarRefresh);
      void unlisten.then((remove) => remove());
    };
  }, [setSnapshot]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setCompact(entry.contentRect.width < COMPACT_WIDTH);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={containerRef} className="space-y-1.5">
      {order.map((id) => (
        <ProviderCard
          key={id}
          id={id}
          card={snapshot?.providers[id]}
          offline={snapshot?.offline}
          compact={compact}
          busy={busy[id]}
          onRefresh={refresh}
        />
      ))}
    </div>
  );
}

function ProviderCard({
  id,
  card,
  offline,
  compact,
  busy,
  onRefresh,
}: {
  id: string;
  card?: UsageCard;
  offline?: boolean;
  compact?: boolean;
  busy?: boolean;
  onRefresh: (id: string) => void;
}) {
  const label = card?.label ?? { claude: "Claude", codex: "Codex", opencode: "OpenCode" }[id] ?? id;
  const state = authState(card, offline);
  const primary = card?.lines[0];
  const accent =
    {
      claude: "var(--color-accent)",
      codex: "var(--color-cyan)",
      opencode: "var(--color-provider-opencode)",
    }[id] ?? "var(--color-text-muted)";
  if (compact) {
    return (
      <button
        type="button"
        onClick={() => onRefresh(id)}
        disabled={busy}
        aria-busy={busy}
        className={`flex h-7 w-full items-center justify-between gap-2 rounded-control border border-border bg-panel px-2 text-left transition-colors hover:border-border-hover hover:bg-raised ${busy ? "animate-pulse" : ""}`}
        title={`${label} — atualizar uso`}
      >
        <span className="flex min-w-0 items-center gap-2">
          <span className="shrink-0" style={{ color: accent }}>
            <ProviderIcon id={id} size={13} />
          </span>
          <strong className="truncate text-meta font-medium text-text">{label}</strong>
        </span>
        {state ? (
          <span
            title={state.hint}
            className={`whitespace-nowrap font-mono text-readout ${state.tone === "danger" ? "text-danger" : "text-warning"}`}
          >
            {state.text}
          </span>
        ) : primary ? (
          <span className="whitespace-nowrap font-mono text-readout text-text-faint">
            <b className="font-medium text-text">{primary.value}</b>
          </span>
        ) : (
          <span className="font-mono text-readout text-text-faint">—</span>
        )}
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={() => onRefresh(id)}
      disabled={busy}
      aria-busy={busy}
      className={`w-full rounded-control border border-border bg-panel px-2.5 py-2 text-left transition-colors hover:border-border-hover hover:bg-raised ${busy ? "animate-pulse" : ""}`}
      title="Atualizar uso"
    >
      <span className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-2">
          <span className="shrink-0" style={{ color: accent }}>
            <ProviderIcon id={id} size={14} />
          </span>
          <strong className="truncate text-ui font-medium text-text">{label}</strong>
        </span>
        {state && (
          <span
            title={state.hint}
            className={`font-mono text-readout uppercase tracking-wider ${state.tone === "danger" ? "text-danger" : "text-warning"}`}
          >
            {state.text}
          </span>
        )}
      </span>
      {typeof primary?.pct === "number" && (
        <span className="mt-2 block h-[3px] overflow-hidden rounded-full bg-white/[0.05]">
          <span
            className="block h-full rounded-full transition-[width] duration-500"
            style={{
              width: `${Math.min(100, Math.max(0, primary.pct))}%`,
              backgroundColor: accent,
              boxShadow: `0 0 8px -1px ${accent}`,
            }}
          />
        </span>
      )}
      {card?.lines.length ? (
        <span className="mt-1.5 flex flex-wrap gap-x-2.5 gap-y-1 font-mono text-readout text-text-faint">
          {card.lines.map((line) => (
            <span key={line.label} className="whitespace-nowrap">
              {line.label} <b className="font-medium text-text">{line.value}</b>
              {line.resetsAt && <span title={line.resetsAt}> · {resetLabel(line.resetsAt)}</span>}
            </span>
          ))}
        </span>
      ) : (
        <span className="mt-1.5 block font-mono text-readout text-text-faint">
          sem leitura disponível
        </span>
      )}
    </button>
  );
}

/**
 * Four distinct situations that used to collapse into one "reautentique":
 * `rejected` genuinely needs a new login, `expired` only needs the CLI run once, and a stale
 * or offline read is not a credential problem at all.
 */
function authState(card: UsageCard | undefined, offline?: boolean) {
  if (card?.auth === "rejected")
    return {
      text: "reautentique",
      tone: "danger" as const,
      hint: "A credencial foi recusada ou não existe. Faça login novamente na CLI.",
    };
  if (card?.auth === "expired")
    return {
      text: "token expirou",
      tone: "warning" as const,
      hint: "O token salvo passou da validade. Rode a CLI uma vez que ela renova sozinha.",
    };
  if (card?.stale || offline)
    return {
      text: "último valor",
      tone: "warning" as const,
      hint: "Não foi possível ler agora; mostrando a última leitura conhecida.",
    };
  return undefined;
}

function resetLabel(value: string) {
  const epoch = /^\d+(\.\d+)?$/.test(value) ? Number(value) * 1000 : Date.parse(value);
  if (!Number.isFinite(epoch)) return "reset pendente";
  const minutes = Math.max(0, Math.round((epoch - Date.now()) / 60_000));
  if (minutes < 60) return `${minutes}min`;
  const hours = Math.round(minutes / 60);
  return hours < 48 ? `${hours}h` : `${Math.round(hours / 24)}d`;
}
