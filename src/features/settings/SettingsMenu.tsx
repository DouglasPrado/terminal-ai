import { useState } from "react";
import { Bell, EyeOff, Settings, SlidersHorizontal } from "lucide-react";
import {
  Menu,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuToggle,
} from "../../components/Menu";
import { ipc, type AppSettings } from "../../lib/ipc";

export function SettingsMenu({
  settings,
  onChange,
}: {
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
}) {
  const [busy, setBusy] = useState(false);
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
  // The mode is reported by the backend as the state actually in force, never as the state that
  // was requested — so a failed apply shows the toggle going back to off, not a lie.
  const setInvisible = (next: boolean) => {
    setBusy(true);
    void ipc
      .setSettings({ invisibleMode: next })
      .then((result) => {
        onChange(result.settings);
        if (result.settings.invisibleMode !== next) {
          window.alert("Não foi possível aplicar o modo invisível.");
        }
      })
      .catch((error: { message?: string }) => {
        window.alert(error.message ?? "Não foi possível aplicar o modo invisível.");
      })
      .finally(() => setBusy(false));
  };
  const testNotification = () => {
    void ipc.notify("Terminal AI", "Notificações estão ativas").then((result) => {
      if (!result.delivered) {
        window.alert("Notificação suprimida: o modo invisível está ativo.");
      }
    });
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
      <MenuItem onClick={testNotification}>
        <Bell size={13} /> Testar notificação do macOS
      </MenuItem>
      <MenuToggle checked={settings.invisibleMode} onChange={setInvisible} disabled={busy}>
        <EyeOff size={13} /> Modo invisível
      </MenuToggle>
      <p className="px-2.5 pb-1.5 pt-0.5 text-meta leading-snug text-text-faint">
        Some do compartilhamento de tela, da Dock, do Cmd+Tab e das notificações. Não esconde o
        processo em execução na máquina, nem a tela física, nem espelhamento para outro monitor — e
        quem compartilha ainda vê o app na própria lista de janelas.
      </p>
    </Menu>
  );
}
