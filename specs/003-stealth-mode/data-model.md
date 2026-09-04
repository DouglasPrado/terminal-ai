# Phase 1 Data Model: Invisible Mode

The feature stores exactly one value. Most of this document is therefore about the *rules* around
that value, not its shape.

## Entity: Invisible Mode

| Attribute | Type | Default | Notes |
|-----------|------|---------|-------|
| `enabled` | boolean | `false` | App-wide. Not scoped to a project, workspace, session or window (spec: Key Entities, Clarification Q4). |

**Persistence**: one row in the existing `settings` key/value table of `app.db`, written through
`SettingsDao`, exactly as `memory_auto_capture` is today.

- key: `invisible_mode`
- value: JSON `true` / `false`
- absent row ⇒ `false`. A missing or unparsable value degrades to `false`, never to `true`: the app
  must never believe it is hidden because a read failed.

**Not stored**: whether each individual OS switch succeeded. That is derived state, recomputed on
every apply. Storing it would create the second source of truth Principle IV forbids.

**Never**: this row is an ordinary preference and MUST NOT hold, or sit alongside, credentials
(FR-017, Principle III).

## Derived state (in memory, never persisted)

| Name | Type | Lives in | Purpose |
|------|------|----------|---------|
| `dock_cooldown` | `Mutex<DockCooldown>` | `AppState` | Feeds the hide wait; see research R3. Updated on every dock visibility change the app makes. |
| `invisible_mode_gate` | `tokio::sync::Mutex<()>` | `AppState` | Serializes changes. Held across the whole apply, cooldown wait included, so two toggles cannot both compute a wait against a stale instant. |

## Domain types (`crates/domain/src/invisible_mode.rs`)

```rust
/// The whole feature's state. A newtype rather than a bare bool so the transition
/// rules below have somewhere to live and something to be tested against.
pub struct InvisibleMode { pub enabled: bool }

/// Pure arithmetic for tao's one-second dock debounce plus the margin the measured
/// failure forced (research R3). `remaining(now)` is how long a hide must wait to
/// not be silently dropped.
pub struct DockCooldown { last_show: Option<Instant> }
```

Both are IO-free and unit-tested in place. `DockCooldown` takes `now` as an argument rather than
reading the clock, so its behaviour at the boundary (0ms, 999ms, 1000ms, 1001ms) is testable without
sleeping.

## State transitions

```text
            ┌──────────────── turn on (all three switches apply) ───────────────┐
            │                                                                   ▼
      ┌──────────┐                                                        ┌──────────┐
      │   OFF    │◀─── turn off (all three restore, one action, FR-011) ───│    ON    │
      └──────────┘                                                        └──────────┘
            ▲                                                                   │
            └──── apply failed: roll back whatever applied, stay OFF (FR-009) ──┘
```

**Apply order (on)** — chosen so rollback is always possible:

1. content protection on every window (cheap, synchronous, individually reversible)
2. await the dock cooldown if one is pending (research R3)
3. dock visibility off
4. persist `invisible_mode = true`
5. refocus the main window

If step 1 fails for any window: undo the windows already protected, persist nothing, report failure.
If step 3 fails: undo step 1, persist nothing, report failure. **The value is persisted only after
the OS state is in force** — so a restart can never restore a mode that never applied.

**Apply order (off)** is the reverse, and is best-effort in the opposite direction: every switch is
attempted even if an earlier one fails, because a partial *restore* leaves the user more visible, not
less, and stopping halfway would strand them.

## Invariants

1. The persisted value and the OS state agree, or the persisted value is `false`. There is no third
   state.
2. The window is shown at startup regardless of whether applying the mode succeeded (research R4).
3. Every window the app owns carries the same protection, including windows created while the mode is
   already on (FR-014).
4. Suppressed notifications are dropped, never queued (Clarification Q2 of the specify session).

## Contract surface delta

`AppSettings` and `SettingsPatch` each gain one field. Full shape and validation:
[contracts/tauri-commands.md](./contracts/tauri-commands.md).
