# Krometrail

**Browser observation and runtime debugging for AI coding agents.**

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

Krometrail is an MCP server and CLI that gives AI coding agents eyes into running applications. It records browser activity — network requests, console output, DOM mutations, framework state, storage changes, and screenshots — then lets agents search, inspect, and diff that recorded session to diagnose bugs. It also bridges the [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) (DAP) for breakpoint-level debugging across 6 languages.

## Browser Observation

Record everything happening in a browser session and investigate it later — no code changes required.

### What Gets Captured

- **Network** — every request/response with headers, bodies, status codes, timing, and WebSocket frames
- **Console** — all console output with levels, args, and stack traces
- **DOM mutations** — meaningful structural changes (forms, dialogs, sections)
- **User input** — clicks, form submissions, field changes
- **Screenshots** — periodic and navigation-triggered snapshots
- **Browser storage** — localStorage/sessionStorage mutations and cross-tab events
- **Framework state** — React and Vue component lifecycles, state/prop diffs, store mutations
- **Framework errors** — auto-detected anti-patterns (stale closures, infinite re-renders, missing cleanup)

### Quick Start

Add to your agent's MCP config (`.mcp.json` in your project root):

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

Start recording a browser session:

```bash
# MCP: chrome_start({ url: "http://localhost:3000", framework_state: true })
# CLI:
krometrail browser start http://localhost:3000 --framework-state

# Place markers at significant moments
krometrail browser mark "submitted form"

# Stop recording
krometrail browser stop
```

Investigate what happened:

```bash
# List recorded sessions
krometrail session list --has-errors

# Get a structured overview
krometrail session overview <session-id>

# Search for specific events
krometrail session search <session-id> --event-types network_response --status-codes 500
krometrail session search <session-id> --framework react --pattern stale_closure

# Deep-dive into a specific event
krometrail session inspect <session-id> --event-id <id>

# Compare two moments (what changed between page load and error?)
krometrail session diff <session-id> --from <timestamp> --to <timestamp>

# Generate reproduction steps or test scaffolds
krometrail session replay-context <session-id> --format playwright
```

### Browser MCP Tools

| Tool | Description |
|------|-------------|
| `chrome_start` | Launch Chrome and start recording (URL, framework observation, tab filtering) |
| `chrome_status` | Current recording status, event counts, active tabs |
| `chrome_mark` | Place a named marker in the recording timeline |
| `chrome_stop` | Stop recording and persist events to database |
| `session_list` | List recorded sessions with filters (time, URL, errors, markers) |
| `session_overview` | Structured overview: navigation, markers, errors, network, framework summary |
| `session_search` | Full-text and structured search across recorded events |
| `session_inspect` | Deep-dive into a specific event with full context and nearest screenshot |
| `session_diff` | Compare two moments: URL, storage, console, network, framework state changes |
| `session_replay_context` | Generate reproduction steps or Playwright/Cypress test scaffolds |

### Framework State Observation

When enabled, Krometrail hooks into React DevTools and Vue Devtools to track:

- Component mount/update/unmount lifecycles
- State and prop changes with before/after diffs
- Render counts and trigger source identification
- Pinia/Vuex store mutations (Vue)
- Auto-detected bug patterns: stale closures, infinite re-renders, missing effect cleanup, excessive context re-renders

```json
{ "url": "http://localhost:3000", "framework_state": true }
{ "url": "http://localhost:3000", "framework_state": ["react"] }
{ "url": "http://localhost:3000", "framework_state": ["react", "vue"] }
```

## Runtime Debugging

Set breakpoints, step through code, and inspect variables across 6 languages via DAP.

### Supported Languages

| Language | Debugger | Adapter | Status |
|----------|----------|---------|--------|
| Python | debugpy | TCP | Stable |
| Node.js | js-debug | TCP | Stable |
| Go | Delve | TCP | Stable |
| Rust | CodeLLDB | TCP | Stable |
| Java | java-debug-adapter | TCP | Stable |
| C/C++ | GDB 14+ / lldb-dap | stdio | Stable |

### Debug CLI

```bash
krometrail launch "python app.py" --break order.py:147
krometrail step over
krometrail eval "discount"
krometrail vars --scope local
krometrail continue
krometrail stop
```

### Debug MCP Tools

