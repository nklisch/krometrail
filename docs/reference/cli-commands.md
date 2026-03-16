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
krometrail completions <shell> # Generate shell completions (bash, zsh, fish)
```

## Global Flags

These flags are available on all debug and browser commands:

| Flag | Description |
|------|-------------|
| `--session <id>`, `-s` | Target a specific debug session (required when multiple sessions are active) |
| `--json` | Output structured JSON instead of plain text |
| `--quiet` | Suppress banners and hints; output viewport only |
| `--version` | Show version |

---

## Session Lifecycle

### `debug launch`

```bash
krometrail debug launch "python app.py" --break order.py:147
krometrail debug launch "python -m pytest tests/ -x" --break "order.py:147 when discount < 0"
krometrail debug launch "node index.js" --break src/api.js:30 --language javascript
krometrail debug launch "go test ./..." --break service/order.go:147
```

<!--@include: ../.generated/cli-debug-launch.md-->

### `debug attach`

```bash
krometrail debug attach --port 5678 --language python
krometrail debug attach --pid 12345 --language go
```

<!--@include: ../.generated/cli-debug-attach.md-->

### `debug status`

```bash
krometrail debug status
```

### `debug stop`

```bash
krometrail debug stop
krometrail debug stop --session abc123
```

---

## Execution Control

### `debug continue`

```bash
krometrail debug continue
krometrail debug continue --timeout 10000
```

<!--@include: ../.generated/cli-debug-continue.md-->

### `debug step`

```bash
krometrail debug step over
krometrail debug step into
krometrail debug step out
krometrail debug step over --count 5
```

<!--@include: ../.generated/cli-debug-step.md-->

### `debug run-to`

```bash
krometrail debug run-to order.py:155
```

<!--@include: ../.generated/cli-debug-run-to.md-->

---

## Breakpoints

### `debug break`

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

# Clear all breakpoints in a file
krometrail debug break --clear order.py
```

<!--@include: ../.generated/cli-debug-break.md-->

### `debug breakpoints`

```bash
krometrail debug breakpoints
```

---

## State Inspection

### `debug eval`

```bash
krometrail debug eval "<expression>"
krometrail debug eval "cart.items[0].__dict__" --depth 3
krometrail debug eval "request.headers" --frame 2
```

<!--@include: ../.generated/cli-debug-eval.md-->

### `debug vars`

```bash
krometrail debug vars
krometrail debug vars --scope global
krometrail debug vars --scope closure
krometrail debug vars --scope all
krometrail debug vars --filter "^user"
krometrail debug vars --frame 2
```

<!--@include: ../.generated/cli-debug-vars.md-->

### `debug stack`

```bash
krometrail debug stack
krometrail debug stack --frames 20
krometrail debug stack --source
```

<!--@include: ../.generated/cli-debug-stack.md-->

### `debug source`

```bash
krometrail debug source order.py
krometrail debug source order.py:140-160
```

<!--@include: ../.generated/cli-debug-source.md-->

---

## Session Intelligence

### `debug watch`

```bash
krometrail debug watch "len(cart.items)" "user.tier" "total > 0"
```

<!--@include: ../.generated/cli-debug-watch.md-->

### `debug unwatch`

```bash
krometrail debug unwatch "user.tier"
```

<!--@include: ../.generated/cli-debug-unwatch.md-->

### `debug log`

```bash
krometrail debug log
krometrail debug log --detailed
```

<!--@include: ../.generated/cli-debug-log.md-->

### `debug output`

```bash
krometrail debug output
krometrail debug output --stderr
krometrail debug output --since-action 5
```

<!--@include: ../.generated/cli-debug-output.md-->

### `debug threads`

```bash
krometrail debug threads
```

---

## Browser Recording

### `browser start`

```bash
krometrail browser start --url http://localhost:3000
krometrail browser start --url http://localhost:3000 --framework-state auto
krometrail browser start --url http://localhost:3000 --framework-state react
krometrail browser start --attach                    # Attach to already-running Chrome
krometrail browser start --profile myproject          # Use isolated Chrome profile
krometrail browser start --all-tabs                   # Record all tabs
```

<!--@include: ../.generated/cli-browser-start.md-->

### `browser status`

```bash
krometrail browser status
```

### `browser mark`

```bash
krometrail browser mark "user submitted form"
```

<!--@include: ../.generated/cli-browser-mark.md-->

### `browser stop`

```bash
krometrail browser stop
krometrail browser stop --close-browser
```

<!--@include: ../.generated/cli-browser-stop.md-->

### `browser export`

```bash
krometrail browser export <session-id> --format har --output file.har
```

<!--@include: ../.generated/cli-browser-export.md-->

---

## Browser Session Investigation

### `browser sessions`

```bash
krometrail browser sessions
krometrail browser sessions --has-errors
krometrail browser sessions --url-contains "localhost:3000"
```

<!--@include: ../.generated/cli-browser-sessions.md-->

### `browser overview`

```bash
krometrail browser overview <session-id>
krometrail browser overview <session-id> --around-marker <marker-id>
krometrail browser overview <session-id> --include timeline,markers,errors
```

<!--@include: ../.generated/cli-browser-overview.md-->

### `browser search`

```bash
krometrail browser search <session-id> --query "payment failed"
krometrail browser search <session-id> --event-types network_response --status-codes 500
krometrail browser search <session-id> --framework react --pattern stale_closure
```

<!--@include: ../.generated/cli-browser-search.md-->

### `browser inspect`

```bash
krometrail browser inspect <session-id> --event <event-id>
krometrail browser inspect <session-id> --marker <marker-id>
krometrail browser inspect <session-id> --timestamp "2025-01-15T10:30:00Z"
```

<!--@include: ../.generated/cli-browser-inspect.md-->

### `browser diff`

```bash
krometrail browser diff <session-id> --from <timestamp-or-event-id> --to <timestamp-or-event-id>
```

<!--@include: ../.generated/cli-browser-diff.md-->

### `browser replay-context`

```bash
krometrail browser replay-context <session-id>
krometrail browser replay-context <session-id> --format test_scaffold --framework playwright
krometrail browser replay-context <session-id> --format test_scaffold --framework cypress
```

<!--@include: ../.generated/cli-browser-replay-context.md-->

---

## Utility

### `doctor`

```bash
krometrail doctor
krometrail doctor --json
```

### `commands`

```bash
krometrail commands
krometrail commands --group debug
krometrail commands --group browser
```

<!--@include: ../.generated/cli-commands.md-->

### `completions`

```bash
krometrail completions bash
krometrail completions zsh
krometrail completions fish
```

See [Shell Completions](#shell-completions) below for installation instructions.

---

## Shell Completions

Generate shell completion scripts for tab-completion of commands, subcommands, and flags:

```bash
# Bash
krometrail completions bash > ~/.local/share/bash-completion/completions/krometrail
# Or add to ~/.bashrc:
source <(krometrail completions bash)

# Zsh
krometrail completions zsh > "${fpath[1]}/_krometrail"
# Or add to ~/.zshrc:
source <(krometrail completions zsh)

# Fish
krometrail completions fish > ~/.config/fish/completions/krometrail.fish
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
