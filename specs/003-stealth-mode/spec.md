# Feature Specification: Invisible Mode

**Feature Branch**: `003-stealth-mode`

**Created**: 2026-09-03

**Status**: Draft

**Input**: User description: "quero que me ajude a fazer um plano para colocar um botão em
configurações abaixo de testar notificação do macOs para ativar um modo invisivel, que ao
compartilhar tela o terminal não é exibido, nem o icone na dock, como se o app não existisse"

## Clarifications

### Session 2026-09-03

Decisions taken on the user's instruction to proceed with defaults ("pode tomar as decisões, depois
eu corrijo"). Each is recorded here so it can be overturned in one place.

- Q: Does the mode survive an app restart? → A: **Yes** — it is a persisted preference, reapplied at
  launch. Rationale: a mode that silently resets is worse than useless, because the user believes
  they are hidden when they are not.
- Q: Are macOS notifications suppressed while the mode is active? → A: **Yes, dropped, not queued** —
  a notification banner is captured by a screen share, so a mode that leaves banners on does not
  deliver what it promises. Suppressed notifications are not replayed when the mode is turned off.
- Q: Is there a global keyboard shortcut to toggle the mode from outside the app? → A: **Out of
  scope for this feature.** The window stays on screen and clickable while the mode is active, so
  the Settings control is a sufficient way back. Revisit if the window turns out to be reachable
  only through the Dock or the app switcher in practice.
- Q: Is the state global or per project/workspace? → A: **Global to the app.** Hiding one window
  while another stays visible would still reveal the app.
- Q: What happens if the system refuses to apply invisibility? → A: **Fail closed on the claim** —
  the app reports the mode as off and says why, rather than showing an "active" control over a
  window that is still being captured.
- Q: What if only part of the mode applies — hidden from the Dock, say, but still captured? → A:
  **All-or-nothing, with rollback.** A partly applied mode is the worst possible outcome, because it
  looks like it worked. Anything that did apply is undone and the mode reports as off.
- Q: If hiding the app from the menu bar turns out to cost an in-app capability (copy, paste, a
  keybinding, quitting), which wins? → A: **The capability wins.** The app must supply it by another
  means; shipping the mode with a broken clipboard is not an option. This is the requirement most
  likely to be quietly dropped during implementation, so it is stated as a precedence rule, not just
  a constraint.
- Q: Must the app also disappear from the window list another app shows when someone picks what to
  share? → A: **No — capture content only.** That list is shown to the person doing the sharing,
  which is the user themselves, not to the people watching, and no app content appears in the
  capture that results. Requiring it would make the feature unshippable for a leak that does not
  exist. It is documented as a stated limit instead.
- Q: Should the mode turn itself on when a screen share starts? → A: **No, manual only.** Detection
  would become a second source of truth for the state and a new way to be exposed (detected one
  second too late is still exposed). The user asked for a button; automatic activation can be its
  own feature later.
- Q: What is the feature called, given the directory says "stealth" and the user says "invisível"? →
  A: **"Invisible mode" in the artifacts and the code, "Modo invisível" in the interface** — English
  identifiers match the rest of the repository, Portuguese matches the shipped UI. The
  `003-stealth-mode` slug stays as a directory and branch name only.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Disappear before sharing a screen (Priority: P1)

A user is about to share their screen in a call and does not want the agent terminals — the
projects, the prompts, the model output — to be part of what everyone sees. They open Settings and
turn on the invisible mode. From that moment the app is absent from what the other participants
see: the window is not in the shared screen, there is no icon in the Dock, no entry in the
application switcher, no name in the menu bar, and no notification banners. On their own physical
screen the app is unchanged and fully usable.

**Why this priority**: This is the whole feature. Everything else in this spec exists to make this
one moment trustworthy.

**Independent Test**: Turn the mode on, start a screen share (and separately a screen recording),
and confirm the app is absent from what is captured while the user keeps working in it normally.

**Acceptance Scenarios**:

1. **Given** the mode is off and the app window is on screen, **When** the user turns the mode on
   and shares their entire screen, **Then** the recipients see the desktop behind the window and no
   trace of the app, while the user continues to see and use it.
2. **Given** the mode is on, **When** the user is offered a list of windows to share and picks one
   belonging to the app, **Then** the recipients see no content of the app.
3. **Given** the mode is on, **When** the user takes a full-screen screenshot and a region
   screenshot over the window, **Then** neither image contains the app.
4. **Given** the mode is on, **When** an event that would normally raise a macOS notification
   occurs, **Then** no banner appears on screen.
