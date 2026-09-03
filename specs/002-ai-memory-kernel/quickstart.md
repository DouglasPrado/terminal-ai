# Quickstart & Validation Guide: ai-memory as the Memory Kernel

A **run/validation** guide — how to prove each phase works end to end. No implementation code; see
[data-model.md](./data-model.md), [contracts/](./contracts/) and `tasks.md` for the how.

Scenario IDs map to the spec's Success Criteria (SC-012…SC-020) and User Stories (US8…US12).

---

## 1. Prerequisites & setup

```bash
# The toolchain from feature 001 is unchanged.
pnpm install

# Fetch and verify the pinned kernel binary (writes src-tauri/binaries/, git-ignored).
bash scripts/fetch-ai-memory.sh
cat scripts/ai-memory.lock          # the single source of the version pin + SHA-256

pnpm tauri dev
```

**Kernel data lives outside the app** (research §5) — this is the shared store, not a Terminal AI
directory:

```text
~/Library/Application Support/ai-memory/
├── wiki/    # markdown source of truth, git-versioned
├── db/      # derived SQLite index (Terminal AI never opens this)
├── raw/  models/  logs/
└── config.toml
```

Terminal AI's own directory keeps only its state: `app.db` (now with the `V005` tables), `config.toml`,
`skills/`, `sessions/`, `logs/`, `cache/`, plus `memory/` retained read-only as the migration source.

**Fixtures on this machine**: real git projects `~/www/albert`, `~/www/dashboard`, `~/www/genfoot`;
CLIs `claude`, `codex`, `opencode` on `PATH`.

> **Before scenario K5**, snapshot the agent configuration files so the removal test has ground
> truth: `shasum -a 256 ~/.claude.json ~/.claude/settings.json ~/.codex/config.toml \
> ~/.config/opencode/opencode.json > /tmp/agent-cfg.before`

---

## 2. Acceptance scenarios by phase

A row is "done" only when the **Expected outcome** is observed (Constitution: verification-first).

| Phase | Drive these actions | Expected outcome | Maps to |
|-------|---------------------|------------------|---------|
| **K0 — Kernel boots** | Ensure nothing is listening on the kernel port. Launch the app and watch the memory panel. | The panel opens immediately in a `starting` state and reaches `ready` within 5s (15s on the very first run, which initializes the data dir). No boot step waits on the kernel. | SC-012, US8 |
| **K1 — Attach, and leave alone** | Start `ai-memory serve --transport http --bind 127.0.0.1:49374 --enable-web` by hand. Launch the app, confirm the panel reports an attached, not-owned kernel. Quit the app. Run `lsof -nP -iTCP:49374 -sTCP:LISTEN`. | The app attaches instead of spawning; `stop_memory_kernel` is refused with `MEMORY_KERNEL_NOT_OWNED`; **after quitting the app the external server is still listening**. | SC-019, FR-039, Principle VII |
| **K2 — Foreign port & missing binary** | (a) `nc -l 49374`, then launch the app. (b) Move the sidecar aside and clear the configured path, then launch. | (a) `portConflict` with actionable guidance; the app neither attaches nor spawns over it. (b) `notInstalled` with install guidance — and every other feature (terminals, projects, usage) works normally. | FR-040, FR-044, US8 |
| **K3 — Resilience** | Open 6 panes across 2 projects. `kill <kernel pid>`. Wait. Then let it recover. | No session closes, no pane freezes, the UI stays interactive; the panel shows unavailable within 60s; the kernel comes back without restarting the app. With N memory views open, the status endpoint is polled once per interval. | SC-013, SC-020, FR-041, FR-042 |
| **K4 — Migration** | With ≥50 legacy entries in `app.db`, open the panel. Run the import in preview, then for real, then again. Then undo it. | Nothing imports at startup; the panel reports the pending count. Preview writes nothing. The real run creates the pages; the second run creates zero. Undo removes the imported pages and the legacy rows and markdown are still on disk. | SC-016, US9, FR-051…FR-054 |
| **K5 — Wiring with consent** | For `albert`, open the wiring flow for `claude-code`. Read the diff. Confirm. Then remove it and re-check the snapshot from §1. | The preview names the target file and shows the diff, and nothing is written until confirmed. Applying records the artifacts. Removal restores every touched file and leaves unrelated entries byte-identical to `/tmp/agent-cfg.before`. | SC-017, US10, FR-055…FR-057 |
| **K5b — Consent is real** | Before consenting to capture, run a Claude session in `albert`. Then edit `~/.claude/settings.json` by hand and try to remove the wiring. | With no capture consent, the session produces no lifecycle observations for that repository. The hand-edited file makes removal refuse with `MEMORY_WIRING_DRIFTED`, explaining and pointing at the backup — it does not clobber the edit. | FR-057, FR-058, Principle III |
| **K6 — Agents share the memory** | With `albert` wired, add a memory entry from the panel. Open a Claude pane in `albert` and ask it to call `memory_status`, then to read the page the panel just created. | The agent reports the same page count the panel shows and reads back the panel's page. This is the proof the shared store and the basename project mapping actually line up. | SC-015, US10 |
| **K7 — Isolation & worktrees** | Add memory with the keyword `PALAVRACHAVE` to both `albert` and `dashboard`. Search from `albert`. Then create a worktree of `albert`, write memory from inside it, and read from the parent. Then run `remove_worktree` on it. | Searching from `albert` returns only `albert`'s page. Memory written from the worktree is visible in `albert`. **`remove_worktree` still succeeds** — nothing untracked was written into the working tree. | SC-014, US11, FR-046, FR-047 |
| **K8 — Handoff** | In wired `albert`, finish a Claude session. Open a Codex pane in the same project and check the panel. | The pending handoff is listed with summary, open questions and next steps; accepting it stops it being offered. | SC-018, US12, FR-060 |
| **K9 — No surprise network** | On a fresh data dir with hybrid search off, start the app offline and watch the kernel's log directory. Then enable hybrid search in settings. | No model download occurs and search still works (full-text, entity, graph). Enabling the toggle discloses the ~87 MB size before fetching. | FR-062, research §9 |
| **K10 — Stale wiring** | Apply wiring, then move or rename the sidecar binary, then restart the app. | The binding is reported `stale` with an offer to re-apply — not left silently broken. | FR-059 |

---

## 3. How to run automated checks

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings
cargo test                       # unit + mockito; the sidecar tests are #[ignore]d
cargo test -- --ignored          # integration against a real sidecar (needs the binary)
pnpm test && pnpm lint && pnpm build
```

The `#[ignore]`d integration tests spawn a sidecar on an **ephemeral port with a temporary data
directory** — never the shared store — and cover: status → write-page → search → read-page →
delete-page → shutdown.

Two behaviours are unit-tested rather than left to the manual scenarios above, because getting them
wrong is data-loss-class:

- the supervisor never terminates a process it did not start (a pure state-transition test, backing
  K1);
- an unscoped kernel query is unrepresentable in the typed client (backing K7).
