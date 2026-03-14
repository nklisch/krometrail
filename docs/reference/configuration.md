---
title: Configuration Reference
description: Viewport configuration, resource limits, MCP server options, and CLI flags.
---

# Configuration Reference

## Viewport Configuration

Passed as `viewport_config` to `debug_launch` or `debug_status`. All parameters are optional.

```json
{
	"command": "python app.py",
	"viewport_config": {
		"source_context_lines": 15,
		"stack_depth": 5,
		"locals_max_depth": 1,
		"locals_max_items": 20,
		"string_truncate_length": 120,
		"collection_preview_items": 5
	}
}
```

<!--@include: ../.generated/viewport-config.md-->

Smaller values reduce tokens per stop. The defaults target ~300–400 tokens for typical programs.

## Resource Limits

Server-enforced safety limits. These prevent runaway sessions from consuming excessive system resources or agent context.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `session_timeout_ms` | `300000` | Max wall-clock time for a session (5 min) |
| `max_actions_per_session` | `200` | Max debug actions before forced termination |
| `max_concurrent_sessions` | `3` | Per-agent concurrent session limit |
| `step_timeout_ms` | `30000` | Max wait time for a single stop event |
| `max_output_bytes` | `1048576` | Max captured stdout/stderr (1 MB) |
| `max_evaluate_time_ms` | `5000` | Max time for expression evaluation |

When a limit is hit, the server returns a structured error with the limit name, current value, and a suggestion.

## Launch Configuration

See the full `debug_launch` parameter table in the [MCP Tools Reference](./mcp-tools).

## Breakpoint Configuration

```typescript
interface Breakpoint {
	line: number;            // Required
	condition?: string;      // Expression that must be true to trigger
	hit_condition?: string;  // Numeric condition on hit count, e.g., ">=100"
	log_message?: string;    // Log instead of breaking. Supports {expr} interpolation
}
```

## MCP Server Flags

When running as MCP server (`krometrail --mcp` or `krometrail mcp`):

| Flag | Description |
|------|-------------|
| `--mcp` | Run as MCP server (stdio transport) |
| `--tools <group>` | Comma-separated tool groups to expose: `debug`, `browser`. Default: all |

## CLI Flags

| Flag | Description |
|------|-------------|
| `--session <id>` | Target session (required when multiple active) |
| `--json` | Output JSON instead of plain text |
| `--quiet` | Output viewport only, no chrome |
| `--timeout <ms>` | Override default timeout for this command |

## Daemon Configuration

The CLI session daemon starts automatically. Configure via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `KROMETRAIL_SOCKET` | `$XDG_RUNTIME_DIR/krometrail.sock` | Unix socket path |
| `KROMETRAIL_IDLE_TIMEOUT` | `300000` | Daemon shutdown after idle (ms) |

Fallback socket: `~/.krometrail/krometrail.sock` when `XDG_RUNTIME_DIR` is not set.
