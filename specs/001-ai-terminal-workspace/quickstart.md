# Quickstart & Validation Guide: AI Terminal Workspace

This is a **run/validation** guide — how to build the app and prove each phase works end to
end. It contains no implementation code; see [data-model.md](./data-model.md),
[contracts/](./contracts/), and `tasks.md` for the how.

Scenario IDs map to the spec's Success Criteria (SC-001…SC-011) and User Stories (P1…P7).

---

## 1. Prerequisites & setup

```bash
# Rust toolchain (not yet installed on this machine)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Xcode Command Line Tools (for the native build + git2/libgit2)
xcode-select --install

# Node 22 + pnpm 10 are already present (nvm). Install JS deps:
pnpm install
```

Run and build:

```bash
pnpm tauri dev      # hot-reloading dev app
pnpm tauri build    # signed/notarized bundle comes later (Phase 10)
```

**App data directory** (created on first run):

```text
~/Library/Application Support/AITerminal/
├── app.db          # SQLite state (never stores secrets)
├── config.toml     # provider profiles + preferences
├── skills/  memory/  sessions/  logs/  cache/
```

**On this machine** (fixtures for the scenarios below):

- CLIs: `claude` (`/opt/homebrew/bin`), `codex` (`~/.local/bin`), `opencode` (`/opt/homebrew/bin`).
- Auth present: `~/.codex/auth.json`; Claude via **Keychain** (no `~/.claude/.credentials.json`);
  OpenCode → **OpenRouter**.
- Real git projects: `~/www/albert`, `~/www/dashboard`, `~/www/genfoot`.

---

## 2. Acceptance scenarios by phase

Each row is "done" only when the **Expected outcome** is observed (Constitution: verification-first).

| Phase | Drive these actions | Expected outcome | Maps to |
|-------|---------------------|------------------|---------|
| **0 — Foundation** | `pnpm tauri dev` | A themed window opens (near-black `#0b0a10`, fuchsia accent, monospace). `~/Library/Application Support/AITerminal/app.db` exists and migrations ran (tables present via `sqlite3 app.db .tables`). | SC-005 |
| **1 — Terminal runtime** | Open 12 terminal panes. In one run `yes`. In another open `vim`, then drag-resize the pane. | UI stays responsive while `yes` floods output; typing in any pane has no perceptible lag; `vim` reflows correctly to the new pane size. | SC-003, SC-004 |
| **2 — Layout tree** | Build the four wireframes: single; 2×2 grid; two columns; asymmetric (two stacked left, one tall right). Quit and relaunch. | Each layout builds via split-right/split-down + drag; after relaunch the split tree, pane sizes, and each pane's provider are restored identically (zero layout loss). | SC-002 |
| **3 — Projects & sidebar** | Confirm sidebar lists `albert`, `dashboard`, `genfoot` with branch + clean/dirty. Open a shell in `albert`; run `pwd`. Start a session in `dashboard`. | Branch/status match `git -C ~/www/<p> status`; shell `pwd` == `/Users/douglasprado/www/albert`; starting the `dashboard` session leaves the `albert` session running with an activity indicator. | SC-001, US2 |
| **4 — Providers** | In `albert`'s cwd, add a Claude pane. Then relaunch with a PATH that omits `claude` and try again. | Claude boots fully interactive in `~/www/albert`; with the CLI absent the pane shows a clear, actionable "command not found — how to install" message (no silent failure). | US1, edge cases |
| **5 — Usage** | Open the sidebar usage cards. Open several Claude panes. Watch `logs/`. Disconnect the network; wait past a refresh window. | Cards populate (Claude session/weekly/model; Codex 5h/weekly/review; OpenCode→OpenRouter balance) from existing auth; logs show **one** poll per provider per window regardless of pane count; offline shows the **last snapshot**, not an error. | SC-006, SC-007 |
| **6 — Worktrees** | For `albert`, create two worktrees on two branches; open an agent in each; have both edit a file. | Two dedicated worktree directories exist (`git -C ~/www/albert worktree list`); edits in one are invisible to the other (isolated working copies). | SC-008 |
| **7 — Layout presets** | Save a 2×2 layout as preset "Multi-agent". Create a new workspace from it. | The new workspace reproduces the split tree and each pane offers to start its assigned provider. | US5 |
| **8 — Skills** | Activate one global skill for both Claude and Codex. Deactivate it. | Both agents receive the skill with no manual file duplication; a preview/diff is shown before applying; deactivation removes **only** app-created content, leaving each CLI's own config intact. | SC-009 |
| **9 — Memory** | Add a memory entry scoped to `albert`. Start an `albert` agent, then a `dashboard` agent. Search by keyword. | The entry is offered to `albert` agents and **never** to `dashboard` agents; keyword search returns it; automatic full-output capture stays OFF by default. | SC-010 |
| **Session history / resume** | Close a running Claude session in `albert`. Reopen it from the project's session history. Separately, add a brand-new pane. | The reopened pane resumes via the agent's native continue (`claude --continue`/`--resume`); the brand-new pane starts fresh; resume works without a live background process. | SC-011 |
| **10 — Daemon / macOS (deferred)** | N/A in v1 | Session persistence across app close, Developer-ID signing, and notarization are validated in the later daemon phase. | — |

---

## 3. How to run automated checks

```bash
cargo test            # Rust unit + integration (adapters via mockito, snapshots via insta)
pnpm test             # Frontend logic (Vitest): layout-tree reducers, Zustand stores
```

- Use the **/verify** skill to drive an affected end-to-end flow and observe behavior before
  committing a nontrivial change.
- Use the **/run** skill to launch the app (`pnpm tauri dev`) and confirm a change works in the
  real app, not just tests.
- A phase is complete only when its row above is observed **and** its `cargo test` / `pnpm test`
  suites pass.
