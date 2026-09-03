# Phase 0 Research: Invisible Mode

Every claim below was checked against the vendored source of the exact versions this workspace
resolves (`tauri 2.11.5`, `tauri-runtime 2.11.3`, `tauri-runtime-wry 2.11.4`, `tao 0.35.3`), not
against documentation. Paths are given so the next reader can re-check them.

---

## R1 — How the window is removed from screen capture

**Decision**: `WebviewWindow::set_content_protected(true)` on every window the app owns.

**Rationale**: it reaches `NSWindow.setSharingType(NSWindowSharingType::None)`
(`tao-0.35.3/src/platform_impl/macos/window.rs:1529`), which is the supported macOS mechanism for
excluding a window from capture streams and screenshots. It is a safe wrapper — no `unsafe` in our
code, so the workspace's `unsafe_code = "deny"` stands. The API exists on both `Window` and
`WebviewWindow` (`tauri-2.11.5/src/webview/webview_window.rs:2227`, `src/window/mod.rs:1818`).

**Alternatives considered**:
- Private CoreGraphics window-server APIs: rejected — private, requires `unsafe`, breaks between
  macOS releases, and buys nothing over the supported path.
- Moving the window off-screen or to a hidden Space while sharing: rejected — it hides the window
  from the user too, which defeats the point.

**Consequence for the spec**: turning protection off restores `NSWindowSharingType::ReadOnly`, which
is the default — so FR-011's "capture behaviour returns" is a real restore, not an approximation.

---

## R2 — How the app leaves the Dock, the switcher and the menu bar

**Decision**: `AppHandle::set_dock_visibility(false)` (`tauri-2.11.5/src/app.rs:659`).

**Rationale**: it dispatches to tao's dock helper, which calls
`TransformProcessType(kProcessTransformToUIElementApplication)`
(`tao-0.35.3/src/platform_impl/macos/dock.rs:33-75`). Becoming a UI-element process is what removes
the Dock tile, the Cmd-Tab entry and the menu bar in one move. Crucially, the helper first sets
`setCanHide(false)` on every window, so the windows **stay on screen** while the app becomes an
accessory — which is exactly the behaviour FR-013 needs and the reason the feature can ship without a
global shortcut.

**Alternatives considered**:
- `AppHandle::set_activation_policy(ActivationPolicy::Accessory)` (`src/app.rs:640`): the same visible
  outcome, but it skips the `canHide(false)` step and the upstream workaround described in R3.
  Rejected as the lower-fidelity path to the same place.

---

## R3 — The one-second dock cooldown, and why it matters twice

**Finding**: `set_dock_hide` is deliberately a **no-op** if it is called within one second of a
`set_dock_show` (`tao-0.35.3/src/platform_impl/macos/dock.rs:41-58`). The upstream comment explains
why: the process-type transition is asynchronous with no completion signal, and hide→show→hide in
quick succession leaves *duplicate Dock icons* stuck in the system.

**Consequence 1 — rapid toggling (FR-016)**: on → off → on inside one second means the second hide is
silently dropped and the Dock icon stays, while the control reads "on". That is precisely the
half-applied state Clarification Q1 forbids.

**Decision**: the command awaits the remaining cooldown before applying a hide. The arithmetic is
pure and lives in `domain::stealth::DockCooldown` (time injected, unit-tested); the wait is a
`tokio::time::sleep` inside the async command, never a blocking sleep — Principle VI. Worst case adds
~1.1s, inside SC-004's 2s budget.

**Consequence 2 — startup is safe**: checked whether the same debounce would silently swallow the
hide we apply at launch. It does not. At launch tao calls `set_dock_visibility` **only when the
configured visibility is false** (`tao-0.35.3/src/platform_impl/macos/app_state.rs:294-298`), so with
the default (visible) nothing records a "last show" and `last_dock_show` is still `None`. A hide from
`setup()` applies immediately.

---

## R4 — No capturable frame at launch (FR-008)

**Decision**: the window is declared `"visible": false` in `src-tauri/tauri.conf.json`, and
`setup()` applies the persisted mode and then calls `show()`.

