---
layout: doc
title: "Krometrail — Rust browser capture foundation"
titleTemplate: false
---

# Krometrail

Krometrail is a Rust foundation for local browser control and temporal visual evidence for coding agents.

The current executable provides browser discovery and the local MCP browser-control boundary:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
cargo run -- mcp
```

`doctor` reports discovered browser installations without launching. `mcp` serves the registry-derived browser-control tools over protocol-only stdio and keeps controlled-browser capture and recording assembled behind the same runtime. Temporal investigation tools remain future-facing contracts in the foundation documents.

## Start here

- [Installation guide](guide/installation) — current source installation and guarded future release installs.
- [Development guide](guide/development) — build, test, lint, run, release, and docs tooling.
- [Documentation navigation](agents) — source-of-truth order and current contributor pages.
- [Runtime reference](reference/runtime) — the current command contract.
- [MCP configuration](guide/mcp-configuration) — current MCP status and Electron renderer boundary.
- [Configuration](reference/configuration) — current configuration status.

## Foundation documents

- [Vision](VISION)
- [Specification](SPEC)
- [Architecture](ARCHITECTURE)
- [Visual evidence](VISUAL-EVIDENCE)
- [Evaluation](EVALUATION)
