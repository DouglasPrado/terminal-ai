# Acceptance record — 2026-09-03 (feature 002: ai-memory kernel)

What was actually observed, and what was not. The scenarios are those in
[`specs/002-ai-memory-kernel/quickstart.md`](../specs/002-ai-memory-kernel/quickstart.md).

**Read the third column carefully.** Roughly half of these are proven by automated tests run today;
the rest need the app on screen and a human driving it, and are recorded as **not yet run** rather
than assumed. A green build is not an acceptance run.

| Scenario | Result | Evidence |
|---|---|---|
| **K0 — Kernel boots** | **Pass (automated + observed in a real run)** | `crates/memory-kernel/tests/sidecar.rs::a_cold_start_fits_the_budget` spawns the real pinned binary on an ephemeral port with a fresh data directory and asserts readiness inside SC-012's 15s cold budget; a warm probe returns in under 1s. Whole `--ignored` suite: 1.43s. **Also observed in an actual `pnpm tauri dev` run**: the supervisor resolved the sidecar from `target/debug/ai-memory` (where Tauri places `externalBin` in dev), spawned it with exactly the argv the code builds — `serve --transport http --bind 127.0.0.1:49374 --enable-web` — and wrote `AITerminal/runtime/ai-memory.json`. The running server answered `tools/list` with 18 tools including `memory_query`, and `/api/v1/workspaces` returned 200, which is the proof `--enable-web` really was passed. |
| **K1 — Attach, and leave alone** | **Partially proven** | The decision half is exhaustively unit-tested: `supervisor::tests::terminate_is_unreachable_for_any_unowned_state` walks all 9 states × 17 events and asserts `Terminate` is never emitted with `owned == false`, and `quitting_the_app_stops_only_our_own_kernel` covers the exit path. **Not yet run:** the end-to-end version — start a server by hand, launch and quit the app, confirm the server is still listening. |
| **K2 — Foreign port & missing binary** | **Partially proven** | `probe::tests::another_mcp_server_is_a_stranger_not_a_kernel` and `a_plain_web_server_is_a_stranger` cover the dangerous case (something that speaks MCP but is not the kernel). `supervisor::tests::a_stranger_on_the_port_neither_attaches_nor_spawns` covers the reaction. **Not yet run:** the UI states for `portConflict` and `notInstalled`. |
| **K3 — Resilience** | **Partially proven** | `stores/memory.test.ts::subscribes exactly once no matter how many views ask` is the SC-020 half. The supervisor's degrade/backoff/recovery path is unit-tested (`repeated_failures_stop_retrying_instead_of_looping`, `recovery_resets_the_failure_count`). **Not yet run:** killing the kernel with 6 live panes and observing that nothing else is affected. |
| **K4 — Migration** | **Pass (automated)** | `migration::tests` — `a_second_run_imports_nothing` (SC-016), `an_interrupted_run_resumes_where_it_stopped`, `the_page_path_is_stable_even_with_the_log_lost`, `a_changed_body_is_re_imported`, `unresolvable_and_empty_entries_are_reported_not_dropped`, `undo_removes_only_what_was_imported`. **Not yet run:** against a real legacy `app.db` with ≥50 entries. |
| **K5 / K5b — Wiring with consent** | **Partially proven** | The removal-safety decision is unit-tested: `wiring::tests::a_file_edited_after_we_wrote_it_is_never_clobbered` proves an edited file is refused rather than restored over, and `merging_into_a_file_with_no_backup_is_refused_rather_than_guessed` covers the other unsafe case. Dry-run safety was verified by hand against the real binary during the contract probe: `install-mcp` and `install-hooks` without `--apply` left all four real agent config files byte-identical. **Not yet run:** the full apply → remove → byte-compare cycle through the UI. |
| **K6 — Agents share the memory** | **Core claim proven; last mile still open** | See "The K6 probe" below. Both derivations were exercised against the **real shared store** with the **real fixture projects**, in both directions, and they agree. What is still not observed is an actual Claude process, launched by the app in a wired project, reaching the kernel over MCP. |
| **K7 — Isolation & worktrees** | **Pass (automated)**, one half outstanding | SC-014 is proven twice: `scope::tests::project_search_never_crosses_scope` (the test ported from the deleted `memory-manager`) and `sidecar.rs::a_scoped_search_never_crosses_projects`, which writes the same word into two projects on a **real server** and asserts a scoped query returns only one. The worktree constraint has its own regression test in `worktree-manager::an_untracked_file_makes_a_worktree_undeletable`. **Not yet run:** writing memory from inside a live worktree. |
| **K8 — Handoff** | **Not yet run** | Requires a wired Claude session to end and a Codex pane to open. The listing path is implemented; nothing has produced a real handoff yet. |
| **K9 — No surprise network** | **Pass (manual + observed in production)** | Verified during the contract probe: with `AI_MEMORY_EMBEDDING_PROVIDER=none`, `models/` stayed at **0 B**, no fetch appeared in the log, status reported the embedding provider `disabled`, and write + search worked normally. The app passes that flag unless hybrid search is switched on. **Confirmed in the real run**: the shared store at `~/Library/Application Support/ai-memory/` has **no `models/` directory at all** after the app started the kernel, and sits at 3.7 MB. Nothing was downloaded. |
| **K10 — Stale wiring** | **Partially proven** | `wiring::tests::a_moved_sidecar_marks_wiring_stale` covers the detection. **Not yet run:** moving the binary and seeing the UI say so. |

