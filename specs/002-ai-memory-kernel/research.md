# Phase 0 Research: ai-memory as the Memory Kernel

Every decision below was verified against **ai-memory v2.0.2** running for real on this machine
(macOS aarch64, tarball SHA-256 verified against the published checksum) before being written down.
Where a claim comes from upstream documentation rather than observation, it is marked
**(unverified)**.

---

## 1. Kernel identity and version — `ai-memory` v2.0.2, pinned

**Decision**: Adopt `github.com/akitaonrails/ai-memory` (Rust, MIT) as the memory kernel, pinned to
an exact release the app ships and verifies by SHA-256. Never track `latest`.

**Rationale**: It already implements what feature 001's memory subsystem lacks and what would be
expensive to rebuild: a git-versioned markdown wiki as the source of truth, a derived SQLite index
with FTS5 + entity + graph ranking, typed cross-agent handoff, and an MCP surface that the three
providers Terminal AI launches (`claude-code`, `codex`, `opencode`) already speak. It is MIT and
runs loopback-only with zero LLM configuration.

**Alternatives considered**: *Keep extending `crates/memory-manager`* — the gap is not small: hybrid
retrieval, lifecycle capture and cross-agent handoff are each a project of their own, and none of it
would be visible to the agents. *Vendor the ai-memory crates as a git dependency and host the server
in-process* — couples the build to an unpublished, fast-moving workspace, and the agents need an HTTP
endpoint anyway, so the coupling buys nothing.

---

## 2. Process model — managed sidecar with attach-first probing

**Decision**: The app resolves an `ai-memory` binary, probes the configured address first, and
**attaches** to whatever is already answering there. Only when nothing answers does it spawn
`ai-memory serve` itself. It supervises — restarts, terminates — **only** a process it started.

**Rationale**: Constitution 2.1.0 Principle VII makes the ownership rule law. A user running
ai-memory via Docker, launchd or `mise` must not have it killed when the app quits, and the shared
store (§5) makes that scenario likely rather than exotic.

**Verified**: `serve` binds loopback and logs `auth=false` with no token configured; `SIGTERM` shuts
it down cleanly in under 3s; nothing was left listening afterwards.

**Alternatives considered**: *Attach-only* — a first run that requires the user to install and start
a server in a terminal kills the feature. *Always spawn* — would fight a server the user already
runs, on a store they share.

---

## 3. Detection probe — `POST /mcp` with `tools/list`, never `/api/v1`

**Decision**: Identify a listening process as the memory kernel by `POST /mcp` with
`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`, sending **both** `Content-Type: application/json`
and `Accept: application/json, text/event-stream`, and requiring `result.tools` to contain
`memory_query`. Any other answer is a foreign process: neither attach nor spawn over it.

**Rationale (verified, and this corrected the design)**: `/api/v1` is mounted **only** with
`--enable-web`. Without that flag `GET /api/v1/workspaces` returns **404**. Probing `/api/v1` would
therefore classify a perfectly healthy ai-memory as an unrelated process and lead the app to start a
second server on a busy port. `/mcp` is always mounted.

**Also verified**: omitting either half of the `Accept` header yields
`406 Not Acceptable: Client must accept both application/json and text/event-stream`.

---

## 4. Transport — HTTP reads on `/api/v1`, writes through the CLI

