---
layout: doc
title: "Krometrail — Rust browser capture foundation"
titleTemplate: false
---

# Krometrail

Krometrail is a Rust foundation for local browser control and temporal visual evidence for coding agents.

The current executable is intentionally small:

```bash
cargo run -- --version
cargo run -- --help
cargo run -- doctor
```

The first two commands work. `doctor` reports that browser transport is not yet available. Browser capture, storage, temporal analysis, and MCP are intended capabilities described by the five foundation documents, not commands exposed by this build.

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
