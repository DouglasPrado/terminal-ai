import { useState } from "react";
import { Sparkles } from "lucide-react";
import { ipc } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { useKernelStatus } from "./useKernelStatus";
import { useMemoryStore } from "../../stores/memory";

/**
 * The hybrid-search opt-in.
 *
 * Off by default, and the size is stated before it is turned on. The kernel fetches a ~87 MB local
 * embedding model in the background on first start unless it is told not to, and reaching the
 * network on the user's behalf without saying so is exactly what FR-062 forbids. Full-text, entity
 * and graph ranking already work without it — this buys better ranking, not working search.
 */
export function MemorySettings() {
  const status = useKernelStatus();
  const refresh = useMemoryStore((s) => s.refresh);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const enabled = status?.hybridSearch ?? false;

  const set = (hybridSearch: boolean) => {
    setBusy(true);
    void ipc
      .setMemoryKernelSettings({ hybridSearch })
      .then(() => refresh())
      .finally(() => {
        setBusy(false);
        setConfirming(false);
      });
  };

  return (
    <div className="rounded-lg border border-border p-2 text-xs">
      <p className="flex items-center gap-1.5 font-medium text-neutral-200">
        <Sparkles size={13} className="text-accent" />
        Busca híbrida {enabled ? "ligada" : "desligada"}
      </p>
      {enabled ? (
        <>
          <p className="mt-1 text-neutral-400">
            A memória usa embeddings locais além de busca textual, entidades e grafo.
          </p>
          <Button className="mt-2" onClick={() => set(false)} disabled={busy}>
            Desligar
          </Button>
        </>
      ) : confirming ? (
        <>
          <p className="mt-1 text-amber-200">
            Ligar isso baixa um modelo local de <strong>~87 MB</strong> na primeira vez. Nada é
            enviado para fora da sua máquina, mas é um download.
          </p>
          <div className="mt-2 flex gap-1.5">
            <Button onClick={() => set(true)} disabled={busy}>
              Baixar e ligar
            </Button>
            <Button onClick={() => setConfirming(false)} disabled={busy}>
              Cancelar
            </Button>
          </div>
        </>
      ) : (
        <>
          <p className="mt-1 text-neutral-400">
            A busca já funciona sem isso. Ligar melhora o ranking e exige um download.
          </p>
          <Button className="mt-2" onClick={() => setConfirming(true)} disabled={busy}>
            Ligar
          </Button>
        </>
      )}
    </div>
  );
}
