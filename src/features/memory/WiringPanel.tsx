import { useEffect, useState } from "react";
import { AlertTriangle, Link2, Unlink } from "lucide-react";
import {
  ipc,
  type Scope,
  type WiringBinding,
  type WiringKind,
  type WiringPlan,
} from "../../lib/ipc";
import { Button } from "../../components/Button";

const AGENTS = [
  { id: "claude-code", label: "Claude Code" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
] as const;

/**
 * Connects an agent to the memory kernel, behind a preview the user actually reads.
 *
 * Nothing is written until "Aplicar". The diff comes from the target file itself, and for capture
 * the exact list of lifecycle events is shown — consenting to the word "capture" would mean
 * nothing, consenting to a list of events means something.
 */
export function WiringPanel({ projectId }: { projectId?: string }) {
  const [bindings, setBindings] = useState<WiringBinding[]>([]);
  const [unavailable, setUnavailable] = useState<Array<[string, string]>>([]);
  const [plans, setPlans] = useState<WiringPlan[]>();
  const [agent, setAgent] = useState<string>("claude-code");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const scope: Scope = { level: "project", refId: projectId };

  const reload = () =>
    void ipc.listMemoryWiring().then((result) => {
      setBindings(result.bindings);
      setUnavailable(result.captureUnavailable);
    });

  useEffect(reload, []);

  if (!projectId) {
    return (
      <p className="text-xs text-neutral-500">
        Selecione um projeto para conectar os agentes à memória.
      </p>
    );
  }

  const kindsFor = (id: string): WiringKind[] =>
    unavailable.some(([a]) => a === id) ? ["mcp"] : ["mcp", "hooks"];

  const preview = (id: string) => {
    setAgent(id);
    setError(undefined);
    setBusy(true);
    void ipc
      .previewMemoryWiring(id, scope, kindsFor(id))
      .then((result) => setPlans(result.plans))
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false));
  };

  const apply = () => {
    setBusy(true);
    void ipc
      .applyMemoryWiring(agent, scope, kindsFor(agent))
      .then(() => {
        setPlans(undefined);
        reload();
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false));
  };

  const remove = (id: string) => {
    setBusy(true);
    void ipc
      .removeMemoryWiring(id, scope)
      .then(reload)
      .catch((e: Error) => setError(e.message))
      .finally(() => setBusy(false));
  };

  const blocked = plans?.some((plan) => plan.warnings.length > 0) ?? false;

  return (
    <div className="space-y-2 text-xs">
      <p className="font-medium text-neutral-200">Agentes conectados à memória</p>

      {AGENTS.map((entry) => {
        const bound = bindings.filter((b) => b.agent === entry.id && b.scopeRefId === projectId);
        const stale = bound.some((b) => b.status === "stale");
        const reason = unavailable.find(([a]) => a === entry.id)?.[1];
        return (
          <div key={entry.id} className="rounded-lg border border-border p-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-neutral-200">{entry.label}</span>
              {bound.length > 0 ? (
                <Button onClick={() => remove(entry.id)} disabled={busy}>
                  <Unlink size={13} /> Desconectar
                </Button>
              ) : (
                <Button onClick={() => preview(entry.id)} disabled={busy}>
                  <Link2 size={13} /> Conectar
                </Button>
              )}
            </div>
            {stale ? (
              <p className="mt-1 flex items-center gap-1 text-amber-300">
                <AlertTriangle size={12} /> A configuração aponta para um binário que mudou de
                lugar. Reconecte para consertar.
              </p>
            ) : null}
            {reason ? <p className="mt-1 text-neutral-500">{reason}</p> : null}
          </div>
        );
      })}

      {error ? (
        <p className="rounded-lg border border-rose-900/60 bg-rose-950/20 p-2 text-rose-300">
          {error}
        </p>
      ) : null}

      {plans ? (
        <div className="rounded-lg border border-border bg-panel p-2">
          <p className="font-medium text-neutral-200">O que vai mudar</p>
          {plans.map((plan) => (
            <div key={plan.kind} className="mt-2">
              <p className="text-neutral-400">
                {plan.kind === "mcp" ? "Acesso à memória" : "Captura de sessão"}
                {plan.path ? ` — ${plan.path}` : ""}
              </p>
              {plan.captureEvents.length > 0 ? (
                <p className="mt-1 text-neutral-500">
                  Seriam registrados: {plan.captureEvents.join(", ")}. Os prompts não.
                </p>
              ) : null}
              {plan.conflict ? <p className="mt-1 text-amber-300">{plan.conflict}</p> : null}
              {plan.warnings.map((warning) => (
                <p key={warning} className="mt-1 text-amber-300">
                  {warning}
                </p>
              ))}
              <pre className="mt-1 max-h-40 overflow-auto rounded bg-black/30 p-2 font-mono text-[11px] text-neutral-300">
                {plan.diff}
              </pre>
            </div>
          ))}
          <div className="mt-2 flex gap-1.5">
            <Button onClick={apply} disabled={busy || blocked}>
              Aplicar
            </Button>
            <Button onClick={() => setPlans(undefined)} disabled={busy}>
              Cancelar
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
