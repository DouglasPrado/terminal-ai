import { useEffect, useMemo, useState } from "react";
import { Plus, Search } from "lucide-react";
import { ipc, type MemoryEntry, type MemoryType, type Scope } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Field, Select, TextArea, TextInput } from "../../components/Field";
import { Modal } from "../../components/Modal";

type Selection = { sessionId: string; text: string };

export function MemoryPanel({ projectId }: { projectId?: string }) {
  const [scopeMode, setScopeMode] = useState<"global" | "project" | "session">("global");
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [query, setQuery] = useState("");
  const [selection, setSelection] = useState<Selection>();
  const [editor, setEditor] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [memoryType, setMemoryType] = useState<MemoryType>("fact");
  const [contextPreview, setContextPreview] = useState("");
  const scope = useMemo<Scope>(
    () =>
      scopeMode === "project" && projectId
        ? { level: "project", refId: projectId }
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
    const request = selection
      ? ipc.captureSelectionToMemory(selection.sessionId, body, chosenScope, memoryType)
      : ipc.addMemory(chosenScope, memoryType, title, body);
    void request.then(() => {
      setEditor(false);
      setSelection(undefined);
      setTitle("");
      setBody("");
      refresh();
    });
  };

  return (
    <div>
      <div className="mb-2 flex items-end gap-1.5">
        <div className="min-w-0 flex-1">
          <Field label="Escopo">
            <Select
              value={scopeMode}
              onChange={(event) =>
                setScopeMode(event.target.value as "global" | "project" | "session")
              }
            >
              <option value="global">Global</option>
              <option value="project" disabled={!projectId}>
                Projeto selecionado
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
          <button
            key={entry.id}
            type="button"
            className="flex w-full items-center gap-2 rounded-control px-1.5 py-1.5 text-left transition-colors hover:bg-raised"
            title={entry.body}
          >
            <span className="shrink-0 rounded-chip border border-border-subtle bg-raised px-1.5 py-px font-mono text-readout text-accent">
              {entry.type}
            </span>
            <span className="min-w-0 flex-1 truncate text-meta text-text-muted">{entry.title}</span>
          </button>
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
