# Implementation Plan: ai-memory as the Memory Kernel

**Branch**: `002-ai-memory-kernel` | **Date**: 2026-09-03 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-ai-memory-kernel/spec.md`

## Summary

Replace Terminal AI's home-grown memory subsystem with **ai-memory v2.0.2** as the memory kernel: a
loopback server the app supervises as a bundled sidecar — or attaches to when the user already runs
one — reached from Rust through a `MemoryKernel` trait. Reads go over `/api/v1` with `reqwest`;
writes go through the `ai-memory` binary with a fixed argv. The store is ai-memory's own default
location, shared with whatever the user runs outside the app, and projects are named by repository
basename so the panel and the agents converge on the same pages. Legacy entries are imported once,
idempotently and undoably, by explicit user action. Agent wiring (MCP + lifecycle hooks) is written
per project behind preview → diff → consent, recorded, and removed with upstream's own `uninstall`.

## Technical Context

**Language/Version**: Rust 2021 (workspace, tokio) + TypeScript 5 / React 19, unchanged from 001.

**Primary Dependencies**: existing workspace deps (`reqwest` with rustls, `tokio`, `serde`,
`serde_json`, `thiserror`, `tracing`, `chrono`, `uuid`, `rusqlite`, `refinery`, `nix`). One addition:
`sha2` for configuration-file and body hashes. External runtime dependency: the `ai-memory` binary,
pinned to v2.0.2 and shipped as a Tauri `externalBin` sidecar.

**Storage**: the memory source of truth moves **out** of `app.db` into the kernel's git-versioned
wiki at ai-memory's default data location. `app.db` keeps only Terminal AI's own state and gains two
tables (wiring records, migration log) in migration `V005`. The legacy `memory_entries`,
`memory_revisions` and `memory_fts` remain as read-only legacy and are not dropped. Secrets stay in
the macOS Keychain.

**Testing**: `cargo test` (pure unit + `mockito` HTTP + `#[ignore]` sidecar integration), `pnpm test`
(Vitest), and the scenarios in [quickstart.md](./quickstart.md) as the phase gates.

**Target Platform**: macOS 14+, Apple Silicon and Intel.

**Project Type**: desktop app (Tauri 2), monorepo — Cargo workspace + pnpm frontend.

**Performance Goals**: kernel usable ≤5s on a typical start and ≤15s on first run (SC-012); memory
reads ≤300ms p95 on a loopback call; kernel status fetched once per interval no matter how many views
are open (SC-020); UI boot budget of <2s from 001 is unaffected because nothing on the boot path
awaits the kernel.

**Constraints**: loopback-only binding; the WebView never addresses the kernel; the app never
supervises a process it did not start; no secret in `app.db`/`config.toml`; no unannounced network
fetch; no destructive kernel operation on a store the app shares.

**Scale/Scope**: single user, single machine. One new crate, one repurposed-then-deleted crate, one
migration, ~14 new typed commands, one new Zustand store, two new frontend surfaces. Delivery order
US8 → US9 → US10 → US11 → US12.

## Constitution Check

Gate evaluated against constitution **v2.1.0**.

| # | Principle | Gate | Status |
| --- | --- | --- | --- |
| I | Typed Rust Boundary | All kernel access in Rust behind typed commands; no generic execution; every path/provider validated. | ✅ PASS — the WebView never sees the kernel URL; `--cors-allow-origin` is deliberately unused; writes use fixed argv built in Rust, never a frontend-supplied string. |
| II | Native PTY Fidelity | Sessions remain real PTYs. | ✅ PASS — untouched. US13 (`ai-memory run`) is explicitly out of scope for this feature. |
| III | Non-Destructive & Credential-Safe | preview→diff→apply→record→remove-only-created; secrets in Keychain; output untrusted; capture off by default and opt-in per project. | ✅ PASS — FR-055…FR-059 encode the sync discipline and per-project consent; FR-048 keeps kernel content untrusted; FR-061 keeps the token out of `app.db`/argv. Requires new Keychain write support (research §10). |
| IV | Single Source of Truth | One central poller with cache; one design-token source. | ✅ PASS — one supervisor, one status poller, one cached snapshot (FR-042, SC-020). The ≥300s floor is scoped by the principle's own wording to *usage* polling ("Usage MUST be polled once per provider…"); a loopback kernel health check is outside that sentence. The reasoning is kept in Complexity Tracking as a record, not a waiver. |
| V | Layout as a Persisted Tree | Layout unaffected. | ✅ PASS — untouched. |
| VI | Isolation & Resilience | One failure never blocks the UI; concurrent same-repo agents isolated by worktrees. | ✅ PASS — FR-041 makes kernel loss non-blocking; the design rejected the `.ai-memory.toml` marker precisely because it would break `remove_worktree` (research §6). |
| VII | Swappable Session & Memory Hosts | Memory behind a `MemoryKernel` abstraction; supervise only what the app started. | ✅ PASS — the trait lives in `domain` with no IO types; `crates/memory-kernel` is Tauri-free so a future daemon reuses it; FR-039 encodes the ownership rule. |

