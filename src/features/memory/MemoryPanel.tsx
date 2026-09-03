import { useEffect, useMemo, useState } from "react";
import { Bot, Plus, Search, Trash2 } from "lucide-react";
import { ipc, type MemoryEntry, type MemoryType, type Scope } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Field, Select, TextArea, TextInput } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { KernelBanner, KernelStatusChip } from "./KernelStatusChip";
import { MigrationBanner } from "./MigrationBanner";
import { WiringPanel } from "./WiringPanel";
import { MemorySettings } from "./MemorySettings";
import { ProjectIdentityNotice } from "./ProjectIdentityNotice";
import { HandoffList } from "./HandoffList";

type Selection = { sessionId: string; text: string; worktreeId?: string };
type ScopeMode = "global" | "project" | "worktree" | "session";

/**
 * Kernel content is untrusted: search snippets arrive with `<mark>` markup around the match, and a
 * page body was written by whatever agent last touched the shared store. React escapes strings, so
 * the risk is not injection — it is that raw markup would be shown to the user as literal text.
 * Strip it and bound the length.
 */
function readable(text: string, limit = 4000): string {
  return text
    .replace(/<[^>]*>/g, "")
    .slice(0, limit)
    .trim();
}

export function MemoryPanel({ projectId }: { projectId?: string }) {
  const [scopeMode, setScopeMode] = useState<ScopeMode>("global");
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection>();
  const [editor, setEditor] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [memoryType, setMemoryType] = useState<MemoryType>("fact");
  const [contextPreview, setContextPreview] = useState("");
  const [open, setOpen] = useState<MemoryEntry>();
  const [editing, setEditing] = useState<MemoryEntry>();
  const scope = useMemo<Scope>(
    () =>
      scopeMode === "project" && projectId
        ? { level: "project", refId: projectId }
        : scopeMode === "worktree" && selection?.worktreeId
          ? { level: "worktree", refId: selection.worktreeId }
          : scopeMode === "session" && selection
            ? { level: "session", refId: selection.sessionId }
            : { level: "global" },
    [projectId, scopeMode, selection],
  );
  const refresh = () =>
    void (query ? ipc.searchMemory(query, scope) : ipc.listMemory(scope)).then((result) =>
      setEntries(result.entries),
    );

  useEffect(refresh, [query, scope]);
  useEffect(() => {
    const receive = (event: Event) => {
      const detail = (event as CustomEvent<Selection>).detail;
      setSelection(detail);
      setTitle(detail.text.split("\n").find(Boolean)?.slice(0, 80) ?? "Terminal selection");
      setBody(detail.text);
      setEditor(true);
    };
    window.addEventListener("terminal-memory-selection", receive);
    return () => window.removeEventListener("terminal-memory-selection", receive);
  }, []);

  const save = () => {
    const chosenScope = scope;
    // Three paths, and the middle one used to silently lose work: capturing a selection sent only
    // the body, so a title the user had just edited was thrown away and re-derived from the text.
    const request = editing
      ? ipc.updateMemory(chosenScope, editing.id, title, body)
      : selection
        ? ipc.captureSelectionToMemory(selection.sessionId, body, chosenScope, memoryType, title)
        : ipc.addMemory(chosenScope, memoryType, title, body);
    void request.then(() => {
      setEditor(false);
      setEditing(undefined);
      setSelection(undefined);
      setTitle("");
      setBody("");
      refresh();
    });
  };

  const openEntry = (entry: MemoryEntry) => {
    // The list only carries a snippet; the body has to be fetched. Before this, clicking a row did
    // nothing at all — the only way to read a memory was the tooltip.
    void ipc.readMemoryPage(scope, entry.id).then((result) => setOpen(result.page));
  };

  const startEdit = (entry: MemoryEntry) => {
    setEditing(entry);
    setTitle(entry.title);
    setBody(entry.body);
    setMemoryType(entry.type);
    setOpen(undefined);
    setEditor(true);
  };

  const remove = (entry: MemoryEntry) => {
    void ipc.deleteMemory(scope, entry.id).then(() => {
      setOpen(undefined);
      refresh();
    });
  };

  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-2">
        <KernelStatusChip />
      </div>
      <div className="mb-2 space-y-2">
        <KernelBanner />
        <ProjectIdentityNotice projectId={projectId} />
        <MigrationBanner onDone={refresh} />
        <HandoffList scope={scope} />
        <MemorySettings />
        <WiringPanel projectId={projectId} />
      </div>
      <div className="mb-2 flex items-end gap-1.5">
        <div className="min-w-0 flex-1">
          <Field label="Escopo">
            <Select
              value={scopeMode}
              onChange={(event) => setScopeMode(event.target.value as ScopeMode)}
            >
              <option value="global">Global</option>
              <option value="project" disabled={!projectId}>
                Projeto selecionado
              </option>
              <option value="worktree" disabled={!selection?.worktreeId}>
                Worktree da sessão
              </option>
              <option value="session" disabled={!selection}>
                Sessão selecionada
              </option>
            </Select>
          </Field>
        </div>
        <Button
          icon
          title="Adicionar memória"
          aria-label="Adicionar memória"
          onClick={() => setEditor(true)}
        >
          <Plus size={14} />
        </Button>
      </div>
      <div className="relative mb-2">
        <Search
          size={13}
          aria-hidden
          className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-text-faint"
        />
        <TextInput
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Buscar na memória"
          className="pl-7"
        />
      </div>
      <div className="space-y-px">
        {entries.map((entry) => (
          <div
            key={entry.id}
            className="group flex w-full items-center gap-2 rounded-control px-1.5 py-1.5 transition-colors hover:bg-raised"
          >
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
              onClick={() => openEntry(entry)}
              title={readable(entry.body, 400)}
            >
              <span className="shrink-0 rounded-chip border border-border-subtle bg-raised px-1.5 py-px font-mono text-readout text-accent">
                {entry.type}
              </span>
              {entry.author === "agent" ? (
                <Bot
                  size={12}
                  aria-label="Escrita por um agente"
                  className="shrink-0 text-text-faint"
                />
              ) : null}
              <span className="min-w-0 flex-1 truncate text-meta text-text-muted">
                {entry.title}
              </span>
            </button>
            <button
              type="button"
              aria-label={`Excluir ${entry.title}`}
              className="shrink-0 text-text-faint opacity-0 transition-opacity hover:text-rose-400 group-hover:opacity-100"
              onClick={() => remove(entry)}
            >
              <Trash2 size={13} />
            </button>
          </div>
        ))}
        {entries.length === 0 && (
          <p className="rounded-control border border-dashed border-border px-3 py-6 text-center text-meta text-text-faint">
            {query ? "Nada encontrado para essa busca." : "Nenhuma memória neste escopo ainda."}
          </p>
        )}
      </div>
      <button
        type="button"
        className="mt-2 text-meta text-text-faint underline-offset-2 transition-colors hover:text-accent hover:underline"
        onClick={() =>
          void ipc.previewMemoryContext(scope).then((result) => setContextPreview(result.composed))
        }
      >
        Ver o contexto que vai para o agente
      </button>
      {editor && (
        <MemoryModal
          title={title}
          body={body}
          type={memoryType}
          selection={Boolean(selection)}
          onTitle={setTitle}
          onBody={setBody}
          onType={setMemoryType}
          onCancel={() => {
            setEditor(false);
            setSelection(undefined);
          }}
          onSave={save}
        />
      )}
      {open && (
        <Modal title={open.title} onClose={() => setOpen(undefined)}>
          <p className="mb-2 text-meta text-text-faint">
            {open.type}
            {open.author === "agent" ? " · escrita por um agente" : ""}
          </p>
          <pre className="max-h-96 overflow-auto whitespace-pre-wrap font-mono text-readout text-text-muted">
            {readable(open.body)}
          </pre>
          <div className="mt-3 flex gap-1.5">
            <Button onClick={() => startEdit(open)}>Editar</Button>
            <Button onClick={() => remove(open)}>Excluir</Button>
          </div>
        </Modal>
      )}
      {contextPreview && (
        <Modal
          title="Contexto de memória"
          description="Exatamente o que será injetado no agente neste escopo."
          width="lg"
          onClose={() => setContextPreview("")}
          footer={<Button onClick={() => setContextPreview("")}>Fechar</Button>}
        >
          <pre className="overflow-auto whitespace-pre-wrap rounded-control border border-border-subtle bg-app p-3 font-mono text-readout leading-5 text-text-muted">
            {contextPreview}
          </pre>
        </Modal>
      )}
    </div>
  );
}

