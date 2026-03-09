---
name: agent-lens-cli
description: >
  Agent Lens CLI reference. Load this skill when invoking the agent-lens CLI (agent-lens <command>).
  Covers all debug and browser subcommands, flags, breakpoint syntax, output modes, and workflows.
---

# Agent Lens CLI Reference

The `agent-lens` binary communicates with a background daemon over a Unix socket. Sessions persist between commands.

> **Language-specific setup:** Before launching a debug session, read the reference for your target language.
> - Python → `references/python.md`
> - Node.js / TypeScript → `references/node.md`
> - Go → `references/go.md`
> - Chrome / browser recording → `references/chrome.md`

---

## Global Flags

All session commands accept these:

| Flag | Short | Description |
|------|-------|-------------|
| `--session <id>` | `-s` | Target a specific session (required when multiple are active) |
| `--json` | | Raw JSON output |
| `--quiet` | | Viewport only — no banners or hints |

Auto-resolves session if exactly one is active. Errors if zero or multiple and `--session` is omitted.

---

## Debug Commands

### `agent-lens launch "<command>"`
```sh
agent-lens launch "python app.py"
agent-lens launch "pytest tests/test_order.py -s" --break order.py:147
agent-lens launch "node server.js" --stop-on-entry
agent-lens launch "go run ./cmd/server"
agent-lens launch --config-name "My Config"          # from .vscode/launch.json
```

Flags:
- `-b / --break <spec>` — initial breakpoint (see Breakpoint Syntax below)
- `--language <lang>` — override auto-detection
- `--framework <id>` — override framework detection, or `none`
- `--stop-on-entry` — pause on first executable line
- `--config <path>` — path to launch.json
- `-C / --config-name <name>` — configuration name from launch.json

### `agent-lens attach --language <lang>`
```sh
agent-lens attach --language python --port 5678
agent-lens attach --language node --port 9229
agent-lens attach --language go --pid 12345
```

### `agent-lens stop`
Terminate the session and kill the process.

### `agent-lens status`
Show current session state and viewport (source + locals + call stack).

---

## Execution Control

```sh
agent-lens continue [--timeout <ms>]
agent-lens step over
agent-lens step into
agent-lens step out
agent-lens step over --count 5
agent-lens run-to order.py:150
```

---

## Breakpoints

```sh
agent-lens break order.py:147
agent-lens break order.py:147,152,160
agent-lens break "order.py:147 when discount < 0"
agent-lens break "order.py:147 hit >=5"
agent-lens break "order.py:147 log processed {count} items"
agent-lens break --exceptions uncaught
agent-lens break --clear order.py
agent-lens breakpoints                    # list all
```

**Breakpoint spec:** `file:line[,line,...] [when <expr>] [hit <n>] [log <msg>]`

Exception filters: Python: `raised`, `uncaught`, `userUnhandled` · Node.js: `all`, `uncaught` · Go: `panic`

---

## State Inspection

```sh
agent-lens vars                           # local scope
agent-lens vars --scope global
agent-lens vars --filter "^user" --frame 2
agent-lens eval "cart.total"
agent-lens eval "order.total" --frame 1 --depth 3
agent-lens stack
agent-lens stack --frames 5 --source
agent-lens source order.py
agent-lens source order.py:140-160
```

---

## Session Intelligence

```sh
agent-lens watch "order.total" "cart.item_count"
agent-lens unwatch "cart.item_count"
agent-lens log
agent-lens log --detailed
agent-lens output
agent-lens output --stderr
agent-lens output --since-action 3
agent-lens threads
```

---

## Browser Commands (`agent-lens browser <subcommand>`)

> See `references/chrome.md` for setup, CDP errors, and investigation patterns.

```sh
# Recording control
agent-lens browser start --url http://localhost:3000 --profile agent-lens
agent-lens browser start --attach
agent-lens browser status
agent-lens browser mark "submitted form"
agent-lens browser stop
agent-lens browser stop --close-browser

# Session investigation
agent-lens browser sessions
agent-lens browser sessions --has-errors --limit 5
agent-lens browser overview <id>
agent-lens browser overview <id> --around-marker <marker-id>
agent-lens browser search <id> --query "validation error"
agent-lens browser search <id> --status-codes 422,500
agent-lens browser inspect <id> --event <event-id>
agent-lens browser inspect <id> --marker <marker-id>
agent-lens browser diff <id> --before <ts> --after <ts>
agent-lens browser replay-context <id> --format reproduction_steps
agent-lens browser replay-context <id> --format test_scaffold --framework playwright
agent-lens browser export <id> --format har --output session.har
```

---

## Diagnostics

```sh
agent-lens doctor   # check prerequisites and adapter health
agent-lens skill    # print the skill file
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
agent-lens launch "python order.py" --break order.py:147
agent-lens vars
agent-lens eval "order.discount_pct"
agent-lens step over
agent-lens continue
agent-lens stop
```

### Debug a test
```sh
agent-lens launch "pytest tests/test_order.py::test_discount -s"
agent-lens continue
agent-lens vars --scope local
agent-lens step into
```

### Record a browser flow
```sh
agent-lens browser start --url http://localhost:3000 --profile agent-lens
# interact in browser
agent-lens browser mark "submitted form"
agent-lens browser stop
agent-lens browser sessions
agent-lens browser overview <id>
agent-lens browser search <id> --status-codes 422,500
```
