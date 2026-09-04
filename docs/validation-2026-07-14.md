# Acceptance record — 2026-07-14

This record maps the quickstart phases to the checks executed on the development Mac. Automated
checks use disposable repositories/directories where changing real user projects would be unsafe.

| Phase                | Result  | Evidence                                                                                                                                                                                                                                               |
| -------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 0 — Foundation       | Pass    | `pnpm tauri dev` reached the native process; Vite was ready in 159 ms. The app DB migrated with all expected tables and the UI created workspaces on boot.                                                                                             |
| 1 — Terminal runtime | Pass    | PTY smoke covered interactive input and resize. A 12-terminal heavy-output test completed without starvation in 0.23 s; output is bounded/batched at 8 ms or 64 KiB.                                                                                   |
| 2 — Layout tree      | Pass    | Four reducer cases cover split, resize, close, maximize, and invariants. Native boot restored a persisted workspace and pane binding from SQLite.                                                                                                      |
| 3 — Projects         | Pass    | Discovery persisted `albert`, `dashboard`, `genfoot`, and `terminal-ai`; git2 inspection supplies branch/dirty/ahead/behind and session counts. Session cwd/trust gates are covered by command paths.                                                  |
| 4 — Providers        | Pass    | Built-in provider detection and login-shell PATH resolution compiled and ran at native boot. Missing executables are disabled with a provider-specific detection message.                                                                              |
| 5 — Usage            | Pass    | One centralized refresh stored exactly one row each for Claude, Codex, and OpenCode. Claude/Codex returned fresh readings; unavailable OpenRouter credentials retained a stale card instead of failing the UI. Adapter parsers and mocked HTTP passed. |
| 6 — Worktrees        | Pass    | A disposable git repository test created, listed, isolated, and removed a feature worktree. Dirty and live-session removal guards are implemented.                                                                                                     |
| 7 — Presets          | Pass    | Four named seeds (`Review`, `Implementation`, `Debug`, `Multi-agent`) were persisted; save/create commands preserve layout plus pane providers without auto-starting sessions.                                                                         |
| 8 — Skills           | Pass    | The manager test previewed/applied/removed a marked Claude artifact and then proved removal refuses an unmarked user-owned file. Binding precedence is persisted.                                                                                      |
| 9 — Memory           | Pass    | The memory test added the same keyword to two project scopes and proved an alpha search returned only alpha. Markdown, revision, FTS5, explicit capture, and context preview paths passed.                                                             |
| History/resume       | Pass    | Provider resume capabilities and history commands are wired; UI history clicks call native resume while empty-pane provider selection starts fresh.                                                                                                    |
| 10 — Deferred        | Tracked | Daemon PTY ownership, Developer ID signing/notarization, and signed auto-update are recorded in `docs/deferred.md` and are not presented as v1 capabilities.                                                                                           |

Final gates executed after this matrix: formatting, Clippy with warnings denied, all Rust tests,
frontend lint/tests, TypeScript compilation, and Vite production build. The Vite build reports only
the known single-bundle size advisory; it is not a correctness failure.
