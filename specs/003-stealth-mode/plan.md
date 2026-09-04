# Implementation Plan: Invisible Mode

**Branch**: `003-stealth-mode` | **Date**: 2026-09-03 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-stealth-mode/spec.md`

## Summary

A single persisted preference that, while on, removes the app from everything a screen capture can
see: the window is excluded from capture streams, the app leaves the Dock, the application switcher
and the menu bar, and macOS notification banners are suppressed. It is turned on and off from one
control in Settings, directly below the notification test, and the window carries a persistent
indicator because the Dock and the menu bar are no longer there to carry it.

Technically it is three OS-level switches driven from the composition root through safe Tauri APIs —
`set_content_protected` per window (`NSWindowSharingType::None`), `set_dock_visibility` for the app
(`TransformProcessType` to a UI-element process), and a suppression check in the `notify` command —
plus one boolean on the existing `AppSettings` object so it persists and restores. No `unsafe`, no
new plugin, no new command.

The two things that make this more than a boolean: applying must be **all-or-nothing with rollback**
(a mode that hid the Dock but left the window capturable looks like it worked), and the state must
be applied **before the window is ever on screen** at launch, which means the window has to start
hidden and be shown after the mode is in force.

## Technical Context

**Language/Version**: Rust (edition 2021, workspace-pinned) + TypeScript 5 / React 19

**Primary Dependencies**: Tauri 2.11.5 (tao 0.35.3, tauri-runtime-wry 2.11.4), rusqlite, xterm.js,
Zustand, Tailwind 4. No new dependency is added by this feature.

**Storage**: `app.db` `settings` table through the existing `SettingsDao` key/value JSON rows

**Testing**: `cargo test` for the pure policy in `domain`, `vitest` for the Settings control and the
indicator, and a manual acceptance matrix in `quickstart.md` for everything the OS actually does —
capture exclusion cannot be asserted from inside the process that is being hidden

**Target Platform**: macOS desktop (the app is macOS-only; the feature has no meaning elsewhere)

**Project Type**: Desktop app — Cargo workspace of focused crates + pnpm React frontend, composed by
Tauri 2

**Performance Goals**: toggle takes effect within 2s (SC-004), which must absorb a dock-transition
cooldown of up to ~1.1s; app boot stays under the constitution's 2s budget even though the window
now starts hidden and is shown explicitly

**Constraints**: no `unsafe` anywhere (`unsafe_code = "deny"` at the workspace); no `unwrap`/`expect`
in runtime paths (both denied by clippy); macOS-only APIs behind `#[cfg(target_os = "macos")]`;
`crates/platform-macos` keeps `#![forbid(unsafe_code)]` and stays free of Tauri types

**Scale/Scope**: one app-wide boolean, one window today with N tolerated, ~5 backend files and ~4
frontend files touched

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1 design. Result: **PASS**, no
deviations to justify.*

| Principle | Verdict | How this design satisfies it |
|-----------|---------|------------------------------|
| I. Typed Rust Boundary | PASS | The feature adds one `bool` to the existing typed `SettingsPatch`. No new command, no string reaching a shell, no path to validate. The WebView gains no new privilege — it can ask for the mode to change and nothing else. |
| II. Native PTY Fidelity | PASS (untouched) | No session, PTY, or output path is read or modified. Hiding the window does not change how its terminals are hosted. |
| III. Non-Destructive & Credential-Safe | PASS | The state is an ordinary preference in `app.db`, never secret material (FR-017). The mode is fully reversible in one action (FR-011) and changes only the app's own presentation — nothing on the user's machine or in their repositories. FR-015's honesty requirement is this principle applied to a UI claim: the app must not overstate what it hides. |
| IV. Single Source of Truth | PASS | One stored value, read through the existing `read_settings`; the OS state is derived from it and never becomes a second store the UI reads. The indicator takes every colour from `src/styles/theme.css`; no hex is written in a component. No poller is added. |
| V. Layout as a Persisted Tree | PASS (untouched) | The layout tree is not read or written. Starting the window hidden does not change what is restored into it. |
| VI. Isolation & Resilience | PASS | The dock cooldown is awaited asynchronously inside the command, never as a blocking sleep on a thread that serves the UI. A failure to apply is reported and rolled back; it cannot leave a session, a window, or the UI wedged. |
| VII. Swappable Session & Memory Hosts | PASS | Window presentation, Dock presence, and notification delivery belong to the app shell, not to `SessionHost` or `MemoryKernel`. `pty-runtime`, `provider-runtime` and `usage-core` are not touched, so the deferred daemon split is unaffected: when sessions move out of process, the shell keeps owning this mode unchanged. |

### Placement note (not a deviation)

The pure part of the feature — the state, the transition, the cooldown arithmetic, the rollback
ordering — lives in `crates/domain/src/invisible_mode.rs` and is unit-tested there with no IO. The part that
cannot leave the composition root is the adapter that calls the Tauri handles, because a window and
an `AppHandle` only exist there; `src-tauri/src/invisible_mode.rs` is that adapter and holds no decisions.
This keeps the constitution's "no business logic in `src-tauri`" rule intact rather than bending it.

## Project Structure

### Documentation (this feature)

```text
specs/003-stealth-mode/
├── plan.md              # This file
├── research.md          # Phase 0 output — the macOS mechanisms and what the source actually does
├── data-model.md        # Phase 1 output — the one entity and its settings row
├── quickstart.md        # Phase 1 output — the acceptance matrix (mostly manual, by necessity)
├── contracts/
│   └── tauri-commands.md   # Delta against the closed command set in feature 001
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 output (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
crates/
└── domain/
    └── src/
        ├── lib.rs              # + `pub mod stealth;`
        └── invisible_mode.rs   # NEW — InvisibleMode, DockCooldown, rollback order. Pure, tested.

src-tauri/
├── tauri.conf.json             # window gains `"visible": false` (FR-008)
└── src/
    ├── invisible_mode.rs       # NEW — thin adapter: content protection, dock visibility, rollback
    ├── commands.rs             # AppSettings/SettingsPatch + invisibleMode; notify suppression
    ├── state.rs                # holds the last dock transition instant for the cooldown
    └── lib.rs                  # setup: apply persisted state, then show the window

src/
├── lib/ipc.ts                  # AppSettings.invisibleMode; notify returns `delivered`
├── components/Menu.tsx         # NEW primitive MenuToggle (a menu row that does not close)
├── features/settings/
│   └── SettingsMenu.tsx        # the control, directly below the notification test
└── App.tsx                     # the persistent indicator in the header
```

**Structure Decision**: the existing layout is kept exactly as the constitution defines it. The only
new files are one pure module in `domain` and one adapter in the composition root; everything else is
an edit to a file that already owns that concern. No new crate, no new command, no new dependency.

## Complexity Tracking

> No Constitution Check violations. Nothing to justify.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _(none)_  | —          | —                                    |
