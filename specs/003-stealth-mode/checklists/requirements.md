# Specification Quality Checklist: Invisible Mode

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-09-03
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Passed on the first validation pass; no spec rewrites were needed.
- The Dock, the application switcher and the menu bar are named in the requirements. They are the
  user-facing surfaces of the operating system the feature is about, not implementation choices —
  the spec never says how the app disappears from them.
- Ten decisions were taken by the assistant rather than asked, on the user's instruction, and are
  recorded in `## Clarifications` so any of them can be overturned in one place: persistence across
  restarts, dropping (not queuing) notifications, no global shortcut, app-wide scope, failing closed
  on the "active" claim, all-or-nothing application with rollback, in-app capability winning over
  invisibility, the share picker's window list being a stated limit rather than a requirement, no
  automatic activation, and the canonical name.
- Constitution check for planning: Principle I (typed boundary — the toggle is a boolean, never a
  command string), Principle III (the state is an ordinary preference, never secret material; the
  UI must state the mode's real limits rather than overpromise), Principle IV (the indicator uses
  the single design-token source). FR-012 and FR-013 are the two requirements most at risk of being
  dropped during implementation, and the ones the acceptance run must exercise hardest.
