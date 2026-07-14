# Phase 0 Research: AI Terminal Workspace

Decisions that resolve the Technical Context. Each entry is **Decision / Rationale /
Alternatives considered**. All grounded in the real facts of `akitaonrails/ai-usagebar` and
`akitaonrails/github-visualize` and the locked project decisions.

---

## 1. Desktop shell — Tauri 2

**Decision**: Build the app on **Tauri 2** (Rust core + system WebView), not Electron.

**Rationale**: Keeps all privileged logic in Rust behind a typed command boundary (Principle I),
lets the same crates back a future daemon (Principle VII), and ships a far smaller binary than a
bundled Chromium. Native macOS integration (Keychain, notifications, LaunchAgent) is
first-class from Rust.

**Alternatives considered**: *Electron* — larger footprint, JS-side privileged APIs weaken the
security boundary. *Pure Rust TUI (ratatui, like ai-usagebar)* — cannot satisfy the wireframes
(drag-resizable arbitrary panels, cards, modals, DnD). *Native SwiftUI* — loses Rust reuse and
cross-platform daemon path.

---

## 2. Terminal output streaming — Tauri `ipc::Channel`

**Decision**: Stream PTY output from Rust to the WebView over a Tauri **`ipc::Channel`**, one
channel per session, with bytes **batched every ~4–16ms** (coalesce reads within a frame) and
delivered as `TerminalChunk { sessionId, seq, base64Bytes }`. Never emit per byte.

**Rationale**: Channels are the throughput-oriented primitive in Tauri 2 and avoid the overhead
of the general event bus for hot paths. Frame-level batching keeps ≥12 noisy terminals from
flooding the bridge and satisfies the "heavy output must not block the UI" / <1-frame-latency
goals (Principle II, performance goals).

**Alternatives considered**: *Tauri global events* — chatty, higher per-message overhead, no
per-session backpressure. *`tauri-plugin-shell`* — **rejected**: exposes command execution to the
frontend, violating Principle I.

---

## 3. PTY — `portable-pty`

**Decision**: Use **`portable-pty`** (the WezTerm crate) for every session.

**Rationale**: Real pseudo-terminal with resize, signal delivery, and raw I/O, so
`claude`/`codex`/`opencode` behave exactly as in a native terminal (Principle II). Cross-platform
support keeps the Windows/daemon door open without re-plumbing.

**Alternatives considered**: *`pty-process`* — thinner, fewer resize/portability guarantees.
*Raw `openpty` + `nix`* — reinvents what `portable-pty` already abstracts. *Non-PTY
`std::process` with piped stdout* — **prohibited** by Principle II (breaks interactivity/TUI).

---

## 4. Terminal renderer — xterm.js + addons

**Decision**: Render each pane with **`@xterm/xterm`** plus addons: **fit** (size to pane),
**webgl** (GPU rendering for many terminals), **unicode11**, **search**, and **web-links**
(link *detection* only — **auto-open disabled**, URLs confirmed before opening, per Principle
III). Exactly one `Terminal` instance per pane, held in a React `ref`, never in React state.

