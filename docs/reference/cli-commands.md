---
title: CLI Command Reference
description: Complete reference for all krometrail CLI commands.
---

# CLI Command Reference

The CLI is an alternative interface to the same tooling that agents access via MCP. Every `debug_*` and `chrome_*` MCP tool has a corresponding CLI command. Many agent setups prefer CLI tools for their transparency, composability, and ease of scripting. The CLI is also useful for CI pipelines and convenience operations like `doctor`.

All commands output the viewport to stdout as structured plain text. Exit codes: 0 for success, 1 for errors, 2 for timeouts.

## Command Structure

The CLI uses namespaced subcommands:

```
krometrail debug <command>     # Debug session commands
krometrail browser <command>   # Browser recording commands
krometrail doctor              # Check installed debuggers
krometrail commands            # List all commands (machine-readable)
```

## Global Flags

| Flag | Description |
|------|-------------|
| `--session <id>` | Target a specific debug session (required when multiple sessions are active) |
| `--json` | Output structured JSON instead of plain text |
| `--quiet` | Suppress banners and hints; output viewport only |
| `--version` | Show version |

---

## Session Lifecycle

```bash
# Launch a debug session
krometrail debug launch "<command>" [options]
  --break <file>:<line>              # Set a breakpoint
  --break "<file>:<line> when <cond>" # Conditional breakpoint
  --stop-on-entry                    # Pause on first line
  --language <lang>                  # Override language detection
  --framework <name>                 # Override framework detection
  --config <path>                    # Path to launch.json
  --config-name / -C <name>          # Configuration name in launch.json
  --cwd <path>                       # Working directory

# Examples:
krometrail debug launch "python app.py" --break order.py:147
krometrail debug launch "python -m pytest tests/ -x" --break "order.py:147 when discount < 0"
krometrail debug launch "node index.js" --break src/api.js:30 --language javascript
krometrail debug launch "go test ./..." --break service/order.go:147

# Attach to a running process
krometrail debug attach --port 5678 --language python
krometrail debug attach --pid 12345 --language go

# Check session status
krometrail debug status

# Stop the active (or specified) session
krometrail debug stop
krometrail debug stop --session abc123
```

---

## Execution Control

```bash
# Continue to next breakpoint
krometrail debug continue
krometrail debug continue --timeout 10000

# Step
krometrail debug step over
krometrail debug step into
krometrail debug step out
krometrail debug step over --count 5

# Run to a specific line (temporary breakpoint)
krometrail debug run-to order.py:155
```

---

## Breakpoints

```bash
# Set breakpoints (replaces existing in that file)
krometrail debug break order.py:147
krometrail debug break order.py:147,150,155

# Conditional
krometrail debug break "order.py:147 when discount < 0"

# Hit count
krometrail debug break "order.py:147 hit >=100"

# Logpoint
krometrail debug break "order.py:147 log 'discount={discount}'"

# Exception breakpoints
krometrail debug break --exceptions uncaught
krometrail debug break --exceptions raised
krometrail debug break --exceptions all

# List all active breakpoints
krometrail debug breakpoints

# Clear all breakpoints in a file
krometrail debug break --clear order.py
```

---

## State Inspection

```bash
# Evaluate an expression
krometrail debug eval "<expression>"
krometrail debug eval "cart.items[0].__dict__" --depth 3
krometrail debug eval "request.headers" --frame 2

# Show variables
krometrail debug vars
krometrail debug vars --scope global
krometrail debug vars --scope closure
krometrail debug vars --scope all
krometrail debug vars --filter "^user"
krometrail debug vars --frame 2

# Full stack trace
krometrail debug stack
krometrail debug stack --frames 20
krometrail debug stack --source

# View source
krometrail debug source order.py
krometrail debug source order.py:140-160
```

---

## Session Intelligence

```bash
# Watch expressions (auto-evaluated on every stop)
krometrail debug watch "len(cart.items)" "user.tier" "total > 0"

# Remove watch expressions
krometrail debug unwatch "user.tier"

# Session investigation log
krometrail debug log
krometrail debug log --detailed

# Program output (stdout/stderr)
krometrail debug output
krometrail debug output --stderr
krometrail debug output --since-action 5

# Thread listing
krometrail debug threads
```

---

## Browser Recording

```bash
# Start recording
krometrail browser start --url http://localhost:3000
krometrail browser start --url http://localhost:3000 --framework-state auto
krometrail browser start --url http://localhost:3000 --framework-state react
krometrail browser start --attach                    # Attach to already-running Chrome
krometrail browser start --profile myproject          # Use isolated Chrome profile
krometrail browser start --all-tabs                   # Record all tabs

# Recording status
krometrail browser status

# Place a marker
krometrail browser mark "user submitted form"

# Stop recording
krometrail browser stop
krometrail browser stop --close-browser

# Export session data
krometrail browser export <session-id> --format har --output file.har
```

---

## Browser Session Investigation

```bash
# List sessions
krometrail browser sessions
krometrail browser sessions --has-errors
krometrail browser sessions --url-contains "localhost:3000"

# Session overview
krometrail browser overview <session-id>
krometrail browser overview <session-id> --around-marker <marker-id>
krometrail browser overview <session-id> --include timeline,markers,errors

# Search events
krometrail browser search <session-id> --query "payment failed"
krometrail browser search <session-id> --event-types network_response --status-codes 500
krometrail browser search <session-id> --framework react --pattern stale_closure

# Deep-dive into an event
krometrail browser inspect <session-id> --event <event-id>
krometrail browser inspect <session-id> --marker <marker-id>
krometrail browser inspect <session-id> --timestamp "2025-01-15T10:30:00Z"

# Diff two moments
krometrail browser diff <session-id> --from <timestamp-or-event-id> --to <timestamp-or-event-id>

# Generate reproduction steps
krometrail browser replay-context <session-id>
krometrail browser replay-context <session-id> --format test_scaffold --framework playwright
krometrail browser replay-context <session-id> --format test_scaffold --framework cypress
```

---

## Utility

```bash
# Check installed adapters/debuggers
krometrail doctor

# List all commands (machine-readable)
krometrail commands
krometrail commands --group debug
krometrail commands --group browser

# Version
krometrail --version
```

---

## JSON Output

Every command supports `--json` for structured output:

```bash
krometrail debug continue --json
```

```json
{
	"status": "stopped",
	"reason": "breakpoint",
	"location": { "file": "order.py", "line": 147, "function": "process_order" },
	"stack": [...],
	"locals": { "discount": { "type": "float", "value": "-149.97" }, ... },
	"source": { "file": "order.py", "start_line": 140, "lines": [...] }
}
```
