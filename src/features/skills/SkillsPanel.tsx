import { useEffect, useState } from "react";
import { Trash2, X } from "lucide-react";
import { ipc, type Scope, type SkillSummary } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Field, Select } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { Tooltip } from "../../components/Tooltip";
import { ProviderIcon } from "../../lib/providers";

type Preview = { skill: SkillSummary; provider: string; diff: string; willCreate: string[] };
type Binding = {
  skillId: string;
  scope: string;
  // Rust sends `null` for a global binding; the scope built here uses `undefined`.
  scopeRefId?: string | null;
  enabled: boolean;
  appliedArtifacts: Array<{ providerId: string }>;
};

function providerColor(id: string) {
  return (
    {
      claude: "var(--color-accent)",
      codex: "var(--color-cyan)",
      opencode: "var(--color-provider-opencode)",
    } as Record<string, string>
  )[id];
}

export function SkillsPanel({
  projectId,
  worktreeId,
  workspaceId,
  sessionId,
}: {
  projectId?: string;
  worktreeId?: string;
  workspaceId?: string;
  sessionId?: string;
}) {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [bindings, setBindings] = useState<Binding[]>([]);
  const [scopeLevel, setScopeLevel] = useState<Scope["level"]>("global");
  const [preview, setPreview] = useState<Preview>();
  const [error, setError] = useState<string>();
  const [deleting, setDeleting] = useState<SkillSummary>();
  const refId =
    scopeLevel === "project"
      ? projectId
      : scopeLevel === "worktree"
        ? worktreeId
        : scopeLevel === "workspace"
          ? workspaceId
          : scopeLevel === "session"
            ? sessionId
            : undefined;
  const scope: Scope =
    scopeLevel !== "global" && refId ? { level: scopeLevel, refId } : { level: "global" };
  // Surface failures instead of dropping them: a rejected command used to leave the button
  // looking simply inert, with no way to tell a refusal from a bug.
  const fail = (cause: unknown) =>
    setError(
      typeof cause === "object" && cause && "message" in cause
        ? String((cause as { message: unknown }).message)
        : String(cause),
    );
  const refresh = () =>
    void ipc
      .listSkills(projectId)
      .then((result) => {
        setSkills(result.skills);
        setBindings(result.bindings);
      })
      .catch(fail);
  useEffect(refresh, [projectId]);
  // The library lives on disk and can change while the app runs — a skill added by hand, or one
  // applied from another window. Re-read it on focus and on the sidebar refresh, instead of only
  // at mount, where a newly added skill would stay invisible until the panel remounted.
  useEffect(() => {
    const handler = () => refresh();
    window.addEventListener("focus", handler);
    window.addEventListener("sidebar-refresh", handler);
    return () => {
      window.removeEventListener("focus", handler);
      window.removeEventListener("sidebar-refresh", handler);
    };
  });

  // A global binding comes back from Rust with `scopeRefId: null` while the scope built here
  // carries `refId: undefined`, and `null === undefined` is false — comparing them directly meant
  // no binding ever matched, so an enabled skill still rendered as "Ativar".
  const sameScope = (binding: Binding) =>
    binding.scope === scope.level && (binding.scopeRefId ?? undefined) === scope.refId;

  const boundAtScope = (skillId: string) =>
    bindings.some(
      (binding) => binding.skillId === skillId && sameScope(binding) && binding.enabled,
    );
  const toggleBinding = (skillId: string) => {
    setError(undefined);
    void ipc.setSkillBinding(skillId, scope, !boundAtScope(skillId)).then(refresh).catch(fail);
  };

  return (
    <>
      <Field label="Escopo">
        <Select
          value={scopeLevel}
          onChange={(event) => setScopeLevel(event.target.value as Scope["level"])}
        >
          <option value="global">Global</option>
          <option value="project" disabled={!projectId}>
            Projeto selecionado
          </option>
          <option value="worktree" disabled={!worktreeId}>
            Worktree selecionada
          </option>
          <option value="workspace" disabled={!workspaceId}>
            Workspace ativo
          </option>
          <option value="session" disabled={!sessionId}>
            Sessão em foco
          </option>
        </Select>
      </Field>
      <p className="mb-3 mt-1.5 font-mono text-readout text-text-faint">
        precedência: sessão › workspace › worktree › projeto › global
      </p>
      {error && (
        <p className="mb-2 rounded-control border border-danger/50 bg-danger/10 px-2.5 py-2 text-meta text-danger">
          {error}
        </p>
      )}
      {skills.length === 0 ? (
        <p className="rounded-control border border-dashed border-border px-3 py-6 text-center text-meta text-text-faint">
          Adicione skill.toml + instructions.md na pasta de skills do app.
        </p>
      ) : (
        <div className="space-y-1.5">
          {skills.map((skill) => {
            const bound = boundAtScope(skill.id);
            return (
              <div
                key={skill.id}
                className={`rounded-control border px-2.5 py-2 transition-colors ${
                  bound ? "border-accent-line bg-accent-background" : "border-border bg-panel"
                }`}
              >
                <div className="flex items-center gap-2">
                  <b className="min-w-0 flex-1 truncate text-ui font-medium text-text-strong">
                    {skill.name}
                  </b>
                  <span className="shrink-0 font-mono text-readout text-text-faint">
                    v{skill.version}
                  </span>
                  <Button
                    size="sm"
                    variant={bound ? "accent" : "default"}
                    aria-pressed={bound}
                    title={`Ligar ou desligar esta skill no escopo ${scope.level}`}
                    onClick={() => toggleBinding(skill.id)}
                  >
                    {bound ? "Ativa" : "Ativar"}
                  </Button>
                  <Tooltip label="Excluir skill">
                    <Button
                      size="sm"
                      variant="danger"
                      icon
                      aria-label={`Excluir ${skill.name}`}
                      onClick={() => setDeleting(skill)}
                    >
                      <Trash2 size={13} />
                    </Button>
                  </Tooltip>
                </div>
                <div className="mt-2 flex flex-wrap gap-1">
                  {(["claude", "codex", "opencode"] as const)
                    .filter(
                      (provider) => !skill.providers.length || skill.providers.includes(provider),
                    )
                    .map((provider) => {
                      const active = bindings.some(
                        (binding) =>
                          binding.skillId === skill.id &&
                          sameScope(binding) &&
                          binding.appliedArtifacts.some(
                            (artifact) => artifact.providerId === provider,
                          ),
                      );
                      return (
                        <span
                          key={provider}
                          className="flex h-6 items-center overflow-hidden rounded-control border border-border bg-raised"
                        >
                          <button
                            type="button"
                            style={active ? { color: providerColor(provider) } : undefined}
                            className={`flex h-full items-center gap-1.5 px-2 font-mono text-readout transition-colors hover:bg-raised-hover ${
                              active ? "font-medium" : "text-text-faint"
                            }`}
                            title={
                              active
                                ? `Arquivo gerenciado aplicado em ${provider}`
                                : `Pré-visualizar aplicação em ${provider}`
                            }
                            onClick={() => {
                              setError(undefined);
                              void ipc
                                .previewSkillApply(skill.id, provider, scope)
                                .then((result) => setPreview({ skill, provider, ...result }))
                                .catch(fail);
                            }}
                          >
                            <ProviderIcon id={provider} size={12} />
                            {provider}
                          </button>
                          {active && (
                            <button
                              type="button"
                              className="grid h-full w-5 place-items-center border-l border-border text-text-faint transition-colors hover:bg-danger/15 hover:text-danger"
                              title="Remover arquivo gerenciado pelo app"
                              aria-label={`Remover arquivo de ${provider}`}
                              onClick={() => {
                                setError(undefined);
                                void ipc
                                  .removeSkill(skill.id, provider, scope)
                                  .then(refresh)
                                  .catch(fail);
                              }}
                            >
                              <X size={10} />
                            </button>
                          )}
                        </span>
                      );
                    })}
                </div>
              </div>
            );
          })}
        </div>
      )}
      {deleting && (
        <Modal
          title={`Excluir ${deleting.name}?`}
          description="Remove a skill da biblioteca do app. Os arquivos já aplicados nos agentes continuam onde estão — use o × de cada provider para tirá-los."
          width="xs"
          onClose={() => setDeleting(undefined)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setDeleting(undefined)}>
                Cancelar
              </Button>
              <Button
                variant="danger"
                className="border border-danger/50"
                onClick={() => {
                  const target = deleting;
                  setDeleting(undefined);
                  setError(undefined);
                  void ipc.deleteSkill(target.id).then(refresh).catch(fail);
                }}
              >
                Excluir
              </Button>
            </>
          }
        >
          <p className="font-mono text-readout text-text-muted">{deleting.id}</p>
        </Modal>
      )}
      {preview && (
        <Modal
          title={`Aplicar ${preview.skill.name} em ${preview.provider}`}
          description="Nada é sobrescrito: o app só cria e remove os arquivos que ele mesmo gerencia."
          width="lg"
          onClose={() => setPreview(undefined)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setPreview(undefined)}>
                Cancelar
              </Button>
              <Button
                variant="accent"
                onClick={() =>
                  void ipc
                    .applySkill(preview.skill.id, preview.provider, scope)
                    .then(() => {
                      setPreview(undefined);
                      refresh();
                    })
                    .catch((cause) => {
                      setPreview(undefined);
                      fail(cause);
                    })
                }
              >
                Aplicar arquivo gerenciado
              </Button>
            </>
          }
        >
          {preview.willCreate.map((path) => (
            <p key={path} className="truncate font-mono text-readout text-text-muted" title={path}>
              cria {path}
            </p>
          ))}
          <pre className="mt-3 overflow-auto rounded-control border border-border-subtle bg-app p-3 font-mono text-readout leading-5 text-text-muted">
            {preview.diff}
          </pre>
        </Modal>
      )}
    </>
  );
}
