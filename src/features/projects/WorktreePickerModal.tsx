import { useEffect, useState } from "react";
import { FolderRoot, GitBranch } from "lucide-react";
import { ipc, type WorktreeSummary } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";

/** Modal worktree picker (T064) — replaces the hand-typed-UUID prompt for "change worktree". */
export function WorktreePickerModal({
  projectId,
  onPick,
  onClose,
}: {
  projectId?: string;
  onPick: (worktreeId?: string) => void;
  onClose: () => void;
}) {
  const [worktrees, setWorktrees] = useState<WorktreeSummary[]>([]);
  useEffect(() => {
    if (!projectId) return;
    void ipc
      .listWorktrees(projectId)
      .then((result) => setWorktrees(result.worktrees))
      .catch(() => {});
  }, [projectId]);
  return (
    <Modal
      title="Trocar worktree"
      description="A sessão atual do painel será encerrada e reaberta no diretório escolhido."
      width="xs"
      onClose={onClose}
      footer={<Button onClick={onClose}>Cancelar</Button>}
    >
      {!projectId && (
        <p className="mb-2 text-meta text-text-muted">Este painel não está ligado a um projeto.</p>
      )}
      <div className="space-y-1">
        <Button block className="justify-start" onClick={() => onPick(undefined)}>
          <FolderRoot size={13} /> Raiz do projeto
        </Button>
        {worktrees.map((worktree) => (
          <Button
            key={worktree.id}
            block
            className="justify-start"
            title={worktree.path}
            onClick={() => onPick(worktree.id)}
          >
            <GitBranch size={13} className={worktree.dirty ? "text-warning" : "text-text-faint"} />
            <span className="truncate font-mono">{worktree.branch}</span>
          </Button>
        ))}
      </div>
      {projectId && worktrees.length === 0 && (
        <p className="mt-2 text-meta text-text-faint">Nenhuma worktree para este projeto.</p>
      )}
    </Modal>
  );
}
