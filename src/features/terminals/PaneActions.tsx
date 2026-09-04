import {
  Columns2,
  Copy,
  Download,
  GitBranch,
  Maximize2,
  MoreHorizontal,
  Pencil,
  Replace,
  RotateCw,
  Rows2,
  X,
} from "lucide-react";
import { Button } from "../../components/Button";
import { Menu, MenuItem, MenuSeparator } from "../../components/Menu";

/**
 * Pane controls. The three high-frequency actions are icon buttons; everything
 * else lives behind one overflow menu. The group is dim until the pane is
 * hovered, focused, active, or has its menu open — chrome that only appears
 * when you reach for it.
 */
export function PaneActions({
  onSplitRight,
  onSplitDown,
  onMaximize,
  onRestart,
  onClose,
  onExport,
  onDuplicate,
  onChangeProvider,
  onChangeWorktree,
  onRename,
}: {
  onSplitRight: () => void;
  onSplitDown: () => void;
  onMaximize: () => void;
  onRestart: () => void;
  onClose: () => void;
  onExport: () => void;
  onDuplicate: () => void;
  onChangeProvider: () => void;
  onChangeWorktree: () => void;
  onRename: () => void;
}) {
  return (
    <div className="ml-auto flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/pane:opacity-100 group-focus-within/pane:opacity-100 has-[[data-open]]:opacity-100 group-data-[active=true]/pane:opacity-100">
      <Button
        variant="ghost"
        size="sm"
        icon
        title="Dividir à direita"
        aria-label="Dividir à direita"
        onClick={onSplitRight}
      >
        <Columns2 size={14} />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        icon
        title="Dividir abaixo"
        aria-label="Dividir abaixo"
        onClick={onSplitDown}
      >
        <Rows2 size={14} />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        icon
        title="Maximizar ou restaurar"
        aria-label="Maximizar ou restaurar"
        onClick={onMaximize}
      >
        <Maximize2 size={14} />
      </Button>
      <Menu icon={<MoreHorizontal size={14} />} title="Mais ações" width={208}>
        <MenuItem onClick={onRestart}>
          <RotateCw size={13} /> Reiniciar
        </MenuItem>
        <MenuItem onClick={onDuplicate}>
          <Copy size={13} /> Duplicar
        </MenuItem>
        <MenuItem onClick={onRename}>
          <Pencil size={13} /> Renomear
        </MenuItem>
        <MenuSeparator />
        <MenuItem onClick={onChangeProvider}>
          <Replace size={13} /> Trocar de agente
        </MenuItem>
        <MenuItem onClick={onChangeWorktree}>
          <GitBranch size={13} /> Trocar worktree
        </MenuItem>
        <MenuItem onClick={onExport}>
          <Download size={13} /> Exportar saída
        </MenuItem>
        <MenuSeparator />
        <MenuItem tone="danger" onClick={onClose}>
          <X size={13} /> Encerrar sessão
        </MenuItem>
      </Menu>
    </div>
  );
}
