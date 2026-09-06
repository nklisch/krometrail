---
title: Manual MCP configuration
description: Connect a standalone Krometrail binary to an MCP client over local standard input and output.
---

# Manual MCP configuration

If you installed the native Claude Code or Codex plugin, you do not need this page—the plugin already declares and manages Krometrail's MCP server.

Use manual configuration when you installed the standalone `krometrail` command or your MCP client cannot load the native plugin.

## Add Krometrail from the command line

```bash
claude mcp add --scope user krometrail -- krometrail mcp
codex mcp add krometrail -- krometrail mcp
```

Restart or reload the client, then check its MCP status for a connected Krometrail server.

## Generic MCP client entry

```json
{
  "mcpServers": {
    "krometrail": {
      "command": "krometrail",
      "args": ["mcp"]
    }
  }
}
```

The MCP client must be able to find `krometrail` on `PATH`. The default standalone install location is `~/.local/bin/krometrail`.

## Protocol and catalogue expectations

Krometrail supports MCP `2026-07-28` discovery and the `2025-11-25` / `2025-06-18`
initialization handshakes over the same stdio command. Modern clients must follow `tools/list`
`nextCursor` values to obtain every tool; one page is not the complete catalogue.

Tool pages and resource templates may be cached privately for 60 seconds within the same
process/configuration. Discard cached pages and cursors when reconnecting after an update or
configuration change. Updating the plugin does not reload an already running MCP process.

Legacy clients receive the full catalogue in one response, without continuation or modern cache fields; legacy aggregate schema overhead remains.

The concrete resource inventory is intentionally empty. Use exact URIs returned by tools and
resource templates. This process can read retained temporal evidence after browser stop, subject
to retention; range handles and retained-resource access do not survive MCP restart. Downloads
require the managed browser session to stay active.

## Do not run `mcp` as a health check

`krometrail mcp` is a standard-input/output protocol server. It waits for an MCP client and writes protocol traffic—not a user interface—to standard output. Use these checks in a terminal instead:

```bash
krometrail --version
krometrail doctor
```

Krometrail keeps protocol output isolated from diagnostics. Bounded private runtime logs live under
the platform data directory; failed or degraded responses identify the relevant file and correlation
identifier. Startup failures that occur before file logging is available fall back to standard error.

## What the connection gives your agent

The server exposes browser lifecycle and page control, current screenshots and structured snapshots, temporal visual evidence, nearby browser events, and retained artifact/source-frame reads.

One MCP server owns at most one active browser session. Stop or detach from the current session before starting or attaching another one. Ending the MCP transport closes a Krometrail-managed browser or detaches from an externally owned browser.

Browser control and captured evidence stay local unless the connected agent explicitly reads a response or artifact through MCP.

For practical requests, see [Use Krometrail with your agent](using-krometrail.md). For startup failures, see [Troubleshooting](troubleshooting.md).
