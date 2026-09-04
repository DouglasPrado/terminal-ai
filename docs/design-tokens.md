# Design tokens

All visual tokens live in `src/styles/theme.css` under Tailwind 4's `@theme` block — the single
source (Principle IV). Components use the semantic utilities (`bg-panel`, `border-border`,
`text-text-muted`, `text-ui`, `rounded-control`, `shadow-glow`) and never a raw hex.

## Direction

Night-city duotone: a cool violet-blue neutral ramp with magenta leading and cyan answering. Neon
is treated as **emitted light, not fill** — a tight 1px ring plus a short bloom (`--shadow-glow`),
never a saturated block of color. It is reserved for live state: the focused pane, the active
workspace tab, the primary action, a repo with a running agent. Everything idle stays near-black
with hairlines, so a wall of terminals never competes with its own chrome.

## Surfaces

`app #08070d` (terminal ground) → `elevated #0c0b13` (sidebar, tab bar) → `panel #100e18` (cards,
pane headers) → `raised #171423` (controls) → `raised-hover #201c30`. Hairlines run
`border-subtle #171425` → `border #241f36` → `border-hover #3b3357`, with `border-active #c026d3`.

## Type

Two families with clear roles: `--font-ui` (SF Pro / system sans) for chrome — labels, buttons,
titles — and `--font-mono` (SF Mono) for data, HUD readouts and terminal output. Four sizes plus a
readout size: `meta 12` · `ui 13` · `title 14` · `heading 17` · `readout 11` (uppercase mono).

## Shape and texture

One radius family: `chip 4` · `control 6` · `panel 10` · `modal 14`. Controls come in two heights
(28px `sm`, 32px `md`). Two CSS utilities carry the texture: `.scanlines` (a ~2% CRT wash, applied
to chrome only — never over the terminal, which must stay crisp) and `.hud-grid` (a faint
wireframe for surfaces waiting on input, such as an unbound pane).

## Provider identity

Per-agent color stays an accent — brand mark, status dot, active border — and never fills a pane:
Claude → fuchsia, Codex → cyan, OpenCode → violet `#a78bfa`, Shell → neutral. The marks themselves
are the official SVGL logos (`claude-ai-icon`, `codex`, `opencode`), inlined in
`src/lib/providers.tsx` because the CSP forbids remote assets, and drawn in `currentColor` so each
one takes its provider's token: three brands whose own palettes are two whites and a clay would be
indistinguishable at 14px on a near-black ground.

xterm renders through canvas/WebGL and cannot resolve CSS variables, so its theme repeats the same
concrete values plus a balanced 16-color ANSI palette. `prefers-reduced-motion` is respected.
