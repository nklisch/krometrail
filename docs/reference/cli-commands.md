---
title: CLI Command Reference
description: Complete reference for all krometrail CLI commands.
---

# CLI Command Reference

All commands output the viewport to stdout as structured plain text. Exit codes: 0 for success, 1 for errors, 2 for timeouts.

## Global Flags

| Flag | Description |
|------|-------------|
| `--session <id>` | Target a specific session (required when multiple sessions are active) |
| `--json` | Output structured JSON instead of plain text |
| `--quiet` | Suppress banners and hints; output viewport only |
| `--version` | Show version |

---

## Session Lifecycle

```bash
# Launch a debug session
krometrail launch "<command>" [options]
  --break <file>:<line>              # Set a breakpoint
  --break "<file>:<line> when <cond>" # Conditional breakpoint
  --stop-on-entry                    # Pause on first line
  --language <lang>                  # Override language detection
  --framework <name>                 # Override framework detection
  --config <path>                    # Path to launch.json
  --config-name / -C <name>          # Configuration name in launch.json
  --cwd <path>                       # Working directory

# Examples:
krometrail launch "python app.py" --break order.py:147
krometrail launch "python -m pytest tests/ -x" --break "order.py:147 when discount < 0"
krometrail launch "node index.js" --break src/api.js:30 --language javascript
krometrail launch "go test ./..." --break service/order.go:147

# Attach to a running process
krometrail attach --port 5678 --language python
krometrail attach --pid 12345 --language go

# Check session status
krometrail status

# Stop the active (or specified) session
krometrail stop
krometrail stop --session abc123
```

---

## Execution Control

```bash
# Continue to next breakpoint
krometrail continue
krometrail continue --timeout 10000

# Step
krometrail step over
krometrail step into
krometrail step out
krometrail step over --count 5

# Run to a specific line (temporary breakpoint)
krometrail run-to order.py:155
```

---

## Breakpoints

```bash
# Set breakpoints (replaces existing in that file)
krometrail break order.py:147
krometrail break order.py:147,150,155

# Conditional
krometrail break "order.py:147 when discount < 0"

# Hit count
krometrail break "order.py:147 hit >=100"

# Logpoint
krometrail break "order.py:147 log 'discount={discount}'"

# Exception breakpoints
krometrail break --exceptions uncaught
krometrail break --exceptions raised
krometrail break --exceptions all

# List all active breakpoints
krometrail breakpoints

# Clear all breakpoints in a file
krometrail break --clear order.py
```

---

## State Inspection

```bash
# Evaluate an expression
krometrail eval "<expression>"
krometrail eval "cart.items[0].__dict__" --depth 3
krometrail eval "request.headers" --frame 2

# Show variables
krometrail vars
krometrail vars --scope global
krometrail vars --scope closure
krometrail vars --scope all
krometrail vars --filter "^user"
krometrail vars --frame 2

# Full stack trace
krometrail stack
krometrail stack --frames 20
krometrail stack --source

# View source
krometrail source order.py
krometrail source order.py:140-160
```

---

## Session Intelligence

```bash
# Watch expressions (auto-evaluated on every stop)
krometrail watch "len(cart.items)" "user.tier" "total > 0"

# Remove watch expressions
krometrail unwatch "user.tier"

# Session investigation log
krometrail log
krometrail log --detailed

# Program output (stdout/stderr)
krometrail output
krometrail output --stderr
krometrail output --since-action 5

# Thread listing
krometrail threads
```

---

## Browser Tools

```bash
# Start recording
krometrail browser start http://localhost:3000
krometrail browser start http://localhost:3000 --framework-state
krometrail browser start http://localhost:3000 --framework-state react

# Recording status
krometrail browser status

# Place a marker
krometrail browser mark "user submitted form"

# Stop recording
krometrail browser stop

# Export session data
krometrail browser export <session-id> --format har --output file.har
```

---

## Session Investigation

```bash
# List sessions
krometrail session list
krometrail session list --has-errors
krometrail session list --url "localhost:3000"

# Session overview
krometrail session overview <session-id>

# Search events
krometrail session search <session-id> "payment failed"
krometrail session search <session-id> --event-types network_response --status-codes 500
krometrail session search <session-id> --framework react --pattern stale_closure

# Deep-dive into an event
krometrail session inspect <session-id> --event-id <event-id>

# Diff two moments
krometrail session diff <session-id> --from 5000 --to 15000
krometrail session diff <session-id> --from-marker "loaded" --to-marker "error"

# Generate reproduction steps
krometrail session replay-context <session-id>
krometrail session replay-context <session-id> --format playwright
krometrail session replay-context <session-id> --format cypress
```

---

## Utility

```bash
# Check installed adapters/debuggers
krometrail doctor

# Print the agent skill file (for Codex system prompts)
krometrail skill

# Version
krometrail --version
```

---

## JSON Output

Every command supports `--json` for structured output:

```bash
krometrail continue --json
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