**Decision**: Reads go over `/api/v1` with `reqwest` (the app's spawn passes `--enable-web`). Writes
go through the `ai-memory` binary with a fixed argv (`write-page`, `delete-page`) — never a shell
string. `POST /mcp` `tools/call` is the documented fallback for anything the CLI cannot express.

**Rationale**: `/api/v1` is read-only by construction upstream, so it cannot serve writes. The CLI
is a first-class HTTP client of the same server and is the supported non-agent write path;
sub-process argv is the same mechanism `project-manager` already uses for `git clone`, and it keeps
Principle I intact because the frontend never composes a command. Writes are rare and
human-initiated, so one process per write is acceptable.

**Verified**: `write-page … → ✓ wrote <path> (page_id=…) under <ws>/<proj>`, exit 0.
`search --json` returns `[{path,title,snippet,rank}]`. `status --json` returns a rich, stable
document (`version`, `data_dir`, `bind`, `counts`, `capture_mode`, `client.{server_url,auth}`).
`/api/v1` returns **bare arrays**, not the `{"workspaces":[…]}` envelopes the upstream docs describe,
and a page carries `body_markdown` (not `body`) with a nested `frontmatter` object.

**Verified about the fallback**: `POST /mcp` is **stateless** (`stateful=false` in the boot log);
`tools/call` answered with no `initialize` handshake and no `Mcp-Session-Id`, as
`application/json` rather than SSE. `memory_write_page` takes **`body`**, not `content` — the
upstream docs are wrong. A tool-level failure returns `result.isError = true` rather than a JSON-RPC
`error` object, so a client that only inspects `error` would read every failed write as a success.

**Alternatives considered**: *MCP as the primary write path* — viable and now de-risked, but it
requires reimplementing an envelope whose error semantics have that trap, for no gain over a
supported CLI verb.

---

## 5. Store location — ai-memory's own default, shared

**Decision**: Do not pass `--data-dir`. Let the kernel use its own platform default
(`~/Library/Application Support/ai-memory`), which is also what a user's standalone ai-memory install
uses.

**Rationale**: The entire premise is one brain. A private data directory under `AITerminal/` would
silo the app's memory from the memory the user's own agents accumulate, which is the problem this
feature exists to solve. Sharing also means attaching to a user-run server (§2) is coherent rather
than a split view of two stores.

**Consequence**: the app writes into a store it does not exclusively own. Every destructive
operation (`reset`, `purge-project`, `--purge-data`) is therefore out of scope; the app only ever
creates, updates and deletes the specific pages it created.

**Alternatives considered**: *Private `AITerminal/memory-kernel/`* — safer ownership, but it
defeats unification and would surprise a user who expects the panel to show what their agent just
wrote.

---

## 6. Scope mapping — basename projects, repo-root strategy for worktrees

**Decision**:

| Terminal AI `Scope` | ai-memory `workspace` / `project` | page path prefix |
| --- | --- | --- |
| `global` | `default` / `_global` (via `scope: "global"`) | `terminal-ai/global/` |
| `project P` | `default` / `basename(P.path)` | `terminal-ai/project/` |
| `worktree W of P` | `default` / `basename(P.path)` | `terminal-ai/worktree/<branch>/` |
| `workspace X` | project of X's owner | `terminal-ai/workspace/<id8>/` |
| `session S` | project of S's cwd | `terminal-ai/session/<id8>/` |

The project name is the **repository directory's basename** — exactly what an agent derives from its
working directory with no help. Worktrees are folded into the parent project by installing hooks
with `--project-strategy repo-root`, which bakes the repo-root derivation into the generated hook
commands (upstream resolves it with `git rev-parse --git-common-dir`).

**Rationale**: If the panel wrote to `albert-8f2c1a3b` while the agent wrote to `albert`, the two
would never see each other — the feature would look wired and be useless. Agreeing with the agent's
own derivation is what makes the shared store real. Collisions (two projects whose directories share
a basename) are detected by the app and disambiguated once by the user, rather than being prevented
by a naming scheme the agents do not follow.

**Rejected because it breaks an existing feature**: pinning the project with a `.ai-memory.toml`
marker in the repository root. `worktree-manager::is_dirty` counts any non-ignored status entry, so
an untracked marker makes `remove_worktree` fail with `WorktreeError::Dirty`. `--project-strategy
repo-root` achieves the same result without writing into the user's working tree.

**Isolation, verified**: the same keyword written into `probe-alpha` and `probe-beta` returned only
the requested project when the query carried `workspace`+`project`, and returned **both** when it did
not. An unscoped query is a silent cross-project leak, so scope is a required parameter of the app's
client type, not an option.

---

## 7. Lifecycle capture — project-scoped hooks, off by default

**Decision**: Install hooks into the agent's **project-scoped** configuration
(`--config-file <project>/.claude/settings.json` and each agent's equivalent) rather than its global
one, with `--project-strategy repo-root` and `--no-capture-prompts`. Nothing is installed until the
user consents for that project.

**Rationale**: Constitution 2.1.0 Principle III requires per-project opt-in with informed consent and
"no events at all" from a repository that has not opted in. Project-scoped hooks deliver exactly
that: the hooks only exist inside the projects the user enabled. Installing globally would make one
project's consent silently enable capture for every repository on the machine.

**Verified**: the dry-run emits a complete `{"hooks": {...}}` object — the exact shape a Claude Code
settings file takes — with the target path in a comment, so the app can merge it itself instead of
delegating the merge to `--apply`. With `--no-capture-prompts`, `UserPromptSubmit` disappears from
the emitted set; what remains is `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`,
`PreCompact`, `Stop`, `SubagentStart`, `SubagentStop`.

**Resolved 2026-09-03 (T175), and it narrows the feature.** Only **claude-code** can be wired for
capture the way Principle III requires. **codex** has no automatic hook installation at all — upstream
prints shell scripts for the user to wire by hand. **opencode** installs a *global* plugin that cannot
be scoped to one project. Per the rule set when this risk was identified, the fallback (global hooks +
marker) is **not** used: capture wiring is simply **not offered** for codex and opencode, and the UI
says why. Both still get MCP registration, so their agents can read and write memory on demand — they
just do not capture their own lifecycle. Additionally, `--no-capture-prompts` is claude-code-only
(verified: exit 1 elsewhere), so prompt capture could not have been declined for them anyway.

**Note on `--capture-mode allowlist`**: it is the strictest upstream setting ("a repository without a
marker emits no lifecycle events at all") but it *requires* the `.ai-memory.toml` marker rejected in
§6. Project-scoped installation reaches the same guarantee without a file in the working tree, so the
server default (`denylist`) is acceptable **only** in combination with project-scoped hooks. If an
agent turns out not to support project-scoped hooks, the fallback for that agent is global hooks +
`--capture-mode allowlist` + a consented marker written together with an entry in
`<repo>/.git/worktrees/<name>/info/exclude`. **(the per-agent support for project-scoped hooks beyond
Claude Code is unverified)**

---

## 8. Removal — upstream `uninstall` first, hash-gated restore as fallback

**Decision**: Three tiers, in order. (1) `ai-memory uninstall --only <hooks|mcp> --mcp-url <url>` —
dry-run first, then `--apply`. (2) If the app created the file itself and it still hashes to what the
app wrote, delete it. (3) Otherwise restore the pre-apply backup, **but only** when the current file
still hashes to what the app left; if it drifted, refuse, explain, and hand over the backup path and
a diff.

**Rationale**: The app writes into files it does not own, so "remove only what it created" cannot
mean "delete the file". Upstream's own remover is the most accurate tier, and — verified — it is
dry-run by default, takes `--only hooks|mcp|instructions|skills`, and identifies the MCP entry by
**URL** (`--mcp-url`), never by name alone, which is exactly the precision required. The hash gate on
tier 3 is what prevents clobbering a file the user edited after the wiring was applied.

**Also verified**: for Claude Code, upstream's *recommended* registration is
`claude mcp add --transport http ai-memory <url>` — Claude Code's own API, undone exactly by
`claude mcp remove ai-memory`. Preferred over editing `~/.claude.json` where the CLI is present.

**Verified safe**: running `install-mcp` and `install-hooks` **without** `--apply` wrote nothing —
the four real agent configuration files on this machine were byte-identical before and after.

---

## 9. Embeddings — off by default, opt-in

**Decision**: Start the kernel without an embedding provider. A settings toggle enables hybrid search
and only then allows the model fetch, after telling the user its size.

**Rationale (verified)**: the first `serve` logs *"fetching the default local embedding model in the
background (~87 MB, one time); hybrid search enables on the next start"* and downloads
`all-MiniLM-L6-v2` without asking. That is an unannounced network fetch on first boot, which
FR-062 forbids and which would fail confusingly offline. FTS5 + entity + graph ranking already works
without it — hybrid search is an upgrade, not a requirement.

**Resolved 2026-09-03 (T176)**: `AI_MEMORY_EMBEDDING_PROVIDER=none` — or `embedding_provider = "none"`
in `config.toml`, documented there as *"opt out (FTS + entity + graph only)"*. Verified: with it set,
`models/` stayed at 0 B, no fetch was logged, the status reported the embedding provider `disabled`,
and write + search worked normally. FR-062 is implementable exactly as written: the app starts the
kernel with the opt-out and only removes it when the user enables hybrid search.

---

## 10. Secrets — no token on loopback; Keychain when there is one

**Decision**: Run loopback with no bearer token, matching upstream's default (verified:
`auth=false`). A token is required only to attach to a server that has one; that value lives in the
macOS Keychain, reaches the child process through its environment, and reaches HTTP as a header. It
is never passed on a command line, never written to `app.db` or `config.toml`, and never returned to
the frontend — status reports `hasToken: bool` only.

**Rationale**: Principle III. Passing `--auth-token` to `install-mcp` would also copy the secret into
the agent's own configuration file, which is a leak the app should not create when loopback needs no
token at all.

**Gap found**: `crates/platform-macos` has **no Keychain API** despite `CLAUDE.md` §2 saying it does;
the repo's only Keychain access is a read via `/usr/bin/security find-generic-password` in
`crates/usage-core/src/adapters/anthropic.rs`. Writing is new work. It must not put the secret on
argv (`security add-generic-password -w <secret>` is visible to `ps`): feed it on stdin, or add
`security-framework`.

---

## 11. Distribution — Tauri `externalBin`, pinned and checksum-verified

**Decision**: Ship the binary as a Tauri sidecar (`bundle.externalBin`), fetched at build time by a
script that verifies the published SHA-256 against a checked-in lock file. Resolution order at
runtime: bundled sidecar → configured path → login-shell `PATH` → guided "not installed" state.

**Rationale**: The user chose a managed sidecar, and a first run that requires terminal steps defeats
that. Tauri copies `externalBin` next to the app executable, so `current_exe().parent()` finds it
with no shell plugin — which matters, because the shell plugin is exactly what Principle I keeps away
from the WebView.

**Verified**: the release publishes `ai-memory-macos-aarch64.tar.gz` (13.2 MB) and `-x86_64` with
`.sha256` files; the checksum matched. Extracted, the binary is **29.5 MB** — that, not the tarball
size, is the bundle cost. Downloaded with `curl` it carries no `com.apple.quarantine` attribute and
runs immediately. The tarball also ships upstream's `LICENSE`, which the MIT terms require the app to
redistribute.

**Alternative kept as fallback**: detect-and-guide, which is already the last step of the resolution
order and costs nothing extra.

---

## 12. Testing strategy

**Decision**: Mirror feature 001 §12. Pure unit tests for scope mapping, page-path derivation, argv
construction, error mapping and the supervisor state machine (a pure `transition(state, event)`
function). `mockito` for the `/api/v1` read client, including `{"error":…}` bodies, 401, 404, 500 and
valid-JSON-wrong-shape. `#[ignore]` integration tests that spawn a real sidecar on an ephemeral port
with a temporary data directory. Vitest for the memory store and the wiring consent flow. Quickstart
scenarios as the phase gates.

**Rationale**: The `mockito` + `insta` pattern is already established in `crates/usage-core`. The
supervisor's most dangerous behaviour — never killing a process it did not start — is a pure state
transition and must be unit-tested rather than left to a manual scenario, with the manual scenario as
a second line of defence.

**Ported test**: `project_search_never_crosses_scope` from `crates/memory-manager/src/lib.rs` is the
FR-024 proof and must survive that crate's deletion, re-expressed against the scope mapper.

---

## Open items to verify during implementation

1. ✅ **Resolved (T176)** — `AI_MEMORY_EMBEDDING_PROVIDER=none` (§9).
2. ✅ **Resolved (T175)** — only claude-code supports per-project capture; codex and opencode get MCP
   registration without capture (§7).
3. Whether `ai-memory uninstall --only mcp` removes an entry registered through `claude mcp add`, or
   only one written into a config file directly (§8). **Partially settled**: `uninstall --help`
   confirms it matches by `--mcp-url` and never by name alone, which is the precision the design
   needs; whether it reaches a `claude mcp add` registration is still unobserved. The implementation
   therefore writes MCP through `install-mcp` (whose target `uninstall` certainly knows) rather than
   through `claude mcp add`, so removal has a path that does not depend on the unknown.
4. **Settled**: `serve` holds `<data_dir>/.serve.lock` and logs `single-instance serve lock held`,
   so a second server on the same store is prevented upstream — a stronger guarantee than the app's
   own pidfile, which now only exists to *adopt* an orphan rather than to prevent a duplicate.
5. Where Tauri places `externalBin` (§11). **Dev half settled 2026-09-03**: under `cargo tauri dev`
   it lands at `target/debug/ai-memory`, which `current_exe().parent()` finds — observed in a real
   run, with the kernel spawned from exactly that path. The `.app` bundle half still needs a real
   `tauri build` to confirm `Contents/MacOS/ai-memory`; the resolution order falls back to `PATH`
   and an explicit setting if that assumption is wrong.
6. Behaviour when the shared store is concurrently written by the app and an agent — upstream uses a
   single writer actor, so this should serialize, but it is unobserved (§5).
7. Whether `memory_delete_page` reporting `deleted: true` for a non-existent page (observed) means
   the undo path cannot distinguish "removed" from "was not there" (§8).