**Rationale**: the window is created from configuration before `setup()` runs. If it were visible on
creation, there would be a window of time — short, but real, and exactly the moment a recording is
already running — where the app is on screen and capturable before protection is applied. Starting
hidden removes the race instead of narrowing it.

**Risk this introduces, and how it is handled**: a failure between creation and `show()` would leave
the app running with no visible window and no Dock icon — unreachable, the worst outcome in the whole
feature. The `show()` call is therefore unconditional and runs on every path, including the
apply-failed path; applying the mode may fail, showing the window may not be skipped.

**Alternatives considered**: applying protection on a window-created event and leaving the window
visible — rejected, it narrows the race without closing it.

---

## R5 — Suppressing notifications

**Decision**: the check lives in the `notify` command (the composition root, which already has the
settings handle); `terminal_ai_platform_macos::notify` is left untouched. The command's response
gains `delivered: bool`.

**Rationale**: `platform-macos` is a Tauri-free, IO-at-the-edge crate that shells out to `osascript`.
Teaching it to read app settings would give it a reason to know about the database. The decision is
policy and belongs where policy already lives. `delivered` is what lets the Settings test button say
"suppressed" instead of falsely reporting success (FR-006).

**Alternatives considered**: dropping the notification silently and still returning `{ ok: true }` —
rejected, it makes the test button lie, which is the same failure mode FR-009 exists to prevent.

---

## R6 — The menu bar disappearing may cost keyboard shortcuts (open risk)

**Finding**: the app builds no menu of its own (`src-tauri/src/lib.rs` never calls `.menu(...)`), so
it ships Tauri's default macOS menu, and `src/features/terminals/TerminalPane.tsx` registers no
`attachCustomKeyEventHandler`. Copy and paste in the WebView therefore depend on the default menu's
key equivalents. As a UI-element process the app has no *displayed* menu bar; whether `NSApp`'s main
menu still services `performKeyEquivalent:` in that state is **not determinable from the source we
vendor** — it depends on AppKit behaviour and must be measured on the target OS.

**Decision**: measure first, in a dedicated task, before writing any mitigation. Clarification Q2
already fixed the outcome if the measurement is bad: **the capability wins**. The prepared mitigation
is an xterm `attachCustomKeyEventHandler` that handles Cmd+C/Cmd+V against `navigator.clipboard`, plus
an in-app quit path — implemented only if the measurement shows they are needed, so the app does not
carry a workaround for a problem it does not have.

**Why this is not a blocker**: the decision is already made; only the amount of work is unknown, and
its upper bound is one key handler and one menu item.

---

## R7 — Naming, and where the value is stored

**Decision**: canonical identifier `invisible_mode` in Rust, `invisibleMode` in TypeScript, settings
row key `invisible_mode`, UI string "Modo invisível". The `003-stealth-mode` directory keeps its slug.

**Rationale**: Clarification Q5. English identifiers match the rest of the workspace; the shipped UI
is Portuguese ("Configurações", "Testar notificação do macOS"), so the label is too.

---

## R8 — Reusing `set_settings` instead of adding a command

**Decision**: the flag rides on the existing `AppSettings` / `SettingsPatch` pair; `set_settings`
gains an `AppHandle` and applies the mode when the flag changes. No new command is registered.

**Rationale**: the UI already loads one settings object at boot and re-renders from what
`set_settings` returns; `memoryAutoCapture` is the same shape and the pattern is proven in this
codebase. It also keeps the persisted value and the applied OS state on one code path, which is what
makes "the stored value is the only source of truth" (Principle IV) true rather than aspirational.

**Alternatives considered**: a dedicated `set_invisible_mode` command returning its own state object
— rejected: it would either duplicate persistence or force the frontend to merge two objects that can
disagree, and it widens the command surface for nothing.

---

## R9 — Observability (the item left Outstanding by `/speckit-clarify`)

**Decision**: one log line on apply and one on failure, at `info`/`warn`, to the app's existing log
file. No counters, no telemetry, no new sink.

**Rationale**: FR-009's failure path is otherwise undebuggable — the user is told "it failed" and
nothing anywhere says why. The log file is local, already exists, and the spec's Out of Scope already
states the feature does not redact it. Logging the toggle does not weaken the mode: anyone who can
read that file is already someone the mode makes no promise against.
