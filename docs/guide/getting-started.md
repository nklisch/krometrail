---
title: Getting Started
description: Install Krometrail and connect it to your AI coding agent in minutes.
---

# Getting Started

Krometrail is an MCP server and CLI that gives AI coding agents eyes into running applications. It records browser activity — network requests, console output, DOM mutations, framework state, and screenshots — and lets agents search, inspect, and diff recorded sessions. It also bridges the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP) for breakpoint-level debugging across 6 languages.

## Prerequisites

- **Bun** (recommended) or **Node.js 18+**
- For debugging: language-specific debuggers (check with `krometrail doctor`)

## Install

::: code-group

```bash [bunx (no install)]
# Run directly without installing
bunx krometrail --version
```

```bash [npm global]
npm install -g krometrail
krometrail --version
```

```bash [npx (no install)]
npx krometrail --version
```

:::

## MCP Configuration

Add Krometrail to your agent's MCP config to expose all tools automatically:

```json
{
	"mcpServers": {
		"krometrail": {
			"command": "bunx",
			"args": ["krometrail", "--mcp"]
		}
	}
}
```

The agent will discover `debug_*`, `chrome_*`, and `session_*` tools automatically — no further setup needed.

## Verify the Installation

Check that debuggers are available for the languages you need:

```bash
krometrail doctor
```

This shows each supported language adapter, whether its debugger is installed, and the installed version.

## First Steps

- **Browser observation** — [Start recording a session](../browser/recording-sessions) and investigate what happened
- **Runtime debugging** — [Launch a debug session](../debugging/overview) against a Python, Node.js, or Go program
- **Agent configuration** — [Set up Claude Code, Codex, or Cursor](./mcp-configuration)
