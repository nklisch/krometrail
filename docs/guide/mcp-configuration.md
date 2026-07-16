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

The default server exposes four lifecycle tools, 24 browser-control tools from the shared registry, the `temporal_debug_bundle` entry point, seven temporal evidence/retention tools (`list_source_frames`, `fetch_source_frames`, `generate_artifacts`, `generate_region_filmstrip`, `pin_resolved_range`, `unpin_resolved_range`, and `query_pin_state`), and `query_browser_events`. The temporal and browser-event registries generate their input schemas from the same Rust contracts as the services. Every lifecycle launch/attach request accepts optional `every_nth_frame` from 1 through 60, defaulting to 1 and remaining immutable for that browser session.

Temporal results use durable recording data. Tool responses can include bounded inline images and canonical resource links. The server advertises two resource templates and reads retained artifacts and source frames through `resources/read`:

- `krometrail://evidence/{session}/{target}/artifacts/{id}`
- `krometrail://evidence/{session}/{target}/frames/{id}`

Concrete retained resources are discovered from tool responses rather than listed up front. Page-state and framework-state capabilities remain unavailable extension points; they are not part of the default server surface.

The browser boundary includes Chrome-compatible pages and explicitly debug-enabled Electron renderer processes. Electron's Node main process remains outside that boundary. Browser control and captured evidence remain local unless the connected MCP client explicitly reads a response.
