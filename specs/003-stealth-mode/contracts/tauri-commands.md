# Contract Delta: Tauri Commands — Invisible Mode

**Amends** `specs/001-ai-terminal-workspace/contracts/tauri-commands.md`, which remains the
authoritative record of the closed command set. Nothing here adds a command; the surface stays
closed. A task in `tasks.md` folds this delta into that file, so the two never drift.

---

## Changed shape: `AppSettings`

```ts
export interface AppSettings {
  projectRoots: string[];
  keybindings: Record<string, string>;
  scrollbackLines: number;
  memoryAutoCapture: boolean;
  usageRefreshSeconds: number;
  invisibleMode: boolean;   // NEW — app-wide, default false
}
```

`invisibleMode` is returned by `get_settings` and by `set_settings`, like every other field. A
missing or unreadable stored value reads as `false` (see data-model.md).

---

## `set_settings`

```ts
{ patch: Partial<AppSettings> } → { settings: AppSettings }
```

**New behaviour when `patch.invisibleMode` is present and differs from the stored value**: the
command applies the OS-level mode *before* persisting, and persists only if applying succeeded. The
returned `settings.invisibleMode` is therefore the state that is actually in force — never the state
that was requested. A caller that sends `true` and reads `false` back has been told, in the only way
that cannot be misread, that it did not work.

**Validation**: none beyond the type. There is no path, provider or cwd to check; the value is a
boolean the user owns. Sending the value it already has is a no-op and returns success.

**New error codes**:

| Code | When | Recovery |
|------|------|----------|
| `INVISIBLE_MODE_APPLY_FAILED` | any OS switch refused; everything that applied has been rolled back | The stored and returned value is `false`. The UI shows the failure; the user may retry. |

Existing codes (`INVALID_PROJECT_ROOT`, `INVALID_KEYBINDINGS`, `INVALID_SCROLLBACK`,
`INVALID_USAGE_INTERVAL`) are unchanged.

**Timing**: may take up to ~1.1s longer than today when a dock transition cooldown is pending
(research R3). The wait is asynchronous and does not block other commands.

---

## `notify`

```ts
{ title: string, body: string } → { ok: true, delivered: boolean }   // `delivered` is NEW
```

`delivered` is `false` when the invisible mode is active and the notification was therefore
suppressed. Suppressed notifications are dropped, not queued.

**Why the response changed rather than the call failing**: suppression is the command working as
specified, not an error. Returning `{ ok: true }` alone would let the Settings test button report
success for a banner nobody saw — the exact dishonesty FR-006 forbids.

Callers that ignore `delivered` keep compiling and keep working; only the test button reads it.

---

## Unchanged

`get_settings` keeps its signature; it simply carries the new field. No command is added, removed or
renamed. The frontend still reaches the backend only through `src/lib/ipc.ts`.
