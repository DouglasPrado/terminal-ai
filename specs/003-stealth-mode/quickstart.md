# Quickstart / Acceptance: Invisible Mode

Most of this feature cannot be asserted from inside the process being hidden — a program cannot
prove it is absent from someone else's screen recording. So the automated tests cover the policy and
the wiring, and the acceptance gate below is deliberately manual and observed. A phase is done when
the criterion is **seen**, per the constitution's verification-first rule.

## Prerequisites

```bash
pnpm install
pnpm tauri dev          # or a built bundle for the capture scenarios
```

For the capture scenarios you need a second pair of eyes or a second machine: a video call where
someone else can describe what they see, or a recording you play back afterwards. Judging a screen
share by looking at your own screen proves nothing — your screen is exactly where the window is
still supposed to be.

## Automated gates

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings
cargo test          # includes domain::stealth (cooldown boundaries, transition, rollback order)
pnpm test           # includes the Settings control and the header indicator
```

---

## A. Capture exclusion — SC-001, FR-003

Turn the mode on, then run all five. Five trials each; the pass mark is 5/5.

| # | Do this | Expect |
|---|---------|--------|
| A1 | Share your entire screen in a call | The other participant sees the desktop where the window is. They are asked to describe the area, not to confirm a leading question. |
| A2 | Share a single window and pick the app's | No app content reaches the recipient. The app being *listed by name* in your own picker is expected and is a documented limit, not a failure (Clarification Q3). |
| A3 | Record the screen (QuickTime, or Cmd+Shift+5) | Play the file back: the window is not in it. |
| A4 | Full-screen screenshot (Cmd+Shift+3) | The image does not contain the app. |
| A5 | Region screenshot (Cmd+Shift+4) dragged over the window | The image does not contain the app. |

Then turn the mode **off** and repeat A1 and A3: the window must be present again. A restore that is
never tested is a restore that does not work.

## B. Presence — SC-002, FR-004

With the mode on: the Dock has no icon for the app; Cmd+Tab does not list it; the menu bar shows no
app menu for it. With the mode off: all three are back, without restarting.

## C. Notifications — SC-003, FR-005, FR-006

1. Mode off → Settings → "Testar notificação do macOS" → a banner appears, the UI reports delivery.
2. Mode on → same button → **no banner**, and the UI says it was suppressed rather than reporting
   success.
3. Mode off again → the banner appears again. Nothing that was suppressed arrives late.

## D. Persistence and the launch race — SC-005, FR-007, FR-008

1. With the mode on, quit and reopen the app.
2. The mode is still on and the indicator says so.
3. **Start a screen recording before launching**, then launch. Play it back: there must be no frame
   containing the window — not even one. This is the scenario `"visible": false` exists for
   (research R4); a single visible frame is a failure of the whole feature, not a rough edge.
4. Time the launch, mode on and mode off. The window now starts hidden and is shown explicitly, so
   this feature sits directly on the constitution's **enforced** `UI boot < 2s` budget. Both paths
   must stay inside it; a boot that got slower because the mode is on is a regression, not a cost of
   the feature.

## E. Reachability and capability — SC-007, FR-012, FR-013

With the mode on:

- Copy from a terminal pane and paste into it. **This is the measurement research R6 flags**: if
  Cmd+C/Cmd+V stop working once the menu bar is gone, the mitigation is required, not optional —
  the capability wins over invisibility.
- Exercise every configured keybinding from Settings → Atalhos.
- Quit the app the way a user would, with no Dock icon and no app menu.
- Click another app to cover the window, then get back to the app.

Any of these failing blocks the feature. None of them is a nice-to-have.

## F. Rapid toggling — FR-016

Toggle on → off → on as fast as the UI allows, then wait two seconds and look at the Dock. The final
state must match the control. A Dock icon still sitting there while the control reads "on" is the
cooldown bug in research R3, and means the wait was not honoured.

Then measure SC-004 directly, which convergence alone does not prove: from a settled state, time one
toggle on and one toggle off from click to the Dock and the capture behaviour actually changing. Both
must land inside 2 seconds, including the worst case where a ~1.1s dock cooldown is pending.

## G. Failure path — FR-009

Hard to force naturally. Simulate by making the dock call fail in a debug build and confirm: content
protection is rolled back, nothing is persisted, the control returns to off, and the user is told it
failed. Restarting after this must not restore an "on" state — nothing was written.

## H. Honesty — SC-008, FR-015

Read the text at the control. It must name: the process still running on the machine, the physical
screen, screen mirroring to another display, and the window list a sharing app shows to the person
sharing. A user who reads it should be able to name at least two of them afterwards.

---

## Traceability

| Scenario | Requirements | Success criteria |
|----------|--------------|------------------|
| A | FR-003, FR-011, FR-014 | SC-001 |
| B | FR-004, FR-011 | SC-002 |
| C | FR-005, FR-006, FR-011 | SC-003 |
| D | FR-007, FR-008, FR-010 | SC-005, SC-006 |
| E | FR-012, FR-013 | SC-007 |
| F | FR-016 | SC-004 |
| D4 | — | UI boot < 2s (constitution) |
| G | FR-009 | — |
| H | FR-015 | SC-008 |
