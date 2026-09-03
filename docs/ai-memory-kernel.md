# The memory kernel

Terminal AI's memory is not stored by Terminal AI. It is stored by
[**ai-memory**](https://github.com/akitaonrails/ai-memory) — a small Rust server that keeps a
git-versioned markdown wiki and indexes it for search. Terminal AI runs it, talks to it, and shows
it. This document is what you need to know as a user or as someone touching that code.

## Where your memory lives

```
~/Library/Application Support/ai-memory/
├── wiki/    # your memory, as markdown, under git
├── db/      # a rebuildable search index
└── config.toml
```

Note the path: this is **ai-memory's own** directory, not one under `AITerminal/`. That is
deliberate. If you already run ai-memory — through Docker, `mise`, a launch agent — Terminal AI uses
the same store, so what you write in the panel is what your agents read, and vice versa. One brain
was the point of adopting it.

The consequence is that Terminal AI is a guest in that store. It creates, updates and deletes the
pages it created, under a `terminal-ai/` prefix, and it never runs a destructive whole-store
operation (`reset`, `purge-project`, `--purge-data`). If you want those, run them yourself.

## Running it

The app ships the kernel as a sidecar binary and starts it on launch. Three things worth knowing:

- **If something is already listening**, Terminal AI attaches to it instead of starting a second
  one — and then it will never stop or restart it, including when you quit the app. The status chip
  says "servidor externo" when that is what is happening.
- **The port is loopback only.** A non-loopback URL is refused outright.
- **No token is used by default**, because a loopback server does not need one. If you attach to a
  server that does require one, it is stored in the macOS Keychain and never written to `app.db`,
  `config.toml`, or any agent's configuration file.

The pinned version lives in [`scripts/ai-memory.lock`](../scripts/ai-memory.lock), with its SHA-256.
`scripts/fetch-ai-memory.sh` downloads and verifies it. Never point this at "latest": the kernel's
response shapes are a contract we observe rather than own, and an unpinned upgrade moves them
underneath a running app. Bumping the pin means re-running the contract probe recorded in
[`research.md`](../specs/002-ai-memory-kernel/research.md).

## Search, and the 87 MB you were not asked about

The kernel can rank with local embeddings, and on its first start it fetches a ~87 MB model in the
background to do so — without asking. Terminal AI starts it with that turned off
(`AI_MEMORY_EMBEDDING_PROVIDER=none`), so a first run reaches no network you did not ask for. Search
still works: full text, entities and the link graph. Turning on hybrid search in the memory panel is
what authorises the download, and the size is stated before you agree to it.

## Connecting your agents

Two separate things, with different blast radii:

| | what it does | where it is written |
|---|---|---|
| **Access** (MCP) | lets an agent read and write memory when it chooses to | the agent's own config, globally |
| **Capture** (hooks) | records the agent's session activity automatically | that project's config only |

Access is offered for Claude Code, Codex and OpenCode. **Capture is offered only for Claude Code** —
Codex has no automatic hook installation at all, and OpenCode's hooks install as a machine-wide
plugin. Neither can be confined to one project, and installing machine-wide capture behind a
per-project consent would be dishonest, so it is not offered and the panel says why.

Nothing is written until you have seen the diff and confirmed. What was written is recorded, with a
hash of the result and a backup of what was there before. Removing it uses the kernel's own
uninstaller where possible, and otherwise restores the backup — **unless the file has changed since**,
in which case Terminal AI refuses and hands you the backup path rather than overwriting an edit you
made.

## Your project's name

The kernel identifies a project by its **directory basename**, because that is what an agent working
in that directory derives on its own. It is what makes the panel and the agent agree.

Two consequences worth knowing:

- Two projects whose folders share a name (`work/api` and `personal/api`) look like one project to
  the kernel. The app detects that and asks you to disambiguate.
- **Renaming a project's folder re-points it.** Old memory stays in the kernel under the old name;
  it just stops appearing in the panel. The app notices and tells you rather than showing an empty
  panel.

Worktrees are folded into their parent repository's project, so agents in sibling worktrees share
memory instead of fragmenting it. Terminal AI never writes a file into a worktree to achieve
this — an untracked file there would make the worktree undeletable, which
`crates/worktree-manager` has a test for.

## Migrating from the old store

Feature 001's memory lived in `app.db` and markdown under `AITerminal/memory/`. That data is not
touched. The panel offers to import it, shows you what would happen first, and running it twice
imports nothing the second time. It can be undone, and the originals stay on disk either way.

## If something is wrong

The status chip and its banner say what state the kernel is in and what to do about it. The states
that need you: **não instalada** (reinstall, or point Settings at a binary), **porta ocupada** (a
non-kernel process holds the port), and **indisponível** (it failed to start five times; the log
under `~/Library/Logs/` and Settings → restart are the next steps). A quarantined binary is detected
specifically and tells you the `xattr` command to run.

Losing the kernel never takes the app down. Terminals keep running, projects keep working, and only
the memory panel shows a banner.
