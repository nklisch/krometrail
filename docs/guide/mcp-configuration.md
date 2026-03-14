---
title: MCP Configuration
description: Configure Krometrail as an MCP server for Claude Code, Codex, Cursor, and Windsurf.
---

# MCP Configuration

Krometrail exposes all its capabilities as MCP tools. Once configured, agents discover `debug_*`, `chrome_*`, and `session_*` tools automatically — no prompting required.

## Claude Code

Add to `~/.claude/mcp.json` (global) or `.mcp.json` in your project root (per-project):

::: code-group

```json [bunx]
{
	"mcpServers": {
		"krometrail": {
			"command": "bunx",
			"args": ["krometrail", "--mcp"]
		}
	}
}
```

```json [npx]
{
	"mcpServers": {
		"krometrail": {
			"command": "npx",
			"args": ["krometrail", "mcp"]
		}
	}
}
```

```json [binary]
{
	"mcpServers": {
		"krometrail": {
			"command": "/path/to/krometrail",
			"args": ["mcp"]
		}
	}
}
```

:::

Claude discovers the `debug_*` tools automatically. No CLAUDE.md changes needed.

## Cursor

Add to `~/.cursor/mcp.json`:

```json
{
	"mcpServers": {
		"krometrail": {
			"command": "npx",
			"args": ["krometrail", "mcp"]
		}
	}
}
```

Restart Cursor after saving. The `debug_*` tools will appear in the AI's tool list.

## Windsurf

Add to `~/.windsurf/mcp_config.json` or via the Windsurf settings UI:

```json
{
	"mcpServers": {
		"krometrail": {
			"command": "npx",
			"args": ["krometrail", "mcp"],
			"env": {}
		}
	}
}
```

## OpenAI Codex

Codex works best with the CLI path. Include the skill file in your system prompt:

```bash
# Print the skill file to stdout
krometrail skill
```

Copy the output into your Codex system prompt, or add a shorter reference:

```
You have access to `krometrail` for runtime debugging. Available commands:
- krometrail launch "<command>" [-b file:line]
- krometrail continue / step over|into|out
- krometrail eval "<expression>"
- krometrail vars [--scope local|global]
- krometrail stop
```

## Tool Filtering with `--mcp`

Use the `--mcp` flag to expose only specific tool groups, reducing the agent's tool list:

```json
{
	"mcpServers": {
		"krometrail-debug": {
			"command": "bunx",
			"args": ["krometrail", "--mcp", "--tools", "debug"]
		},
		"krometrail-browser": {
			"command": "bunx",
			"args": ["krometrail", "--mcp", "--tools", "browser"]
		}
	}
}
```

Available tool groups: `debug`, `browser`, `session`, `all` (default).

## Verification

Ask your agent: "What debug tools do you have available?" It should list `debug_launch`, `debug_continue`, `debug_evaluate`, and other tools.

Run `krometrail doctor` in a terminal to confirm which language adapters are installed.