## Observed in a real application run (2026-09-03, ~17:15)

Three things were put to the application rather than to a test, and one of them did not get answered:

- **Orphan adoption (FR-043) — works.** A kernel from an earlier run was still listening on 49374
  after its app had gone. Launching the app again produced **one** `ai-memory serve` process, not
  two, and left `runtime/ai-memory.json` untouched — same pid, same `started_at`. The app adopted
  the orphan instead of starting a second server against a store that already had one. This is the
  case the pidfile exists for, and it is no longer hypothetical.
- **Sidecar resolution in dev — works.** Tauri copies `externalBin` to `target/debug/ai-memory`
  under `cargo tauri dev`, and `current_exe().parent()` finds it there with no shell plugin. That
  settles the dev half of research open item 5; the `.app` bundle half still needs a real build.
- **Clean shutdown (`RunEvent::Exit`) — still unvalidated.** The app was stopped with `pkill`, which
  does not exercise Tauri's exit event, so the "terminate the kernel we own" path was not observed.
  The kernel is still running, which under those circumstances is correct rather than a bug — but it
  means K1's end-to-end half remains open. It needs a real quit from the window.

## The K6 probe (2026-09-03, ~17:30)

K6's real question is narrow: **does the project name an agent derives from its working directory
match the one the app's scope mapper produces?** If it does not, the panel and the agents write into
different places, everything still "works", and the feature is useless. That can be answered without
the GUI, so it was.

Against the running kernel and the real store, using the fixture projects from `quickstart.md`:

| Step | Result |
|---|---|
| Write from `~/www/albert` with **no** `--project`, as an agent's cwd derivation would | `✓ wrote … under default/albert` — the same `default`/`albert` the app's mapper produces for that path |
| Read it back with **explicit** `workspace=default&project=albert`, which is exactly what `AiMemoryKernel::search` and `read` send | found, with the body intact |
| Write from the app's side with explicit scope | `✓ wrote … under default/albert` |
| Search it from `~/www/albert` with **no** scope, as an agent would | found |
| Write the same keyword into `~/www/dashboard` from its own cwd, then search `albert` scoped | **only `albert` returned** — SC-014, now against the real store with real directories |
| The same search **unscoped** | both projects returned — which is precisely why the typed client makes scope required rather than optional |

All three probe pages were deleted afterwards; the store is back to 0 pages. The empty `albert` and
`dashboard` project registrations remain, which is what using memory for those projects would have
created anyway.

**What this does not prove**: that a Claude process launched by the app, in a project whose wiring
has been applied, actually reaches the kernel over MCP and reports the same counts. That needs the
wiring written into the user's own agent configuration and a pane open, so it stays open.

## Automated gates, run today

```
cargo fmt --all --check                              OK
cargo clippy --workspace --all-targets -- -D warnings  0 errors
cargo test --workspace                               83 passed, 0 failed
cargo test --workspace -- --ignored                  3 passed (real sidecar)
cargo build -p terminal-ai                           links
tsc --noEmit / eslint --max-warnings 0 / prettier    clean
vitest run                                           13 passed
vite build                                           built
```

## What this record does not say

It does not say the feature is accepted. Five scenarios still need the application running and a
person watching it. **K6's core claim is no longer among them** — the two derivations were shown to
agree against the real store, in both directions — but its last mile is: a Claude process launched by
the app, talking to the kernel over MCP it was wired into. Until that is seen, "the panel and the
agents share one brain" is proven at the addressing layer and assumed at the transport layer.

K1's decision logic is exhaustively unit-tested, but its end-to-end half — quit the app, confirm an
attached server survives and an owned one does not — has still not been driven.

Two upstream behaviours also remain unverified and are recorded as open items in
[`research.md`](../specs/002-ai-memory-kernel/research.md): whether Tauri places `externalBin` where
the binary resolver expects it in a real `.app` bundle, and how the kernel behaves when the app and
an agent write to the shared store at the same moment.
