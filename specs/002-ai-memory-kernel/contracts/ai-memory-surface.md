# Contract: the `ai-memory` external surface (observed, v2.0.2)

This is the **external** contract Terminal AI depends on. Unlike the other contracts in this folder,
Terminal AI does not own it and cannot change it — so every line is marked with how it is known.

- ✅ **observed** — exercised against a running v2.0.2 on 2026-09-03 and the response recorded.
- ⚠ **documented** — stated by upstream docs, not yet exercised. Treat as a risk until observed.

**Pin**: `v2.0.2`, `ai-memory-macos-aarch64.tar.gz`, SHA-256
`1b7113614c76f6d38d80e39d9657806e6621a3cd20eeddd0c1927eadacc1d0c6` ✅. The pin lives in
`scripts/ai-memory.lock` and is the only place a version is written.

## Process

| | |
| --- | --- |
| Start ✅ | `ai-memory serve --transport http --bind 127.0.0.1:<port> --enable-web` |
| Global flags ✅ | `--data-dir <path>`, `--config <path>` — **before** the subcommand |
| Data dir ✅ | Defaults under the platform data dir. Terminal AI passes **no** `--data-dir`, so the store is shared (research §5). |
| Init ✅ | `ai-memory init` creates `wiki/ raw/ db/ models/ logs/` and a default `config.toml`. `serve` also runs wiki migrations and creates a git checkpoint on first start. |
| Loopback default ✅ | Boot log reports `auth=false` with no token; `allowed_hosts = ["localhost","127.0.0.1","::1"]`. |
| Shutdown ✅ | `SIGTERM` exits cleanly in under 3s and frees the port. |
| First-run fetch ✅ | Downloads `all-MiniLM-L6-v2` (~87 MB) in the background, unprompted, and enables hybrid search on the **next** start. Suppression flag ⚠ **unidentified** — research §9 open item 1. |
| Quarantine ✅ | A `curl`-downloaded binary carries only `com.apple.provenance`; it runs with no Gatekeeper prompt. |

## Detection

```
POST /mcp
Content-Type: application/json
Accept: application/json, text/event-stream      ← BOTH required ✅ (else 406)

{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
→ 200 application/json, result.tools = 18 tools ✅
```

`/mcp` is always mounted ✅. **`/api/v1` is mounted only with `--enable-web`; without it every route
returns 404** ✅ — which is why detection must not use it.

## Reads — `/api/v1` (requires `--enable-web`)

All observed responses are **bare arrays**, not the `{"workspaces":[…]}` envelopes the upstream docs
describe ✅.

| Route | Observed shape |
| --- | --- |
| `GET /api/v1/workspaces` ✅ | `[{workspace_name, project_count, page_count, last_updated}]` |
| `GET /api/v1/projects` ✅ | `[{workspace_name, project_name, page_count, last_updated}]` |
| `GET /api/v1/search?q=&workspace=&project=&limit=` ✅ | `[{workspace, project, path, title, kind, snippet, rank}]` — `snippet` contains `<mark>` HTML |
| `GET /api/v1/workspaces/{w}/projects/{p}/pages/{*path}` ✅ | `{workspace, project, path, title, kind, tier, pinned, created_at, updated_at, supersedes, frontmatter, body_markdown, links, backlinks}` |
| `GET …/pages`, `…/recent`, `…/briefing`, `…/overview`, `…/handoffs?state=` ⚠ | documented; shapes unverified |

**Scope is not optional.** A search without `workspace`+`project` returns pages from every project ✅
— demonstrated with the same keyword in two projects. The typed client makes scope a required
parameter so this is unrepresentable.

**`body_markdown`, not `body`** ✅, and `frontmatter` is a nested object ✅.

## Writes — the CLI

| Command | Observed |
| --- | --- |
| `write-page --path --body [--title --kind --tag --tier --pinned] --workspace --project` ✅ | `✓ wrote <path> (page_id=…) under <ws>/<proj>`, exit 0 |
| `delete-page --path --workspace --project` ✅ | Reports `deleted: true` **even for a page that does not exist** — the response cannot distinguish "removed" from "was not there" |
| `search <query> --workspace --project -n --json` ✅ | `[{path,title,snippet,rank}]` |
| `read-page [query] --path --workspace --project --json` ⚠ | documented |
| `status --json` ✅ | `{version, data_dir, bind, db_path, counts{pages_latest,pages_all,sessions,observations}, derived{…}, storage{…}, providers{llm,embedding}, spool{…}, capture_mode, ingest{…}, client{server_url,auth}}` |

`AI_MEMORY_SERVER_URL` points client subcommands at a non-default port ✅.

## Writes — `/mcp` (documented fallback)

Stateless ✅ — boot log says `stateful=false`; `tools/call` answered with **no `initialize`
handshake** and no `Mcp-Session-Id`, as `application/json` rather than SSE.

```
POST /mcp   {"jsonrpc":"2.0","id":3,"method":"tools/call",
             "params":{"name":"memory_write_page",
                       "arguments":{"path":"…","body":"…","workspace":"…","project":"…"}}}
→ {"jsonrpc":"2.0","id":3,
   "result":{"content":[{"type":"text","text":"{\"page_id\":…,\"path\":…,\"checkpoint\":…}"}],
             "isError":false}}
```

**Two traps, both observed:**
1. The argument is **`body`**, not `content` — passing `content` yields
   `-32602 missing field 'body'` ✅. The upstream docs are wrong.
