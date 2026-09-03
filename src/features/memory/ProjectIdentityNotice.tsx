import { useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { ipc } from "../../lib/ipc";

/**
 * Warns when a project's directory was renamed or moved.
 *
 * The kernel names a project by its directory basename so that agents agree with the panel. The
 * price is that renaming the directory re-points the project: new memory goes somewhere new and the
 * old memory stops appearing. Without this notice that reads as "my memory vanished" (FR-064).
 */
export function ProjectIdentityNotice({ projectId }: { projectId?: string }) {
  const [notice, setNotice] = useState<{ previous: string; current: string }>();

  useEffect(() => {
    setNotice(undefined);
    if (!projectId) return;
    void ipc.checkMemoryProjectIdentity(projectId).then((result) => {
      if (result.stale && result.previousProject) {
        setNotice({ previous: result.previousProject, current: result.currentProject });
      }
    });
  }, [projectId]);

  if (!notice) return null;

  return (
    <div className="rounded-lg border border-amber-900/60 bg-amber-950/20 p-3 text-xs text-amber-200">
      <p className="flex items-center gap-1.5 font-medium">
        <AlertTriangle size={13} /> A pasta deste projeto mudou de nome
      </p>
      <p className="mt-1 text-amber-200/80">
        A memória anterior está guardada como <code>{notice.previous}</code>, mas agora o projeto
        resolve para <code>{notice.current}</code>. As memórias antigas continuam no kernel — elas
        só não aparecem aqui enquanto os dois nomes divergirem.
      </p>
    </div>
  );
}
