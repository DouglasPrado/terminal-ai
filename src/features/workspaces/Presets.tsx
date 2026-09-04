import { useEffect, useState } from "react";
import { LayoutGrid } from "lucide-react";
import { Menu, MenuItem, MenuLabel, MenuSeparator } from "../../components/Menu";
import { ipc, type LayoutNode, type PaneBinding } from "../../lib/ipc";

type Preset = { id: string; name: string };

export function Presets({
  layout,
  bindings,
  projectId,
  onCreated,
}: {
  layout?: LayoutNode;
  bindings: Record<string, PaneBinding>;
  projectId?: string;
  onCreated: (workspaceId: string, title: string) => void;
}) {
  const [presets, setPresets] = useState<Preset[]>([]);
  const refresh = () => void ipc.listPresets().then((result) => setPresets(result.presets));
  useEffect(refresh, []);

  return (
    <Menu title="Presets" icon={<LayoutGrid size={15} />} align="end" width={220}>
      <MenuLabel>Novo workspace a partir de</MenuLabel>
      {presets.map((preset) => (
        <MenuItem
          key={preset.id}
          onClick={() =>
            void ipc
              .createWorkspaceFromPreset(preset.id, projectId)
              .then(({ workspaceId }) => onCreated(workspaceId, preset.name))
          }
        >
          {preset.name}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem
        disabled={!layout}
        onClick={() => {
          if (!layout) return;
          const name = window.prompt("Nome do preset");
          if (!name) return;
          const providers = Object.fromEntries(
            Object.entries(bindings)
              .filter((entry): entry is [string, PaneBinding & { providerId: string }] =>
                Boolean(entry[1].providerId),
              )
              .map(([paneId, binding]) => [paneId, binding.providerId]),
          );
          void ipc.savePreset(name, layout, providers).then(refresh);
        }}
      >
        Salvar layout atual…
      </MenuItem>
    </Menu>
  );
}
