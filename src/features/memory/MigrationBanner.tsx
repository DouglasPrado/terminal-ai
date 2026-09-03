import { useState } from "react";
import { ArrowRightLeft, Undo2 } from "lucide-react";
import { ipc, type MigrationReport } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { useKernelStatus } from "./useKernelStatus";

/**
 * Offers to import the legacy memory store — and never does it on its own.
 *
 * The import writes into a store shared with whatever ai-memory the user runs outside the app, so
 * doing it silently at first boot would be writing into someone's memory without asking. The
 * pending count comes from the kernel status snapshot, so this costs no extra call.
 */
export function MigrationBanner({ onDone }: { onDone?: () => void }) {
  const status = useKernelStatus();
  const [report, setReport] = useState<MigrationReport>();
  const [busy, setBusy] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  const pending = status?.pendingMigration ?? 0;
  const usable = status?.state === "ready" || status?.state === "attached";
  if (dismissed || (pending === 0 && !report)) return null;

  const run = (dryRun: boolean) => {
    setBusy(true);
    void ipc
      .runMemoryMigration(dryRun)
      .then((result) => {
        setReport(result);
        if (!dryRun) onDone?.();
      })
      .finally(() => setBusy(false));
  };

  const undo = () => {
    setBusy(true);
    void ipc
      .undoMemoryMigration()
      .then(() => {
        setReport(undefined);
        onDone?.();
      })
      .finally(() => setBusy(false));
  };

  const previewed = report && !report.completedAt;
  const done = report?.completedAt != null;

  return (
    <div className="rounded-lg border border-border bg-panel p-3 text-xs">
      <p className="flex items-center gap-1.5 font-medium text-neutral-200">
        <ArrowRightLeft className="h-3.5 w-3.5 text-accent" />
        {done
          ? `${report.imported} memórias importadas`
          : `${pending} memórias antigas ainda não foram importadas`}
      </p>

      {!report ? (
        <p className="mt-1 text-neutral-400">
          Elas continuam intactas no disco. Ver o que aconteceria não escreve nada.
        </p>
      ) : null}

      {report ? (
        <ul className="mt-2 space-y-0.5 text-neutral-400">
          <li>
            {previewed ? "Seriam importadas" : "Importadas"}: {report.imported}
          </li>
          {report.alreadyImported > 0 ? (
            <li>Já importadas antes: {report.alreadyImported}</li>
          ) : null}
          {report.skipped.map((s) => (
            <li key={s.entryId} className="text-amber-300/80">
              Ignorada ({s.entryId.slice(0, 8)}): {s.reason}
            </li>
          ))}
          {report.failed.map((f) => (
            <li key={f.entryId} className="text-rose-300/80">
              Falhou ({f.entryId.slice(0, 8)}): {f.reason}
            </li>
          ))}
        </ul>
      ) : null}

      <div className="mt-2 flex gap-1.5">
        {!done ? (
          <>
            <Button onClick={() => run(true)} disabled={busy || !usable}>
              Ver o que aconteceria
            </Button>
            <Button onClick={() => run(false)} disabled={busy || !usable}>
              Importar
            </Button>
          </>
        ) : (
          <Button onClick={undo} disabled={busy}>
            <Undo2 size={13} /> Desfazer
          </Button>
        )}
        <Button onClick={() => setDismissed(true)} disabled={busy}>
          Agora não
        </Button>
      </div>

      {!usable ? (
        <p className="mt-1.5 text-neutral-500">A memória precisa estar ativa para importar.</p>
      ) : null}
    </div>
  );
}