**Rationale**: xterm.js is the proven emulator (used by VS Code's integrated terminal); the WebGL
addon sustains ≥12 live terminals at 60fps. Keeping the instance out of React state avoids
re-render churn and preserves typing latency.

**Alternatives considered**: *Hand-rolled ANSI emulator* — full terminal emulation is large and
error-prone. *Canvas/DOM renderer without WebGL* — degrades under many busy panes.

---

## 5. SQLite driver — `rusqlite` (bundled, FTS5) + `refinery`

**Decision**: Persist state with **`rusqlite`** compiled with the `bundled` feature (ships SQLite
with FTS5), executed on `tokio::task::spawn_blocking`. Run schema migrations with **`refinery`**
(embedded SQL migrations).

**Rationale**: A desktop app's DB access is short and local; a synchronous driver behind
`spawn_blocking` is simpler than an async pool and `bundled` guarantees FTS5 availability for
memory search. `refinery` keeps migrations as plain versioned SQL.

**Alternatives considered**: *`sqlx`* (async, compile-time-checked queries) — heavier build,
needs a runtime DB or offline cache for macros; **viable future swap** if query complexity grows.
*`tauri-plugin-sql`* — exposes SQL to the frontend, weakening the boundary.

---

## 6. Split-tree layout — `react-resizable-panels` + `@dnd-kit`

**Decision**: Render the layout with **`react-resizable-panels`**, mapping our `LayoutNode` tree
1:1 onto nested `PanelGroup` (split) / `Panel` (pane) components. We own persistence of the tree
(our schema + `save_layout` command), not the library's autosave. Use **`@dnd-kit`** to move panes
between regions.

**Rationale**: Nested `PanelGroup`s model arbitrary horizontal/vertical splits — including the
asymmetric wireframe — with built-in drag-to-resize, so we get resizing for free while keeping the
tree as our single source of truth (Principle V).

**Alternatives considered**: *Hand-rolled flexbox splitter* — must reimplement drag handles,
min/max sizes, keyboard resize. *Golden-Layout/dockview* — heavier, opinionated persistence that
fights our schema.

---

## 7. Git — `git2` + `git clone` shell-out

**Decision**: Use **`git2`** (libgit2 bindings) for repository discovery, branch/status/
ahead-behind, and worktree create/list/remove. Shell out to the `git` CLI only for **clone** (to
surface progress).

**Rationale**: `git2` gives structured, fast status without parsing porcelain; worktree APIs are
available through libgit2. Clone is the one long, progress-bearing operation where the CLI's
output is more convenient.

**Alternatives considered**: *Parsing `git status --porcelain`* everywhere — brittle. *Pure CLI
shell-out for all ops* — slower and stringly-typed; also broadens the command surface.

---

## 8. Usage adapters — reimplemented in Rust (`usage-core`)

**Decision**: Reimplement the provider adapters natively in a `usage-core` crate (the
`ai-usagebar` core is monolithic and not a reusable published crate). Support **Claude, Codex,
and OpenCode (→ OpenRouter)** for v1. **One** `UsagePoller` fetches all providers; the whole UI
reads one shared snapshot.

- **Anthropic / Claude**: read OAuth from `~/.claude/.credentials.json`; **fall back to the macOS
  login Keychain** when the file is absent — which is the case on this machine (`.credentials.json`
  is absent). Query the undocumented `api.anthropic.com` usage endpoint.
- **Codex / OpenAI**: read OAuth from `~/.codex/auth.json` (present on this machine). Query the
  undocumented `chatgpt.com/backend-api` endpoint.
- **OpenCode → OpenRouter** (user-confirmed backing provider): API key resolved **env var first,
  then config**; OpenRouter exposes a **documented** credits/balance API.

**Cadence & safety**: **≥300s refresh floor**, **~60s local cache** written atomically with a file
lock (`flock`). The Anthropic and Codex endpoints are undocumented and **rate-limit aggressively
below ~300s**, so polling MUST be centralized (one poll per provider per window — never per
terminal/card, Principle IV). The last good snapshot is persisted for offline / expired-auth
display.

**Rationale**: Native reimplementation gives full control over the card data and avoids depending
on an external binary's JSON contract; centralizing the poller is mandatory given the fragile
endpoints.

**Alternatives considered**: *Shell out to the `ai-usagebar` CLI JSON* — fast to start but adds a
runtime dependency to install/keep updated and constrains the data to its Waybar-shaped output.
*Vendor/fork the `ai-usagebar` modules (MIT)* — a middle path; rejected for v1 to avoid tracking
upstream, but its credential-resolution and cadence logic are the reference.

---

## 9. Login-shell environment resolution (macOS Finder PATH)

**Decision**: On first run, resolve the login-shell environment **once** by invoking the user's
`$SHELL` in login+interactive mode and capturing its environment (e.g. `"$SHELL" -l -i -c
'/usr/bin/env -0'`), then **merge** with known tool paths (`/opt/homebrew/bin`, `~/.local/bin`,
`~/.cargo/bin`, `~/.nvm/versions/node/*/bin`), **cache** the result, and expose it for editing in
settings. Two provider spawn strategies, both native PTY:
- **(A) Direct**: spawn the CLI with the resolved env injected.
- **(B) Login-shell + exec**: spawn `$SHELL -l -i` then `exec <cli>` so `.zprofile`/`.zshrc`
  populate PATH "for free".

**Decision within**: prefer **(B)** for provider panes (env correctness with no separate resolution
hack); **(A)** acceptable where a clean env is desired. The "Shell" pane is (B) without the `exec`.

**Rationale**: Finder-launched apps do not inherit the login shell's PATH; on this machine the
CLIs live in `/opt/homebrew/bin`, `~/.local/bin`, and an nvm path, so unresolved env means
"command not found". `exec` replaces the shell with the CLI, so signals and exit behave cleanly —
no bash wrapper capturing output.

**Alternatives considered**: *Hard-coded PATH* — breaks on nvm/version changes. *Inherit Tauri's
env as-is* — fails exactly the case we must support.

---

## 10. Session resume per CLI

**Decision**: Persist a **per-project session history** record for every launched agent session,
including a **resume reference** (the CLI's own session id / transcript path) when the CLI exposes
one. Clicking a history entry re-spawns the agent with its native resume flag; a **brand-new pane
never passes a resume flag** (always fresh). The live process is not kept alive across app close;
resume relies on the CLI's own stored transcript.

Known (to be confirmed) resume mechanisms:
- **Claude**: `claude --continue` (most recent) and `claude --resume <id>`, with transcripts under
  `~/.claude/projects/`.
- **Codex**: supports resuming a prior session.
- **OpenCode**: supports continuing a session.

> **These exact flags/paths MUST be verified against the installed CLI versions at implementation
> time** (`claude --help`, `codex --help`, `opencode --help`). Do not hard-code an unverified flag.
> When a provider offers no resume capability, the history entry reopens a fresh session in the
> same cwd and indicates resume was unavailable (FR-030).

**Rationale**: Delivers the user's requirement (history + resume-on-click, fresh for new) without a
persistent daemon (deferred), because the CLIs already persist their own transcripts.

**Alternatives considered**: *Keep processes alive in a daemon* — that IS the deferred Phase 10;
unnecessary for resume since the CLIs store transcripts. *No resume (always fresh)* — rejected by
the user's clarification.

---

## 11. Design tokens — ported from `github-visualize` into one Tailwind 4 `@theme`

**Decision**: Port the confirmed `github-visualize` palette **verbatim** into a single Tailwind 4
`@theme` block (`src/styles/theme.css`) as the sole token source (Principle IV):

| Token | Value | Origin |
|---|---|---|
| app background | `#0b0a10` | `bg-[#0b0a10]` on html/body |
| panel | `#0a0a0a` @ 60% (`neutral-950/60`) | card bg |
| border / hover / active | `#262626` / `#404040` / `#a21caf` | neutral-800 / neutral-700 / fuchsia-700 |
| text / strong / muted | `#e5e5e5` / `#fafafa` / `#a3a3a3` | neutral-200 / 50 / 400 |
| accent / strong / bg | `#e879f9` / `#f0abfc` / `#4a044e` @60% | fuchsia-400 / 300 / 950 |
| data pink / cyan | `#f472b6` / `#22d3ee` | additions / removals |
| success / warning / danger | `#34d399` / `#facc15` / `#ef4444` | CI/status |
| font | `ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, monospace` | `font-mono` |

Per-agent color appears **only as small accents** (top strip, status dot, icon, active border),
never filling a pane: Claude → fuchsia, Codex → cyan, OpenCode → violet `#a78bfa`, Shell → neutral.
Low `rounded-lg` corners, thin borders, no large shadows/glow; `prefers-reduced-motion` respected.

**Rationale**: The values are confirmed from the real repo (including the exact `#0b0a10`), so the
app reads as the same design system. One token source prevents visual drift.

**Alternatives considered**: *Re-deriving colors by eye* — drift risk. *Copying `github-visualize`
components* — impossible: that repo is Rails/Stimulus/canvas, no React tree to reuse — only tokens
transfer.

---

## 12. Testing strategy

**Decision**: `cargo test` for Rust unit + integration tests; **`mockito`** for usage HTTP adapters
(record/replay fake endpoints); **`insta`** snapshots for adapter response parsing; **Vitest** +
Testing Library for frontend logic (layout-tree reducers, Zustand stores, LayoutNode ↔ Panel
mapping); and the **`quickstart.md`** scenarios as end-to-end acceptance per phase.

**Rationale**: The fragile external endpoints must be tested against mocks (never live in CI); the
layout tree and its persistence are the highest-risk frontend logic and deserve unit coverage;
runtime behavior (native PTY, ≥12 terminals, restore-after-restart) is only truly validated by
driving the app, hence quickstart scenarios gate each phase.

**Alternatives considered**: *Live smoke tests against provider APIs* — flaky and rate-limited;
keep as an optional manual `make smoke`, not CI.

---

## Open items to verify during implementation

1. **Exact resume flags/paths** per installed CLI version (`claude`, `codex`, `opencode`) — do not
   hard-code before confirming.
2. **Exact usage JSON shapes** and endpoint paths for the undocumented Anthropic and Codex usage
   endpoints (they drift; `insta` snapshots + a manual smoke test catch changes).
3. **Exact macOS Keychain service/account name** used by the Claude CLI for the credentials
   fallback.
4. **OpenRouter balance/credits endpoint** field names for the OpenCode card.
5. **Tauri 2 `ipc::Channel` API** specifics and the current xterm.js addon package names/versions —
   confirm against current docs at implementation time.
6. **libgit2 worktree API** coverage via `git2` vs. needing a `git worktree` shell-out fallback.

None of these block planning or task generation; they are implementation-time confirmations.
