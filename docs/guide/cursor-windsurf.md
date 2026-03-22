---
title: "Using Krometrail with Cursor and Windsurf"
description: "Set up Krometrail with Cursor, Windsurf, and other MCP-compatible editors."
---

# Using Krometrail with Cursor and Windsurf

Both Cursor and Windsurf support MCP servers. Krometrail integrates via MCP to give the AI runtime debugging.

## Cursor Setup

Add to your Cursor MCP configuration at `~/.cursor/mcp.json` (global) or `.cursor/mcp.json` in your project root (per-project):

::: code-group

```json [binary]
{
  "mcpServers": {
    "krometrail": {
      "command": "krometrail",
      "args": ["--mcp"]
    }
  }
}
```

```json [npx]
{
  "mcpServers": {
    "krometrail": {
      "command": "npx",
      "args": ["krometrail@latest", "--mcp"]
    }
  }
}
```

```json [bunx]
{
  "mcpServers": {
    "krometrail": {
      "command": "bunx",
      "args": ["krometrail@latest", "--mcp"]
    }
  }
}
```

:::

Restart Cursor after saving the config. The `debug_*` tools will appear in the AI's tool list.

## Windsurf Setup

Add to your Windsurf MCP configuration at `~/.codeium/windsurf/mcp_config.json` (or click the MCPs icon in the Cascade panel and select "Configure"):

::: code-group

```json [binary]
{
  "mcpServers": {
    "krometrail": {
      "command": "krometrail",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```

```json [npx]
{
  "mcpServers": {
    "krometrail": {
      "command": "npx",
      "args": ["krometrail@latest", "--mcp"],
      "env": {}
    }
  }
}
```

```json [bunx]
{
  "mcpServers": {
    "krometrail": {
      "command": "bunx",
      "args": ["krometrail@latest", "--mcp"],
      "env": {}
    }
  }
}
```

:::

## Verification

Ask the AI assistant:

> What debugging tools do you have access to?

It should mention `debug_launch`, `debug_continue`, `debug_evaluate`, and other `debug_*` tools.

Run a quick test:

> Launch a debug session for this Python file and show me the variables at line 10.

## Project-Level Configuration

For team setups, add a `.mcp.json` file to your project root:

```json
{
  "mcpServers": {
    "krometrail": {
      "command": "krometrail",
      "args": ["--mcp"]
    }
  }
}
```

This lets all team members use krometrail without individual configuration.

## Known Limitations

- **MCP transport**: Krometrail uses stdio transport, which is supported by both Cursor and Windsurf.
- **Session persistence**: Debug sessions persist as long as the MCP server process runs. If Cursor/Windsurf restarts the MCP server, active sessions are lost.
- **Port allocation**: Krometrail allocates local ports for debugger connections. Ensure ports 4000–5000 are not blocked by firewall rules.

## Checking Adapter Status

In a terminal:

```bash
krometrail doctor
```

This confirms which language debuggers are installed (Python, Node.js, Go, Rust, Java, C/C++).
