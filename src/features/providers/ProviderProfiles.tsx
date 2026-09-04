import { useEffect, useState } from "react";
import { Boxes } from "lucide-react";
import { ipc, type ProviderSummary } from "../../lib/ipc";
import { Button } from "../../components/Button";
import { Field, TextInput } from "../../components/Field";
import { Modal } from "../../components/Modal";
import { Tooltip } from "../../components/Tooltip";
import { ProviderIcon } from "../../lib/providers";

/** Custom provider profiles (T061 / FR-015): list providers and add a user-defined CLI. */
export function ProviderProfiles() {
  const [open, setOpen] = useState(false);
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const empty = { id: "", label: "", command: "", args: "", color: "#e879f9" };
  const [form, setForm] = useState(empty);
  const refresh = () =>
    void ipc
      .listProviders()
      .then((result) => setProviders(result.providers))
      .catch(() => {});
  useEffect(() => {
    if (open) refresh();
  }, [open]);
  const save = () => {
    if (!form.id.trim() || !form.command.trim()) return;
    void ipc
      .upsertProviderProfile({
        id: form.id.trim(),
        label: form.label.trim() || form.id.trim(),
        command: form.command.trim(),
        args: form.args.split(/\s+/).filter(Boolean),
        color: form.color,
      })
      .then(() => {
        setForm(empty);
        refresh();
      })
      .catch(() => {});
  };
  return (
    <>
      <Tooltip label="Agentes">
        <Button
          variant={open ? "accent" : "ghost"}
          icon
          aria-pressed={open}
          aria-label="Agentes"
          onClick={() => setOpen(true)}
        >
          <Boxes size={15} />
        </Button>
      </Tooltip>
      {open && (
        <Modal
          title="Agentes"
          description="CLIs detectadas no PATH resolvido, e as suas próprias."
          onClose={() => setOpen(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setOpen(false)}>
                Fechar
              </Button>
              <Button
                variant="accent"
                disabled={!form.id.trim() || !form.command.trim()}
                onClick={save}
              >
                Adicionar agente
              </Button>
            </>
          }
        >
          <ul className="mb-4 space-y-1">
            {providers.map((provider) => (
              <li
                key={provider.id}
                className="flex items-center gap-2 rounded-control border border-border-subtle bg-panel px-2.5 py-1.5"
              >
                <span
                  className="shrink-0"
                  style={{ color: provider.color ?? "var(--color-text-muted)" }}
                >
                  <ProviderIcon id={provider.id} size={14} />
                </span>
                <span className="min-w-0 flex-1 truncate text-ui text-text">{provider.label}</span>
                <span className="font-mono text-readout text-text-faint">{provider.kind}</span>
                {!provider.detected && (
                  <span className="font-mono text-readout text-warning">não encontrada</span>
                )}
              </li>
            ))}
          </ul>
          <div className="space-y-2 border-t border-border-subtle pt-3">
            <p className="font-mono text-readout uppercase tracking-[0.2em] text-text-faint">
              Novo agente
            </p>
            {(
              [
                ["id", "identificador", "claude-beta"],
                ["label", "nome exibido", "Claude Beta"],
                ["command", "comando", "/usr/local/bin/claude"],
                ["args", "argumentos", "--model opus"],
              ] as const
            ).map(([field, label, placeholder]) => (
              <Field key={field} label={label}>
                <TextInput
                  value={form[field]}
                  placeholder={placeholder}
                  className="font-mono"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, [field]: event.target.value }))
                  }
                />
              </Field>
            ))}
            <Field label="cor de identidade">
              <input
                type="color"
                value={form.color}
                onChange={(event) =>
                  setForm((current) => ({ ...current, color: event.target.value }))
                }
                className="h-8 w-16 cursor-pointer rounded-control border border-border bg-app px-1"
              />
            </Field>
          </div>
        </Modal>
      )}
    </>
  );
}
