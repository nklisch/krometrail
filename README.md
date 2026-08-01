# Krometrail

[![MCP Toplist](https://mcptoplist.com/badge/glama%2Fnklisch%2Fkrometrail.svg)](https://mcptoplist.com/server/glama%2Fnklisch%2Fkrometrail)

**Browser memory for coding agents.**

A screenshot shows your agent what a page is. Krometrail preserves what the page did—flicker, reversed motion, transient layout shifts, hydration flashes, focus jumps, and incorrect canvas frames that disappear before the next screenshot.

Krometrail controls and records a local Chrome-compatible browser, then turns a selected interval into still-image evidence your agent can inspect: storyboards, difference maps, focused filmstrips, motion history, and the underlying source frames.

[Read the documentation](https://krometrail.dev) · [Install](https://krometrail.dev/guide/installation) · [Use it with your agent](https://krometrail.dev/guide/using-krometrail)

## Install for Claude Code or Codex

The native plugin supplies the Krometrail skill, MCP connection, and matching managed binary.

```bash
# Claude Code
claude plugin marketplace add nklisch/krometrail --scope user
claude plugin install krometrail@krometrail --scope user

# Codex
codex plugin marketplace add nklisch/krometrail
codex plugin add krometrail@krometrail
```

Restart or reload your agent after installation. The first activation downloads and verifies the release paired with the plugin; later starts use the verified local copy.

## Install the standalone command

Use the standalone command for terminal checks or manual MCP configuration on Linux or macOS:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
krometrail --version
krometrail doctor
```

Then register it with an MCP client if needed:

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

## Ask your agent

> Use Krometrail to reproduce the settings-panel animation. It sometimes moves backward before it settles. Inspect the temporal evidence around that interaction, show me the relevant source frames, and tell me whether capture gaps limit the conclusion.

Krometrail preserves evidence; the coding agent still interprets it, finds the responsible code, and verifies the fix.

## Local by default

Browser control, recording, captured frames, browser events, and generated artifacts run locally. Krometrail does not send session contents or telemetry to an external service by default. A connected agent reads evidence through local MCP only when it needs it.

Recording is bounded by a configurable disk budget. Krometrail reports known capture gaps and does not claim to capture every browser-rendered frame.

## Develop Krometrail

Krometrail is a Rust 2024 workspace with a Rust 1.85 minimum supported version. Run the quality gate from the repository root:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the development binary through Cargo:

```bash
cargo run -- --help
cargo run -- doctor
```

See the [development guide](https://krometrail.dev/guide/development) for workspace, documentation, release, and fixture commands. The authoritative product and architecture documents remain under [`docs/`](docs/agents.md).

## License

MIT
