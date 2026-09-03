import { useEffect, useState } from "react";
import { ArrowRightCircle, Clock } from "lucide-react";
import { ipc, type Handoff, type Scope } from "../../lib/ipc";
import { Button } from "../../components/Button";

/**
 * Continuity waiting for the next agent.
 *
 * Deliberately read-only apart from clearing stale entries. A handoff is consumed when the next
 * agent accepts it at session start — if this panel accepted one, the agent that was about to
 * receive that context would get nothing instead. Showing it is the useful thing; standing in the
 * middle of it is not.
 */
export function HandoffList({ scope }: { scope: Scope }) {
  const [handoffs, setHandoffs] = useState<Handoff[]>([]);
  const [busy, setBusy] = useState(false);

  const reload = () => {
    void ipc
      .listMemoryHandoffs(scope, "open")
      .then((result) => setHandoffs(result.handoffs))
      .catch(() => setHandoffs([]));
  };

  useEffect(reload, [scope]);

  if (handoffs.length === 0) return null;

  const expire = () => {
    setBusy(true);
    void ipc
      .expireMemoryHandoffs(scope, 7)
      .then(reload)
      .finally(() => setBusy(false));
  };

  return (
    <div className="rounded-lg border border-border bg-panel p-2 text-xs">
      <p className="flex items-center gap-1.5 font-medium text-neutral-200">
        <ArrowRightCircle size={13} className="text-accent" />
        {handoffs.length} continuação pendente{handoffs.length > 1 ? "s" : ""}
      </p>
      <p className="mt-1 text-neutral-500">
        O próximo agente que abrir neste projeto recebe isto automaticamente.
      </p>
      <ul className="mt-2 space-y-2">
        {handoffs.map((handoff) => (
          <li key={handoff.id} className="border-l border-border pl-2">
            <p className="text-neutral-300">
              {handoff.agent} · {handoff.summary || "sem resumo"}
            </p>
            {handoff.openQuestions.length > 0 ? (
              <p className="mt-0.5 text-neutral-500">
                Em aberto: {handoff.openQuestions.join("; ")}
              </p>
            ) : null}
            {handoff.nextSteps.length > 0 ? (
              <p className="mt-0.5 text-neutral-500">
                Próximos passos: {handoff.nextSteps.join("; ")}
              </p>
            ) : null}
          </li>
        ))}
      </ul>
      <Button className="mt-2" onClick={expire} disabled={busy}>
        <Clock size={13} /> Limpar as com mais de 7 dias
      </Button>
    </div>
  );
}
