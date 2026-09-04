# Provider setup

Terminal AI resolves the macOS login-shell environment once and augments `PATH` with common
Homebrew, local, and Cargo locations. Built-in commands are `claude`, `codex`, `opencode`, and
`/bin/zsh -l`. Missing commands are disabled in the picker with an actionable detection result.

## Authentication and usage

- Claude CLI authentication is read from `~/.claude/.credentials.json`, with the
  `Claude Code-credentials` macOS Keychain item as fallback.
- Codex authentication is read from `~/.codex/auth.json`.
- OpenCode's usage card uses OpenRouter. Set `OPENROUTER_API_KEY`, or configure the key at
  `provider.openrouter.options.apiKey` in `~/.config/opencode/opencode.json`.

Credentials are read in memory and never copied into Terminal AI's database or logs. Anthropic
and Codex usage endpoints are not public contracts, so failures retain the last good snapshot and
show stale or re-authentication state.

## Custom providers

Custom profiles contain a label, executable, arguments, color, and non-secret environment values.
The executable must resolve through the cached login-shell `PATH`. The allowed-root check gates custom
agents, and profiles must not be used to store tokens.

Native resume is supported where the CLI exposes it: Claude (`--continue`/`--resume`), Codex
(`resume`), and OpenCode (`--continue`). A fresh pane always starts a fresh session.