| Tool | Description |
|------|-------------|
| `debug_launch` | Launch a program with initial breakpoints |
| `debug_attach` | Attach to a running process |
| `debug_stop` | Terminate the debug session |
| `debug_status` | Query session state and capabilities |
| `debug_continue` | Resume execution until next breakpoint |
| `debug_step` | Step over, into, or out |
| `debug_run_to` | Run to a specific line |
| `debug_set_breakpoints` | Set breakpoints with conditions, hit counts, logpoints |
| `debug_set_exception_breakpoints` | Filter by exception type |
| `debug_list_breakpoints` | List all active breakpoints |
| `debug_evaluate` | Evaluate an expression in the current frame |
| `debug_variables` | Inspect variables by scope with regex filtering |
| `debug_stack_trace` | Get the full call stack |
| `debug_source` | Read source code around a location |
| `debug_watch` | Add/remove persistent watch expressions |
| `debug_action_log` | Review the investigation log |
| `debug_output` | Capture stdout/stderr |
| `debug_threads` | List threads, goroutines, etc. |

## Features

- **Passive browser recording** — capture network, console, DOM, storage, and screenshots without code changes
- **Framework-aware** — React and Vue state tracking with bug pattern detection
- **Session investigation** — search, inspect, diff, and replay recorded browser sessions
- **Compact viewport** — debugger state rendered in ~400 tokens per stop, optimized for LLM context windows
- **Conditional breakpoints** — `order.py:147 when discount < 0`, hit counts, logpoints
- **Watch expressions** — persistent expressions auto-evaluated on every stop
- **Framework detection** — auto-detects pytest, Django, Flask, jest, mocha, go test
- **Multi-threaded** — thread/goroutine listing and selection

### Skill File

Install the agent skill for CLI-based workflows. Install via [skilltap](https://skilltap.dev) or print to stdout:

```bash
skilltap install ./skill  # Install via skilltap
krometrail skill            # Or print skill to stdout
```

## Development

```bash
bun install              # Install dependencies
bun run dev              # Run CLI in dev mode
bun run mcp              # Run MCP server
bun run build            # Compile single binary
bun run build:all        # Build for all platforms (Linux, macOS, Windows)
```

### Testing

```bash
bun run test             # All tests
bun run test:unit        # Unit tests (no external deps)
bun run test:integration # Integration tests (needs debuggers)
bun run test:e2e         # E2E tests (full MCP path)
bun run test:agent       # Agent harness scenarios
```

Integration and E2E tests require debuggers to be installed. Run `krometrail doctor` to check availability. Tests skip cleanly per-adapter when a debugger is not found.

### Agent Harness

The agent harness (`tests/agent-harness/`) is a scenario-based test suite for evaluating how well agents debug with Krometrail. It contains 35 scenarios across 3 languages at 5 difficulty levels:

- **Python** — 12 scenarios (closure bugs, mutation errors, float accumulation, deep pipelines)
- **Node.js** — 11 scenarios (async races, event loop ordering, regex state, `this` binding)
- **TypeScript** — 12 scenarios (type assertion escapes, generic constraints, runtime registries)

```bash
bun run test:agent          # Run scenarios
bun run test:agent:report   # Generate report with token/cost metrics
```

### Linting

```bash
bun run lint             # Check with Biome
bun run lint:fix         # Auto-fix
```

## Architecture

```
src/
  mcp/          MCP server + tool handlers
  cli/          CLI entry point + commands (citty)
  core/         Session manager, viewport renderer, DAP client, compression
  adapters/     Language-specific debugger adapters (6 languages)
  browser/      Chrome CDP recording, investigation engine, framework observers
  daemon/       Session persistence over Unix socket
  frameworks/   Auto-detection for test/web frameworks
```

The MCP server and CLI share the same core. Browser tools use CDP to record events into a SQLite-backed session store with JSONL event storage. Debug tools use the session manager to orchestrate DAP communication. The viewport renderer formats state for agents, and adapters handle language-specific debugger setup.

## Documentation

| Document | Contents |
|----------|----------|
| [VISION.md](docs/VISION.md) | Problem statement, prior art, roadmap |
| [ARCH.md](docs/ARCH.md) | System layers, data flow, viewport rendering |
| [UX.md](docs/UX.md) | Viewport abstraction, agent interaction patterns |
| [SPEC.md](docs/SPEC.md) | Adapter contract, type definitions |
| [INTERFACE.md](docs/INTERFACE.md) | MCP tool + CLI command reference |
| [TESTING.md](docs/TESTING.md) | Testing philosophy and tiers |

## License

MIT