**Post-Phase-1 re-check**: the design introduces no new violation. The one divergence (the status
poll interval) is recorded below with its argument rather than being silently absorbed.

## Project Structure

### Documentation (this feature)

```
specs/002-ai-memory-kernel/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0 — every decision verified against a running v2.0.2
├── data-model.md        # Phase 1 — V005 tables + the kernel's page model
├── contracts/
│   ├── memory-kernel.md     # the MemoryKernel trait (styled after session-host.md)
│   ├── tauri-commands.md    # the delta to the closed command catalog
│   └── ai-memory-surface.md # the observed external contract, with what is unverified
├── quickstart.md        # Phase 1 — runnable acceptance scenarios K0–K7
└── tasks.md             # produced by /speckit-tasks
```

### Source Code (repository root)

```
crates/
├── domain/
│   └── src/memory.rs          # NEW — MemoryKernel trait + KernelStatus, MemoryPage, Handoff.
│                              #       Zero new deps; no reqwest/serde_json in any signature.
├── memory-kernel/             # NEW crate — everything ai-memory.
│   └── src/{lib,supervisor,http,cli,wiring,scope,migration}.rs
│                              # Deps: domain + reqwest/tokio/serde/thiserror/tracing/sha2.
│                              # NO tauri, NO persistence → reusable by the deferred daemon.
├── memory-manager/            # DELETED. Its FR-024 isolation test is ported to memory-kernel.
├── persistence/
│   ├── migrations/V005__memory_kernel.sql   # NEW — wiring + migration-log tables
│   └── src/dao/mod.rs                       # + MemoryWiringDao, MemoryMigrationDao
└── platform-macos/
    └── src/keychain.rs        # NEW — read/write/delete via /usr/bin/security (stdin, never argv)

src-tauri/
├── src/state.rs               # + kernel: Arc<MemoryKernelSupervisor>, beside usage: Arc<UsagePoller>
├── src/lib.rs                 # supervisor task beside the usage poller; RunEvent::Exit shutdown
├── src/commands.rs            # 5 existing memory commands re-pointed + ~14 new ones
├── tauri.conf.json            # + bundle.externalBin, bundle.resources (upstream LICENSE)
└── binaries/                  # git-ignored; filled by scripts/fetch-ai-memory.sh

src/
├── stores/memory.ts           # NEW Zustand store — kernel status + entries + wiring
├── features/memory/KernelSetup.tsx   # NEW — not-installed / stopped / attached / consent + diff
├── features/memory/MemoryPanel.tsx   # status chip, edit/delete, open a page, handoffs, briefing
└── lib/ipc.ts                 # typed clients for the new commands — still the only channel

scripts/
├── fetch-ai-memory.sh         # NEW — download + verify SHA-256 + place as target-triple sidecar
└── ai-memory.lock             # NEW — pinned version + checksum, the single source of the pin
```

**Structure Decision**: The monorepo layout from 001 is unchanged. The one structural judgement is
the split between `crates/memory-kernel` (pure, Tauri-free, `domain`-only) and the composition root:
DB writes for wiring records and the migration log happen in `src-tauri`, exactly as the usage
poller's snapshot writes already do in `src-tauri/src/lib.rs`, and as `apply_skill` records its
artifacts in `commands.rs`. That keeps `memory-kernel` free of `persistence` and therefore reusable
by the deferred daemon, satisfying Principle VII rather than merely claiming it.

## Complexity Tracking

| Violation | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| Kernel status polled every ~15s (not a violation — recorded for the reader who wonders) | The floor exists for the Anthropic/Codex usage endpoints, which are undocumented and rate-limit aggressively. This is a loopback health check against a process the app itself started; there is no quota and no remote party. A 300s interval would leave the UI claiming a dead kernel is healthy for five minutes, defeating SC-013's 60s requirement. | *Reuse the 300s floor* — violates SC-013 and reports stale state. *Event-driven only* — a crashed child emits no event the app can rely on; liveness needs a probe. The letter of Principle IV that matters here — **one** poller, **one** cached snapshot, never per-card — is fully honoured. |
| An external binary is redistributed inside the app bundle | The user chose a managed sidecar; a first run requiring terminal installation defeats it. | *Detect-and-guide only* — retained as the last step of the resolution order, but not as the default experience. It brings a 29.5 MB bundle cost and an MIT attribution obligation, both accepted and tracked. |
| The app writes into a store it does not exclusively own | Sharing the store with the user's own ai-memory is the point of the feature (research §5). | *A private data directory* — silos the app's memory from the agents' memory. Mitigated by scope: the app performs no destructive kernel operation (`reset`, `purge-project`, `--purge-data` are out of scope) and only ever touches pages it created. |
