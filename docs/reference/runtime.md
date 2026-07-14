---
title: Runtime reference
description: The command surface currently shipped by the Rust Krometrail binary.
---

# Runtime reference

Krometrail ships one Rust binary and one composition root. The runtime assembles browser transport, controlled-browser capture, recording storage, and retention before dispatching a command.

## Commands

| Command | Current behavior |
| --- | --- |
| `krometrail --version` | Prints the Cargo package version and exits successfully. |
| `krometrail --help` | Prints the available command surface and exits successfully. |
| `krometrail doctor` | Reports discovered Chrome/Chromium installations or a structured `browser_not_found` failure. It does not launch a browser. |
| `krometrail mcp` | Serves the browser-control tool registry over MCP stdio until transport EOF or process shutdown. |

`krometrail mcp` writes only MCP JSON-RPC traffic to standard output. It owns at most one controlled browser session and uses ownership-aware shutdown: managed browsers close, while explicitly attached browsers detach. The current MCP surface provides four lifecycle tools and 24 registry-derived control operations, including ordered batching.

Temporal investigation tools, browser-event inspection tools, durable MCP resources, and unavailable page/framework-state capabilities are not part of the current command. Current screenshots are returned as MCP image content.

The root CLI is defined in [`src/cli.rs`](https://github.com/nklisch/krometrail/blob/main/src/cli.rs), and the full composition root is in [`src/app.rs`](https://github.com/nklisch/krometrail/blob/main/src/app.rs). See [MCP configuration](../guide/mcp-configuration.md) for a client entry and [`SPEC.md`](../SPEC.md) for the broader intended external contracts.
