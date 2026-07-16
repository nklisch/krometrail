# Documentation Navigation

## Authoritative foundation

Read these five documents first. They define Krometrail's current direction and intended system contracts:

- **[VISION.md](VISION.md)** — product thesis, audience, boundaries, and success criteria.
- **[SPEC.md](SPEC.md)** — externally observable browser, recording, capability, and failure contracts.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Rust workspace, component boundaries, data flow, storage, and failure isolation.
- **[VISUAL-EVIDENCE.md](VISUAL-EVIDENCE.md)** — temporal artifact vocabulary, provenance, and interpretation rules.
- **[EVALUATION.md](EVALUATION.md)** — capture, artifact, browser-control, and agent-effectiveness validation.

The foundation documents intentionally describe capabilities that are being built in stages. The current executable state is documented in [the development guide](guide/development.md) and the [runtime reference](reference/runtime.md); do not turn an intended capability into a command example until its implementation exists.

## Public documentation

- [Installation](guide/installation.md) — choose the Claude Code/Codex plugin or standalone binary path.
- [Use Krometrail with your agent](guide/using-krometrail.md) — practical prompts, evidence choices, and fix-verification workflow.
- [Troubleshooting](guide/troubleshooting.md) — plugin activation, browser discovery, MCP, permission, and storage recovery.
- [Manual MCP configuration](guide/mcp-configuration.md) — connect a standalone binary to an MCP client.
- [Command reference](reference/runtime.md) — the installed binary's command surface.
- [Configuration reference](reference/configuration.md) — current environment and per-session capture settings.

## Contributor documentation

- [Development](guide/development.md) — Rust build, test, lint, run, release, and docs-tooling commands.
- [Browser fixtures](https://github.com/nklisch/krometrail/blob/main/tests/fixtures/browser/README.md) — retained target applications and their current uses.

## Versioned technology research

- [Rust CDP transport — 2026-07](research/rust-cdp-transport-2026-07.md) — source-grounded comparison of `cdpkit`, `chromey`, and an owned transport, plus the real-browser selection gate.

Documentation is part of the current repository contract. Remove or update a page when the executable or workspace no longer supports its claims; do not preserve old runtime instructions as compatibility guidance.
