# Deferred product work

The following Phase-10 items are deliberately outside the v1 process model and remain tracked for
a release-focused follow-up:

1. A per-user daemon that owns PTYs so sessions survive closing or upgrading the GUI.
2. Apple Developer ID signing, hardened runtime entitlements, and notarization in CI.
3. A signed auto-update feed with staged rollout, rollback, and schema compatibility checks.

These are not hidden behind incomplete flags in v1. The current in-process host terminates with the
app, local development builds are unsigned, and updates are manual. Each follow-up requires its own
threat model and acceptance plan before implementation.

## Reattach after a real WebView reload (FR-032 follow-up)

FR-032 (2026-07-15) avoids the terminal-wipe on reload by intercepting `Cmd/Ctrl+R` and refreshing
only the sidebar in place. A _real_ WebView reload (app menu, a crash, or Vite HMR in dev) still
drops the frontend's per-pane `sessionId`s, so restored panes fall back to the provider picker while
the in-process PTYs keep running (orphaned). Making any reload non-destructive is deferred: it needs
(1) persisting `sessionId` per pane in the `panes` table + layout contract, (2) reconciling live
sessions onto restored panes on mount via `list_sessions`, and (3) an `attach_session` command that
re-points a live session's output to a fresh `ipc::Channel` (today the sender is baked into the
spawn closure in `host.rs`). Scrollback backfill already works via `get_scrollback`.

## `projects.trusted` column (V001)

Project trust was removed from the product (constitution 2.0.0). The column stays in migration
V001 as `INTEGER NOT NULL DEFAULT 0` and is no longer read or written — inserts simply omit it.
Dropping it would mean a destructive migration against users' existing `app.db` for no functional
gain. Fold it into the next migration that has to rewrite the `projects` table for another reason.