5. **Given** the mode is on, **When** the user looks at the Dock, the application switcher, and the
   menu bar, **Then** the app is in none of them.

---

### User Story 2 - Know it is on, and get back out (Priority: P1)

With the Dock icon and the menu bar gone, the window itself is the only thing that can tell the user
what state they are in. The app shows a persistent indicator while the mode is active, turning it
off restores everything in one action, and restarting the app does not quietly change the state.

**Why this priority**: Also P1, and inseparable from Story 1: a hiding mode the user cannot read or
reverse is a trap. A user who believes the mode is off when it is on loses their app; a user who
believes it is on when it is off loses their privacy.

**Independent Test**: With the mode on, restart the app and confirm from the window alone — without
opening any menu — that the mode is still on; then turn it off and confirm the Dock icon, the
switcher entry, the menu bar, and notifications all come back.

**Acceptance Scenarios**:

1. **Given** the mode is on, **When** the user looks at the app window, **Then** a persistent
   indicator states the mode is active, without any menu being opened.
2. **Given** the mode is on, **When** the user quits and reopens the app, **Then** the mode is still
   on and the indicator says so.
3. **Given** the mode is on at launch, **When** the app starts, **Then** the window never appears in
   any capture — there is no moment, however brief, where it is capturable.
4. **Given** the mode is on, **When** the user turns it off, **Then** the Dock icon, the application
   switcher entry, the menu bar, notification delivery, and normal capture behaviour all return
   without restarting the app.
5. **Given** the mode is on, **When** the user copies text from a terminal, pastes into it, uses any
   configured keybinding, and quits the app, **Then** all of it works exactly as it does with the
   mode off.

---

### User Story 3 - Understand what it does not hide (Priority: P2)

At the point where the user turns the mode on, the app states plainly what it does not do: the
process keeps running and is visible to anyone inspecting the machine, anyone physically looking at
the screen still sees everything, and screen mirroring to an external display or projector is not
screen capture and may still show the window.

**Why this priority**: An overpromise here is the most damaging outcome the feature can have — a
user who trusts the mode further than it goes gets exposed at exactly the moment they were trying
to be careful.

**Independent Test**: Read the text shown at the control and confirm each stated limit against the
running app.

**Acceptance Scenarios**:

1. **Given** the user opens the control, **When** they read the text next to it, **Then** it names
   the running process, the physical screen, and screen mirroring as things the mode does not hide.
2. **Given** the mode is on, **When** someone inspects the machine's running processes, **Then** the
   app is present there, matching what the text said.

---

### Edge Cases

- **Toggled rapidly**: turning the mode on and off several times in a few seconds must converge on
  the last requested state, with no leftover Dock icon, duplicate icon, or window that is hidden
  from the user while the control reads "off".
- **Applied at launch**: if the state is restored at startup, there must be no window frame on
  screen before invisibility is in force.
- **Application refuses, or applies only partly**: whatever did apply is undone, and the app must
  not present the mode as active.
- **Window sent behind other windows**: with no Dock icon and no switcher entry, the app must still
  be reachable; it must not be possible to put it in a state the user cannot get out of.
- **A second window is opened while the mode is on**: it is invisible too, from the moment it
  exists.
- **Notification raised while the mode is on**: it is dropped, and anything that reports on delivery
  says it was suppressed instead of reporting success.
- **Mode on while the app is quitting or crashing**: the next launch reads the persisted state and
  applies it; a crash does not leave the user's Dock or notifications in a half-changed state.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST offer a single control in Settings, directly below the macOS notification
  test action, that turns the invisible mode on and off.
- **FR-002**: The control MUST show the current state at a glance, and MUST show a new state only
  after that state has actually taken effect.
- **FR-003**: While the mode is active, the app's window MUST be absent from every screen capture
  the system produces — sharing the whole screen, sharing a single window, screen recording,
  full-screen screenshots, and region screenshots.
- **FR-004**: While the mode is active, the app MUST NOT be represented in the Dock, in the
  application switcher, or in the menu bar.
- **FR-005**: While the mode is active, the app MUST NOT display macOS notification banners.
  Suppressed notifications MUST be dropped, not queued for later delivery.
- **FR-006**: Any action that reports on notification delivery MUST report suppression explicitly
  rather than reporting success.
- **FR-007**: The mode's state MUST persist across app restarts and MUST be reapplied at launch.
- **FR-008**: When the mode is active at launch, the app window MUST NOT become visible on screen
  until invisibility has been applied.
