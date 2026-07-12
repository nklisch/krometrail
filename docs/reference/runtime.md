---
title: Runtime reference
description: The command surface currently shipped by the Rust Krometrail binary.
---

# Runtime reference

Krometrail is currently a Rust workspace foundation. The executable does not yet launch a browser, record frames, persist sessions, expose MCP tools, or provide a language debugger.

## Commands

| Command | Current behavior |
| --- | --- |
| `krometrail --version` | Prints the Cargo package version and exits successfully. |
| `krometrail --help` | Prints the available command surface and exits successfully. |
| `krometrail doctor` | Fails explicitly with `error[unsupported]` because browser transport is not available. |

The root CLI is defined in [`src/cli.rs`](https://github.com/nklisch/krometrail/blob/main/src/cli.rs), and the composition root is in [`src/app.rs`](https://github.com/nklisch/krometrail/blob/main/src/app.rs). `doctor` must remain a truthful availability check rather than a fake-success placeholder.

## Intended capabilities

The five foundation documents define the contracts that later implementations will consume:

- browser lifecycle and control;
- continuous visual capture and explicit capture gaps;
- recording storage and temporal range queries;
- temporal visual artifacts with provenance;
- MCP tools derived from the capability registry.

Use [`SPEC.md`](../SPEC.md) for intended external behavior and [`ARCHITECTURE.md`](../ARCHITECTURE.md) for implementation boundaries. No command or MCP configuration should be inferred from those future-facing contracts until the corresponding Rust implementation lands.
