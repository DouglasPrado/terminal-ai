---

description: "Task list for Invisible Mode"
---

# Tasks: Invisible Mode

**Input**: Design documents from `/specs/003-stealth-mode/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/](./contracts/tauri-commands.md),
[quickstart.md](./quickstart.md)

**Tests**: included. Not optional here — the constitution requires an observed acceptance criterion
per phase, and `plan.md` names `cargo test` + `vitest` + the manual matrix as the gates. The manual
tasks are marked as such because a process cannot assert its own absence from someone else's screen
recording.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 / US2 / US3 from spec.md
- Every task names the exact file it touches

## Path Conventions

Cargo workspace + pnpm frontend, per `plan.md` → Structure Decision: pure logic in `crates/domain/`,
the Tauri adapter and commands in `src-tauri/src/`, UI in `src/`.

---

## Phase 1: Setup

**Purpose**: put the contract change in front of the code change, as CLAUDE.md §2 requires.

- [X] T001 Fold the delta from `specs/003-stealth-mode/contracts/tauri-commands.md` into the
  authoritative closed command set in
  `specs/001-ai-terminal-workspace/contracts/tauri-commands.md`: add `invisibleMode: boolean` to the
  `AppSettings` shape, document `set_settings`'s apply-before-persist behaviour and the
  `INVISIBLE_MODE_APPLY_FAILED` code, and change `notify`'s response to `{ ok: true, delivered: boolean }`

No dependency is added and no tooling changes, so there is nothing else to set up.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the state, the arithmetic and the adapter every story below calls. Nothing user-visible
lands here.

- [X] T002 Create `crates/domain/src/invisible_mode.rs` with `InvisibleMode { enabled: bool }` and
  `DockCooldown { last_show: Option<Instant> }` exposing `remaining(now: Instant) -> Duration`, both
  IO-free, per `data-model.md` → Domain types
- [X] T003 Declare `pub mod invisible_mode;` in `crates/domain/src/lib.rs` next to the existing `host` and
  `memory` modules
- [X] T004 Add `#[cfg(test)]` unit tests in `crates/domain/src/invisible_mode.rs` covering `DockCooldown`
  at its boundaries (no previous show, 0ms, 999ms, 1000ms, 1001ms after a show) and the
  `InvisibleMode` on/off transition, injecting `now` rather than reading the clock
- [X] T005 Add `invisible_mode: bool` to `AppSettings` and `SettingsPatch` in
  `src-tauri/src/commands.rs`, read it in `read_settings` from the `invisible_mode` settings row
  defaulting to `false` on a missing or unparsable value, and write it in `set_settings` alongside
  the other `SettingsDao` keys
- [X] T006 Add `last_dock_transition: std::sync::Mutex<Option<std::time::Instant>>` to `AppState` in
  `src-tauri/src/state.rs` and initialise it in `AppState::new`
- [X] T007 Create `src-tauri/src/invisible_mode.rs` — the adapter — with `apply(app, enabled)`
  implementing both orderings in `data-model.md` → State transitions. Turning **on**: content
  protection over every `app.webview_windows()`, awaited dock cooldown, `set_dock_visibility`,
  rollback of whatever applied on any failure, refocus of the main window. Turning **off**: the
  reverse order, **best-effort — every switch is attempted even if an earlier one fails**, because
  stopping halfway through a restore strands the user more hidden, not less. Declare
  `mod invisible_mode;` in `src-tauri/src/lib.rs`
- [X] T008 Apply content protection to windows created **after** the mode is already on (FR-014):
  hook window creation in `src-tauri/src/lib.rs` and call the adapter's per-window protection for
  the new window when the stored mode is active

**Checkpoint**: `cargo test` passes and the app builds and behaves exactly as before — nothing is
wired to the adapter yet.

---

## Phase 3: User Story 1 — Disappear before sharing a screen (Priority: P1)

**Goal**: the mode can be turned on from Settings and the app becomes absent from captures, the Dock,
the switcher, the menu bar and notification banners.

**Independent test**: turn it on, start a screen share and a recording, and confirm from the other
side that the app is absent while it stays usable locally.

- [X] T009 [US1] Wire `set_settings` in `src-tauri/src/commands.rs` to call `invisible_mode::apply` when
  `patch.invisible_mode` differs from the stored value, persist **only** after applying succeeded,
  and return the state actually in force; take `app: AppHandle` as a command argument
