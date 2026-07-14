---
title: MCP Configuration
description: Configure Krometrail browser control over local MCP stdio.
---

# MCP configuration

Krometrail exposes its browser-control surface through the `mcp` command and standard input/output transport. A typical MCP client entry is:

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

The client must be able to find the `krometrail` executable. Standard output belongs exclusively to MCP JSON-RPC traffic; startup and runtime failures are reported on standard error.

The server owns at most one active browser session. Call `start_browser` to launch a managed Chrome-compatible browser or `attach_browser` with an explicitly enabled local CDP endpoint. Use `stop_browser` before starting or attaching another session. Ending the MCP transport also closes a managed browser or detaches from an externally owned browser through the same bounded shutdown path.

The current server exposes the four lifecycle tools and the 24 browser-control operations derived from the shared control registry. It does not yet expose temporal investigation, browser-event inspection, durable resources, page-state, or framework-state tools. Screenshots and post-action images are returned directly as MCP image content; no unreadable resource URI is advertised.

The browser boundary includes Chrome-compatible pages and explicitly debug-enabled Electron renderer processes. Electron's Node main process remains outside that boundary. Browser control and captured evidence remain local unless the connected MCP client explicitly reads a response.
