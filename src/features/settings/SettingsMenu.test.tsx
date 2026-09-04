import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SettingsMenu } from "./SettingsMenu";
import { ipc, type AppSettings } from "../../lib/ipc";

vi.mock("../../lib/ipc", () => ({
  ipc: { setSettings: vi.fn(), notify: vi.fn() },
}));

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    projectRoots: ["~/www"],
    keybindings: {},
    scrollbackLines: 10_000,
    memoryAutoCapture: false,
    usageRefreshSeconds: 300,
    invisibleMode: false,
    ...overrides,
  };
}

async function openMenu() {
  fireEvent.click(screen.getByRole("button", { name: "Configurações" }));
}

describe("invisible mode control", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(window, "alert").mockImplementation(() => {});
  });
  // Vitest runs without `globals`, so testing-library's automatic cleanup never registers and the
  // DOM would accumulate across tests.
  afterEach(cleanup);

  it("sits directly below the notification test", async () => {
    render(<SettingsMenu settings={settings()} onChange={() => {}} />);
    await openMenu();
    // FR-001 says "directly below", so assert adjacency rather than mere presence.
    const notification = screen.getByRole("menuitem", { name: /Testar notificação/ });
    const invisible = screen.getByRole("menuitemcheckbox", { name: /Modo invisível/ });
    expect(notification.nextElementSibling).toBe(invisible);
  });

  it("asks the backend to turn the mode on", async () => {
    const onChange = vi.fn();
    vi.mocked(ipc.setSettings).mockResolvedValue({ settings: settings({ invisibleMode: true }) });
    render(<SettingsMenu settings={settings()} onChange={onChange} />);
    await openMenu();
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Modo invisível/ }));
    expect(ipc.setSettings).toHaveBeenCalledWith({ invisibleMode: true });
  });

  it("renders the state the backend returned, not the one that was requested", async () => {
    // The whole point of FR-009: a rolled-back apply must not leave the control claiming success.
    const onChange = vi.fn();
    vi.mocked(ipc.setSettings).mockResolvedValue({ settings: settings({ invisibleMode: false }) });
    render(<SettingsMenu settings={settings()} onChange={onChange} />);
    await openMenu();
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Modo invisível/ }));
    await vi.waitFor(() => expect(onChange).toHaveBeenCalled());
    expect(onChange.mock.calls[0]?.[0].invisibleMode).toBe(false);
    expect(window.alert).toHaveBeenCalled();
  });

  it("says a test notification was suppressed instead of reporting success", async () => {
    vi.mocked(ipc.notify).mockResolvedValue({ ok: true, delivered: false });
    render(<SettingsMenu settings={settings({ invisibleMode: true })} onChange={() => {}} />);
    await openMenu();
    fireEvent.click(screen.getByRole("menuitem", { name: /Testar notificação/ }));
    await vi.waitFor(() => expect(window.alert).toHaveBeenCalled());
    expect(vi.mocked(window.alert).mock.calls[0]?.[0]).toMatch(/suprimida/i);
  });

  it("states what the mode does not hide", async () => {
    render(<SettingsMenu settings={settings()} onChange={() => {}} />);
    await openMenu();
    const text = screen.getByText(/Não esconde/).textContent ?? "";
    expect(text).toMatch(/processo/i);
    expect(text).toMatch(/tela física/i);
    expect(text).toMatch(/espelhamento/i);
  });
});