- [X] T010 [US1] Return `INVISIBLE_MODE_APPLY_FAILED` from `set_settings` in
  `src-tauri/src/commands.rs` when the adapter rolls back, with a message naming what failed
- [X] T011 [US1] Suppress delivery in `commands::notify` in `src-tauri/src/commands.rs` when the
  mode is active and change its response to `{ ok: true, delivered: bool }`, leaving
  `terminal_ai_platform_macos::notify` untouched
- [X] T012 [P] [US1] Add a `MenuToggle` primitive to `src/components/Menu.tsx` — a menu row showing
  checked state that does **not** close the menu on click, unlike `MenuItem`
- [X] T013 [US1] Add `invisibleMode: boolean` to `AppSettings` in `src/lib/ipc.ts`, change `notify`'s
  return type to `{ ok: true; delivered: boolean }`, and add the field to the initial settings state
  in `src/App.tsx`
- [X] T014 [US1] Add the "Modo invisível" `MenuToggle` to `src/features/settings/SettingsMenu.tsx`
  directly below the "Testar notificação do macOS" item, driving it through `ipc.setSettings`, and
  surface both a failed apply and a suppressed test notification instead of reporting success
- [X] T015 [P] [US1] Add `src/features/settings/SettingsMenu.test.tsx` asserting the toggle calls
  `ipc.setSettings({ invisibleMode })` and renders the state the backend returned, not the state
  that was requested
- [ ] T016 [US1] Run acceptance sections A, B and C of `quickstart.md` (capture exclusion with a
  second observer, Dock/switcher/menu bar, notification suppression) and record the result

**Checkpoint**: the mode works within a running session. It does not yet survive a restart and the
window does not yet say it is on — that is Story 2, and this story should not reach a user without it.

---

## Phase 4: User Story 2 — Know it is on, and get back out (Priority: P1)

**Goal**: the state survives a restart, is applied before the window is ever capturable, is readable
from the window itself, and costs the user no capability.

**Independent test**: restart with the mode on and confirm from the window alone that it is on; then
turn it off and confirm the Dock, the switcher, the menu bar and notifications all return.

- [X] T017 [US2] Set `"visible": false` on the main window in `src-tauri/tauri.conf.json` so it is
  not on screen before the mode is in force (research R4)
- [X] T018 [US2] In `setup()` in `src-tauri/src/lib.rs`, read the persisted `invisible_mode`, apply
  it through the adapter, and then call `show()` on the main window **unconditionally on every path,
  including the apply-failed path**, so a failure can never leave the app running with no window and
  no Dock icon
- [X] T019 [P] [US2] Add the persistent indicator to the header in `src/App.tsx`, styled only from
  `src/styles/theme.css` tokens, readable without opening any menu
- [X] T020 [P] [US2] Add a test in `src/App.test.tsx` asserting the indicator appears when
  `settings.invisibleMode` is true and is absent when it is false
- [ ] T021 [US2] Measure what the missing menu bar costs: with the mode on, test Cmd+C, Cmd+V, every
  configured keybinding, and quitting (research R6). **If any is broken**, add
  `attachCustomKeyEventHandler` handling Cmd+C/Cmd+V through `navigator.clipboard` in
  `src/features/terminals/TerminalPane.tsx` and an in-app quit path — the capability wins over
  invisibility (Clarification Q2). If nothing is broken, record the measurement and add no code
- [ ] T022 [US2] Run acceptance sections D, E and F of `quickstart.md` (restart persistence and the
  pre-launch recording, reachability and capability, rapid toggling) and record the result

**Checkpoint**: the feature is complete and trustworthy for a user.

---

## Phase 5: User Story 3 — Understand what it does not hide (Priority: P2)

**Goal**: the limits are stated where the user turns the mode on.

**Independent test**: read the text at the control and check each claim against the running app.

- [X] T023 [US3] Add the limits text next to the control in
  `src/features/settings/SettingsMenu.tsx`, naming the running process, the physical screen, screen
  mirroring to another display, and the window list a sharing app shows to the person sharing (FR-015)
- [ ] T024 [US3] Run acceptance section H of `quickstart.md` and record the result

---

## Phase 6: Polish & Cross-Cutting

- [X] T025 [P] Add one `info` line on a successful apply and one `warn` line on failure in
  `src-tauri/src/invisible_mode.rs`, naming which switch failed (research R9)
