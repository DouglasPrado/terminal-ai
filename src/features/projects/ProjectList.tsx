import { useCallback, useEffect, useState } from "react";
import { ArchiveRestore, Archive, MoreHorizontal, Pencil } from "lucide-react";
import { ipc, type ProjectSummary } from "../../lib/ipc";
import { Menu, MenuItem } from "../../components/Menu";
import { Button } from "../../components/Button";
import { Field, TextInput } from "../../components/Field";
import { Modal } from "../../components/Modal";

/**
 * The sidebar's project list. Deliberately one line per project — name and a live-session dot,
 * nothing else. Branch, dirty state and worktrees describe the workspace you have *open*, so
 * they live in the pinned footer instead of repeating on every row here.
 */
export function ProjectList({
  workspaceId,
  selectedId,
  archived = false,
  onSelect,
  onProjects,
}: {
  workspaceId?: string;
  selectedId?: string;
  /** Render the archived set instead of the active one. */
  archived?: boolean;
  onSelect: (id: string) => void;
  onProjects?: (projects: ProjectSummary[]) => void;
}) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [renaming, setRenaming] = useState<ProjectSummary>();
  const refresh = useCallback(
    () =>
      void ipc.listProjects(workspaceId).then((result) => {
        setProjects(result.projects);
        onProjects?.(result.projects);
      }),
    [onProjects, workspaceId],
  );
  useEffect(refresh, [refresh]);
  // T069: refresh the activity dot when a session starts or ends. Re-listing also reconciles
  // against the filesystem, so a repository deleted or created outside the app corrects itself
  // on the next window focus rather than waiting for a manual refresh.
  useEffect(() => {
    const handler = () => refresh();
    window.addEventListener("projects-refresh", handler);
    window.addEventListener("focus", handler);
    return () => {
      window.removeEventListener("projects-refresh", handler);
      window.removeEventListener("focus", handler);
    };
  }, [refresh]);

  const setArchived = (projectId: string, value: boolean) =>
    void ipc.setProjectArchived(projectId, value).then(refresh);

  const visible = projects.filter((project) => project.archived === archived);
  if (visible.length === 0)
    return (
      <p className="rounded-control border border-dashed border-border px-3 py-4 text-center text-meta text-text-faint">
        {archived ? "Nenhum projeto arquivado" : "Nenhum projeto nesta pasta"}
      </p>
    );
  return (
    <div className="space-y-0.5">
      {visible.map((project) => {
        const selected = !archived && selectedId === project.id;
        return (
          <div
            key={project.id}
            className={`group flex h-9 items-center gap-2 rounded-control border pl-2.5 pr-1 transition-colors ${
              selected
                ? "border-accent-line bg-accent-background"
                : "border-transparent hover:border-border hover:bg-raised"
            } ${archived ? "opacity-60" : ""}`}
          >
            <button
              type="button"
              onClick={() => !archived && onSelect(project.id)}
              title={project.path}
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
            >
              {/* A live agent in this repo is the one thing that glows in the list. */}
              <span
                aria-hidden
                className={`size-1.5 shrink-0 rounded-full ${
                  project.activeSessions > 0
                    ? "bg-accent shadow-[0_0_8px_1px_var(--color-accent)]"
                    : "bg-border-hover"
                }`}
              />
              <span className="truncate text-title text-text">{project.name}</span>
            </button>
            <span className="opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
              <Menu
                size="sm"
                icon={<MoreHorizontal size={14} />}
                title="Ações do projeto"
                width={180}
              >
                <MenuItem onClick={() => setRenaming(project)}>
                  <Pencil size={13} /> Renomear
                </MenuItem>
                {archived ? (
                  <MenuItem onClick={() => setArchived(project.id, false)}>
                    <ArchiveRestore size={13} /> Restaurar
                  </MenuItem>
                ) : (
                  <MenuItem onClick={() => setArchived(project.id, true)}>
                    <Archive size={13} /> Arquivar
                  </MenuItem>
                )}
              </Menu>
            </span>
          </div>
        );
      })}
      {renaming && (
        <RenameProjectModal
          project={renaming}
          onClose={() => setRenaming(undefined)}
          onSaved={() => {
            setRenaming(undefined);
            refresh();
          }}
        />
      )}
    </div>
  );
}

/** Renames a project for display. Emptying the field restores the directory name. */
function RenameProjectModal({
  project,
  onSaved,
  onClose,
}: {
  project: ProjectSummary;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(project.name);
  const save = () => void ipc.setProjectName(project.id, name.trim() || undefined).then(onSaved);
  return (
    <Modal
      title="Renomear projeto"
      description={project.path}
      width="xs"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="accent" onClick={save}>
            Salvar
          </Button>
        </>
      }
    >
      <Field label="Nome">
        <TextInput
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && save()}
        />
      </Field>
      <p className="mt-1.5 text-meta text-text-faint">
        Deixe em branco para voltar ao nome da pasta.
      </p>
    </Modal>
  );
}
