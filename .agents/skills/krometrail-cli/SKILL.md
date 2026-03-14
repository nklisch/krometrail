---
name: krometrail-cli
description: >
  Krometrail CLI reference. Load this skill when invoking the krometrail CLI (krometrail <command>).
  Covers all debug and browser subcommands, flags, breakpoint syntax, output modes, and workflows.
---

# Krometrail CLI Reference

The `krometrail` binary communicates with a background daemon over a Unix socket. Sessions persist between commands.

> **Language-specific setup:** Before launching a debug session, read the reference for your target language.
> - Python → `references/python.md`
> - Node.js / TypeScript → `references/node.md`
> - Go → `references/go.md`
> - Chrome / browser recording → `references/chrome.md`

---

## Command Structure

```
krometrail debug <command>     # Debug session commands
krometrail browser <command>   # Browser recording commands
krometrail doctor              # Check installed debuggers
krometrail commands            # List all commands (machine-readable)
```

## Global Flags

All debug session commands accept these:

| Flag | Short | Description |
|------|-------|-------------|
| `--session <id>` | `-s` | Target a specific session (required when multiple are active) |
| `--json` | | Raw JSON output |
| `--quiet` | | Viewport only — no banners or hints |

Auto-resolves session if exactly one is active. Errors if zero or multiple and `--session` is omitted.

---

## Debug Commands

### `krometrail debug launch "<command>"`
```sh
krometrail debug launch "python app.py"
krometrail debug launch "pytest tests/test_order.py -s" --break order.py:147
krometrail debug launch "node server.js" --stop-on-entry
krometrail debug launch "go run ./cmd/server"
krometrail debug launch --config-name "My Config"          # from .vscode/launch.json
```

Flags:
- `-b / --break <spec>` — initial breakpoint (see Breakpoint Syntax below)
- `--language <lang>` — override auto-detection
- `--framework <id>` — override framework detection, or `none`
- `--stop-on-entry` — pause on first executable line
- `--config <path>` — path to launch.json
- `-C / --config-name <name>` — configuration name from launch.json
- `--cwd <path>` — working directory for the debug target

### `krometrail debug attach --language <lang>`
```sh
krometrail debug attach --language python --port 5678
krometrail debug attach --language node --port 9229
krometrail debug attach --language go --pid 12345
```

### `krometrail debug stop`
Terminate the session and kill the process.

### `krometrail debug status`
Show current session state and viewport (source + locals + call stack).

---

## Execution Control

```sh
krometrail debug continue [--timeout <ms>]
krometrail debug step over
krometrail debug step into
krometrail debug step out
krometrail debug step over --count 5
krometrail debug run-to order.py:150
```

---

## Breakpoints

```sh
krometrail debug break order.py:147
krometrail debug break order.py:147,152,160
krometrail debug break "order.py:147 when discount < 0"
krometrail debug break "order.py:147 hit >=5"
krometrail debug break "order.py:147 log processed {count} items"
krometrail debug break --exceptions uncaught
krometrail debug break --clear order.py
krometrail debug breakpoints                    # list all
```

**Breakpoint spec:** `file:line[,line,...] [when <expr>] [hit <n>] [log <msg>]`

Exception filters: Python: `raised`, `uncaught`, `userUnhandled` · Node.js: `all`, `uncaught` · Go: `panic`

---

## State Inspection

```sh
krometrail debug vars                           # local scope
krometrail debug vars --scope global
krometrail debug vars --filter "^user" --frame 2
krometrail debug stack
krometrail debug stack --frames 5 --source
krometrail debug source order.py
krometrail debug source order.py:140-160
```

Evaluate an expression in the current frame:

```
krometrail debug eval "cart.total"
krometrail debug eval "order.total" --frame 1 --depth 3
```

---

## Session Intelligence

```sh
krometrail debug watch "order.total" "cart.item_count"
krometrail debug unwatch "cart.item_count"
krometrail debug log
krometrail debug log --detailed
krometrail debug output
krometrail debug output --stderr
krometrail debug output --since-action 3
krometrail debug threads
```

---

## Browser Commands (`krometrail browser <subcommand>`)

> See `references/chrome.md` for setup, CDP errors, and investigation patterns.

```sh
# Recording control
krometrail browser start --url http://localhost:3000 --profile krometrail
krometrail browser start --attach
krometrail browser start --framework-state auto
krometrail browser start --framework-state react
krometrail browser status
krometrail browser mark "submitted form"
krometrail browser stop
krometrail browser stop --close-browser

# Session investigation
krometrail browser sessions
krometrail browser sessions --has-errors --limit 5
krometrail browser overview <id>
krometrail browser overview <id> --around-marker <marker-id>
krometrail browser search <id> --query "validation error"
krometrail browser search <id> --status-codes 422,500
krometrail browser inspect <id> --event <event-id>
krometrail browser inspect <id> --marker <marker-id>
krometrail browser diff <id> --from <moment> --to <moment>
krometrail browser replay-context <id> --format reproduction_steps
krometrail browser replay-context <id> --format test_scaffold --framework playwright
krometrail browser export <id> --format har --output session.har
```

---

## Diagnostics

```sh
krometrail doctor   # check prerequisites and adapter health
```

---

## Output Modes

| Mode | Flag | Content |
|------|------|---------|
| Default | — | Formatted viewport: source + locals + call stack |
| JSON | `--json` | Raw JSON payload |
| Quiet | `--quiet` | Viewport text only, no banners |

---

## Common Workflows

### Debug with a breakpoint
```sh
krometrail debug launch "python order.py" --break order.py:147
krometrail debug vars
krometrail debug step over
krometrail debug continue
krometrail debug stop
```

### Debug a test
```sh
krometrail debug launch "pytest tests/test_order.py::test_discount -s"
krometrail debug continue
krometrail debug vars --scope local
krometrail debug step into
```

### Record a browser flow
```sh
krometrail browser start --url http://localhost:3000 --profile krometrail
# interact in browser
krometrail browser mark "submitted form"
krometrail browser stop
krometrail browser sessions
krometrail browser overview <id>
krometrail browser search <id> --status-codes 422,500
```
