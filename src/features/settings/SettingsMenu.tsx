import { Bell, Settings, SlidersHorizontal } from "lucide-react";
import { Menu, MenuItem, MenuLabel, MenuSeparator } from "../../components/Menu";
import { ipc, type AppSettings } from "../../lib/ipc";

export function SettingsMenu({
  settings,
  onChange,
}: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
}) {
  const updateBindings = () => {
    const raw = window.prompt("Keybindings (JSON)", JSON.stringify(settings.keybindings, null, 2));
    if (!raw) return;
    try {
      const keybindings = JSON.parse(raw) as Record<string, string>;
      void ipc.setSettings({ keybindings }).then((result) => onChange(result.settings));
    } catch {
      window.alert("JSON inválido");
    }
  };
  return (
    <Menu icon={<Settings size={15} />} title="Configurações" width={264}>
      <MenuItem onClick={updateBindings}>
        <SlidersHorizontal size={13} /> Editar atalhos…
      </MenuItem>
      <MenuSeparator />
      <MenuLabel>Atalhos</MenuLabel>
      {Object.entries(settings.keybindings).map(([action, shortcut]) => (
        <p key={action} className="flex items-center justify-between gap-2 px-2.5 py-1 text-meta">
          <span className="truncate text-text-muted">{action}</span>
          <kbd className="shrink-0 rounded-chip border border-border bg-raised px-1.5 py-px font-mono text-readout text-text">
            {shortcut}
          </kbd>
        </p>
      ))}
      <MenuSeparator />
      <MenuItem onClick={() => void ipc.notify("Terminal AI", "Notificações estão ativas")}>
        <Bell size={13} /> Testar notificação do macOS
      </MenuItem>
    </Menu>
  );
}