- [ ] T026 Run acceptance section G of `quickstart.md` — force the dock call to fail in a debug
  build and confirm content protection is rolled back, nothing is persisted, the control returns to
  off, and a restart does not restore an "on" state
- [X] T027 Run the full gate: `cargo fmt --all && cargo clippy --all-targets -- -D warnings &&
  cargo test && pnpm test`

---

## Dependencies

```text
T001 (contract) ─┐
                 ├─▶ Phase 2: T002 → T003 → T004
                 │              T005, T006 ─┐
                 │                          ├─▶ T007 (adapter) → T008 (new windows)
                 └──────────────────────────┘
                                             │
                    ┌────────────────────────┴────────────────────┐
                    ▼                                             ▼
        Phase 3 (US1): T009 → T010 → T011                 (US1 blocks US2:
                       T012 [P] → T014                     the toggle must exist
                       T013 → T014 → T015 [P] → T016       before restoring it
                                                            at launch means anything)
                    ▼
        Phase 4 (US2): T017 → T018, T019 [P], T020 [P], T021 → T022
                    ▼
        Phase 5 (US3): T023 → T024
                    ▼
        Phase 6:       T025 [P] → T026 → T027
```

- **US1 depends on** the whole of Phase 2.
- **US2 depends on US1** — restoring a mode at launch is meaningless before the mode can be set.
- **US3 depends on US1** only for the file it edits (`SettingsMenu.tsx`); its content is independent
  and could be written first.

## Parallel opportunities

- Phase 2: T005 and T006 touch different files and can run together; T002→T003→T004 is a chain
  inside one crate.
- Phase 3: T012 (`Menu.tsx`) is independent of the Rust tasks T009–T011 and of T013 (`ipc.ts`); T015
  is independent once T014 lands.
- Phase 4: T019 and T020 (`App.tsx` + its test) run alongside T017/T018 (Rust and config).
- Across stories: T023's text can be drafted at any time; only its edit to `SettingsMenu.tsx` has to
  follow T014.

## Implementation strategy

**MVP = Phase 1 + Phase 2 + Phase 3 (US1).** That is a working toggle: the app disappears from
captures, the Dock, the switcher and notifications for as long as the session lasts.

**It should not reach a user on its own.** US2 is also P1 for a reason — without it the mode does not
survive a restart, the window never says it is on, and the launch race in FR-008 is still open. Ship
US1 and US2 together; US3 is text and can follow immediately after.

---

## Implementation record

Written during `/speckit-implement`. Deviations from the task text, and why:

- **T006** stores `Mutex<DockCooldown>` — the domain type itself — rather than the
  `Mutex<Option<Instant>>` the task named. Same information, but the cooldown arithmetic stays
  behind the type that owns and tests it instead of being re-derived at the call site.
- **T019/T020** extracted `src/features/settings/InvisibleModeBadge.tsx` instead of leaving the
  indicator as inline JSX in `App.tsx`. CLAUDE.md §2 says new UI belongs in a
  `src/features/<area>/` component, and inline JSX would also have been untestable without
  rendering the whole app.
- **T015/T020 required a devDependency.** `@testing-library/react` was already installed but had
  never been used and there was no DOM environment, so no component could be rendered under test.
  Added `jsdom` (dev only, absent from the shipped bundle) and `test: { environment: "jsdom" }` in
  `vite.config.ts`, which had to switch to `defineConfig` from `vitest/config` to typecheck. Also
  needed an explicit `afterEach(cleanup)`: vitest runs without `globals`, so testing-library's
  automatic cleanup never registers and the DOM accumulates across tests.
- **T008** hooks `on_page_load` rather than a window-created event: Tauri 2.11 has no such event
  (`RunEvent` carries no `Created` variant), and every new window loads a page. Re-applying
  protection is idempotent.
- **T025** landed inside T007 rather than as a separate pass — the log lines belong on the same
  branches as the failures they describe, and splitting them would have meant editing the file
  twice for no benefit.

### Still open — needs the app running on a real Mac

T016, T021, T022, T024 and T026 are the observed acceptance gates. They cannot be discharged from
here: three of them (quickstart A1, A2 and E) need a **second person** watching a screen share, and
all of them need the built app in front of a user. The code they exercise is written and every
automated gate passes; what is missing is the observation, and reporting them as done without it
would be exactly the dishonesty FR-006 and FR-009 exist to prevent.