- **FR-009**: Applying the mode MUST be all-or-nothing. If any part of it cannot be applied, the app
  MUST undo the parts that did apply, report the mode as inactive, and tell the user that applying
  it failed. A partly applied mode MUST NEVER be presented as active.
- **FR-010**: While the mode is active, the app MUST show a persistent indicator inside its own
  window stating that the mode is on, readable without opening any menu.
- **FR-011**: Turning the mode off MUST restore capture visibility, the Dock icon, the application
  switcher entry, the menu bar, and notification delivery, in one action and without restarting the
  app.
- **FR-012**: Enabling the mode MUST NOT remove or degrade any capability the user has inside the
  app, including copying, pasting, every configured keybinding, and quitting the app. Where
  invisibility and an in-app capability conflict, the capability wins: the app MUST provide that
  capability by another means rather than ship the mode without it.
- **FR-013**: While the mode is active, the app MUST remain reachable by the user; it MUST NOT be
  possible to reach a state where the window cannot be brought back into view.
- **FR-014**: The mode MUST apply to every window the app owns, including windows opened while the
  mode is already active.
- **FR-015**: The control MUST state, where the user turns it on, what the mode does not hide: the
  running process on the machine, the physical screen, screen mirroring to another display, and the
  window list another app shows to the person choosing what to share.
- **FR-016**: Repeated toggling MUST converge on the last requested state, leaving no intermediate
  or inconsistent presence in the Dock, the switcher, or the menu bar.
- **FR-017**: The mode's state MUST be a single app-wide preference, stored as an ordinary local
  preference, and MUST NOT be stored as or alongside secret material.

### Key Entities

- **Invisible Mode State**: one on/off preference for the whole app, belonging to the installation
  rather than to a project, workspace, or session. It has no history and no per-scope variants; it
  is read at launch and written whenever the user toggles it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the mode active, the app is absent from 100% of captures across 5 trials of each
  of: sharing the whole screen, sharing a single window, screen recording, full-screen screenshot,
  region screenshot.
- **SC-002**: With the mode active, the app is present in 0 of the Dock, the application switcher,
  and the menu bar.
- **SC-003**: With the mode active, 0 notification banners appear on screen.
- **SC-004**: Turning the mode on or off takes effect within 2 seconds and takes at most 2
  interactions from anywhere in the app.
- **SC-005**: After restarting with the mode active, the number of capturable window frames is 0.
- **SC-006**: A user looking only at the app window can tell whether the mode is on within 3
  seconds, without opening a menu.
- **SC-007**: 0 regressions in what the user can do inside the app with the mode on versus off,
  measured over copy, paste, every configured keybinding, and quitting.
- **SC-008**: After reading the text at the control, a user can name at least 2 things the mode does
  not hide.

## Assumptions

- macOS only. The app is a macOS desktop workspace and this feature has no meaning on other
  platforms.
- The state persists across restarts and is reapplied at launch, so a user who hid the app yesterday
  is still hidden today.
- Suppressed notifications are dropped rather than queued; there is no catch-up delivery when the
  mode is turned off.
- The window remains on screen and clickable while the mode is active, which is what makes the
  Settings control a sufficient way back and lets a global shortcut stay out of scope.
- The mode is app-wide; there is no per-project or per-workspace variant.
- Canonical term: "invisible mode" in the spec, plan, tasks and code; "Modo invisível" in the
  shipped interface, matching the app's existing Portuguese UI. The `003-stealth-mode` directory and
  branch slug is a historical name only and carries no meaning in the product.
- The feature is a privacy affordance, not a security control. It does not encrypt anything, does
  not hide the process from anyone inspecting the machine, and makes no claim to defeat monitoring
  software running on the same machine.

## Out of Scope

- Hiding the app's process from the system's process list, activity monitors, or the file system.
- Renaming or disguising the app, its window title, or its process name.
- Hiding the app from anyone physically looking at the screen, from a camera pointed at it, or from
  screen mirroring to an external display or projector.
- Removing or redacting anything the app has already written to disk, including its own logs, its
  database, and terminal scrollback.
- Removing the app from the list of windows another application offers to the person choosing what
  to share. That list is shown to the user doing the sharing, not to the people watching, and no app
  content appears in the capture that results.
- Turning the mode on automatically when a screen share or a recording starts. The mode is turned on
  and off by the user.
- A global keyboard shortcut to toggle the mode from outside the app.
- Any behaviour on platforms other than macOS.
