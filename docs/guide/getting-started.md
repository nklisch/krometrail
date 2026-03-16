---
title: Getting Started
description: Install Krometrail and connect it to your AI coding agent in minutes.
---

# Getting Started

Krometrail gives your coding agent runtime context it can't get from source code alone. Record a browser session while you reproduce a bug, drop a marker, and hand it off — your agent gets the network requests, console errors, framework state, and screenshots it needs to investigate. It also bridges the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP) so your agent can set breakpoints, step through code, and inspect variables across 10 languages.

## Prerequisites

- For debugging: language-specific debuggers (check with `krometrail doctor`)
- **Bun** or **Node.js 18+** only needed if using `bunx`/`npx` instead of the standalone binary

## Install

Install the standalone binary — no Node.js or Bun required:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

Or run without installing via a package runner:

::: code-group

```bash [bunx (no install)]
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