2. A **tool** failure arrives as `result.isError = true`, not as a JSON-RPC `error` object. A client
   that only inspects `error` reads every failed write as a success. The response funnel must check,
   in order: transport → HTTP status → `error` → `result.isError` → payload shape.

Observed `memory_write_page` schema ✅: required `path`, `body`; optional `title`, `tier`, `tags[]`,
`pinned`, `project`, `workspace`, `scope`, `expires_at`. `scope: "global"` targets the reserved
`_global` scope. `memory_query` additionally accepts `scopes[]`, `global`, `include_expired`,
`explain`, `as_of` ✅.

## Wiring

| Command | Observed |
| --- | --- |
| `install-mcp --client <c> [--server-url --name --auth-token --config-file --session-aware] [--apply]` ✅ | Without `--apply`: prints a plan and **writes nothing** — the four real agent config files on this machine were byte-identical before and after. Emits a `{"mcpServers":{"ai-memory":{"type":"http","url":…}}}` snippet, and recommends `claude mcp add --transport http ai-memory <url>` as the preferred route for Claude Code. |
| `install-hooks --agent <a> [--config-file --project-strategy --capture-mode --no-capture-prompts --capture-assistant --profile] [--apply]` ✅ | Without `--apply`: prints a complete `{"hooks":{…}}` object plus the target path in a comment, and writes nothing. With `--no-capture-prompts`, `UserPromptSubmit` is absent; the remaining events are `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, `PreCompact`, `Stop`, `SubagentStart`, `SubagentStop`. |
| `uninstall [--apply] [--only hooks\|mcp\|instructions\|skills] [--mcp-url --mcp-name --purge-data --yes]` ✅ (help) | Dry-run unless `--apply`. Help states: *"Uninstall never matches by name alone; when this is set, the entry must match both name and `--mcp-url`."* Whether it removes an entry registered via `claude mcp add` is ⚠ unverified. |

**The dry-run text is not a machine contract.** It is human-readable output with an embedded JSON
block. The app extracts the JSON block for the *content*, but computes the **diff itself** by reading
the target file before and after into a temporary copy. If the text format changes, the app degrades
to "cannot preview; apply disabled" rather than applying something it could not show.

### Per-agent capture support ✅ (observed 2026-09-03 — T175)

This is the finding that decides how far the capture feature can go, so it is recorded in full.

| Agent | Automatic hook installation | Scope of capture | Verdict for Terminal AI |
| --- | --- | --- | --- |
| **claude-code** ✅ | Yes — emits a complete `{"hooks":{…}}` object for a settings file the app can target with `--config-file`. | **Per project** when written to `<project>/.claude/settings.json`. | **Capture supported.** `--no-capture-prompts` works here and is the app's default. |
| **codex** ✅ | **No.** Output is literally *"codex hook scripts (manual install — wire each to the matching event)"* followed by a list of `.sh` paths. There is no configuration file to merge. | n/a | **Capture unavailable from the app.** MCP registration only. |
| **opencode** ✅ | Yes, but as a **global** TypeScript plugin at `~/.config/opencode/plugins/ai-memory.ts`, which the installer *overwrites* on every re-run (keeping a `.bak-<ts>`). | **Machine-wide.** Not scopeable to a project. | **Capture unavailable from the app** under a per-project consent model. MCP registration only. |

`--no-capture-prompts` / `--capture-prompts` ✅ **require `--agent claude-code`**; the command exits 1
for any other agent, explaining that other agents use their prompt hook to deliver handoff context,
so removing it would break cross-agent continuity. Prompt capture is therefore **not optional** for
codex and opencode — another reason their capture is not offered.

For codex, `--project-strategy repo-root` is delivered as `AI_MEMORY_PROJECT_STRATEGY=repo-root` in
each hook script's environment rather than baked into a command ✅.

**Single-instance lock** ✅: `serve` holds `<data_dir>/.serve.lock` and logs
`single-instance serve lock held`. A second server on the same data directory is prevented upstream,
which is a stronger guarantee than the app's own pidfile.

**Embedding opt-out** ✅: `AI_MEMORY_EMBEDDING_PROVIDER=none` (or `embedding_provider = "none"` in
`config.toml`) suppresses the model fetch entirely — verified: `models/` stayed at 0 B, the status
reports the embedding provider `disabled`, and write + search still work on FTS5 + entity + graph.

**Hook commands bake absolute paths** ✅ — both the binary path and `--data-dir` are written into
each hook command. If the sidecar moves (an app update), hooks break silently. The applied binary
path is therefore recorded and compared at startup (`stale` status, FR-059).

`--project-strategy repo-root` ⚠ bakes repo-root project derivation into the generated hooks
(upstream resolves it via `git rev-parse --git-common-dir`), which is how worktrees fold into the
parent project **without** writing a `.ai-memory.toml` into the working tree.

## Scoping

- Default workspace `default`, default project `scratch` ✅; project otherwise derived from the
  working directory's basename ⚠.
- Active-project isolation defaults to `PerActor` with `session_ttl_secs=3600`, `max_entries=4096` ✅.
- `capture_mode` defaults to `denylist` ✅; `allowlist` requires a `.ai-memory.toml` marker ⚠ — which
  this feature rejects (research §6), preferring project-scoped hook installation.

## What Terminal AI will never call

`reset`, `purge-project`, `purge-session`, `compact`, `uninstall --purge-data`, `move-project`,
`rename-project`. The store is shared with the user (research §5), so destructive whole-store or
whole-project operations are out of scope for the app. It creates, updates and deletes only the pages
it created.