function MemoryModal({
  title,
  body,
  type,
  selection,
  onTitle,
  onBody,
  onType,
  onCancel,
  onSave,
}: {
  title: string;
  body: string;
  type: MemoryType;
  selection: boolean;
  onTitle: (value: string) => void;
  onBody: (value: string) => void;
  onType: (value: MemoryType) => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  return (
    <Modal
      title={selection ? "Salvar seleção do terminal" : "Nova memória"}
      description="Nada é capturado sem você mandar — esta é a única porta de entrada."
      onClose={onCancel}
      footer={
        <>
          <Button variant="ghost" onClick={onCancel}>
            Cancelar
          </Button>
          <Button variant="accent" disabled={!title.trim() || !body.trim()} onClick={onSave}>
            Salvar memória
          </Button>
        </>
      }
    >
      <div className="space-y-2.5">
        <Field label="Título">
          <TextInput value={title} onChange={(event) => onTitle(event.target.value)} />
        </Field>
        <Field label="Tipo">
          <Select value={type} onChange={(event) => onType(event.target.value as MemoryType)}>
            {[
              "fact",
              "decision",
              "constraint",
              "preference",
              "glossary",
              "known_issue",
              "command",
              "todo",
            ].map((value) => (
              <option key={value}>{value}</option>
            ))}
          </Select>
        </Field>
        <Field label="Conteúdo">
          <TextArea value={body} onChange={(event) => onBody(event.target.value)} rows={12} />
        </Field>
      </div>
    </Modal>
  );
}
