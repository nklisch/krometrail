---
id: idea-mcp-signal-shutdown
created: 2026-07-19
updated: 2026-07-19
tags: [bug]
---

The MCP server does not exit on SIGINT or SIGTERM — Claude Code escalates to
SIGKILL on every shutdown. Observed in the plugin MCP connection logs during the
2026-07-19 plugin-install debugging session:

> "Sending SIGINT to MCP server process" → "SIGINT failed, sending SIGTERM" →
> "SIGTERM failed, sending SIGKILL"

The documented contract says the server exits cleanly on stdin EOF; signal-driven
shutdown should also terminate gracefully so hosts don't have to hard-kill the
process and store recovery paths aren't exercised on every disconnect.
