---
title: Command reference
description: Commands provided by the installed Krometrail binary.
---

# Command reference

Krometrail provides four command-line entry points:

| Command | What it does |
| --- | --- |
| `krometrail --version` | Prints the installed version. |
| `krometrail --help` | Lists the available command surface. |
| `krometrail doctor` | Finds local Chrome and Chromium installations without launching a browser. Returns a structured `browser_not_found` error when none is available. |
| `krometrail mcp` | Serves Krometrail to an MCP client over standard input and output until the client disconnects or the process stops. |

## `doctor`

Use `doctor` as the terminal health check for browser discovery:

```bash
krometrail doctor
```

It does not launch a browser, change a profile, or start recording. Doctor never
initializes, reclaims, validates, or reads recording storage or recording
configuration: an unusable data directory or an invalid recording setting
cannot block discovery. Diagnostic logging stays on and stays best effort:
discovery is appended to the data directory's diagnostics log when that
location is writable, and an unwritable location only produces a warning on
standard error.

## `mcp`

An MCP client starts this command for you:

```text
krometrail mcp
```

It is not an interactive terminal command. Standard output is reserved for MCP protocol messages.
Krometrail writes bounded private diagnostics under its platform data directory and returns the log
path plus a correlation identifier on failed or degraded MCP calls. Standard error is reserved for
startup failures that occur before file diagnostics are available.

The same stdio command supports MCP `2026-07-28` discovery and initialization at
`2025-11-25` or `2025-06-18`. Modern tool discovery is paginated; follow every `nextCursor`.
Legacy versions receive the complete catalogue in one response.

The MCP server gives an agent:

- managed browser launch or explicit local attachment;
- page navigation, inspection, interaction, and current screenshots;
- continuous recording during the controlled session;
- temporal bundles, storyboards, difference maps, filmstrips, motion history, and source frames;
- nearby console, exception, navigation, and request metadata;
- local retention and pinning of important intervals.

The server owns at most one active browser session. A Krometrail-managed browser closes when the session or MCP transport ends; an externally owned browser is detached rather than closed.

See [Manual MCP configuration](../guide/mcp-configuration.md) to connect a standalone binary.
