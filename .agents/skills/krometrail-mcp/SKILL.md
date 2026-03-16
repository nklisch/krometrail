---
name: krometrail-mcp
description: >
  Krometrail MCP tool reference. LOAD THIS SKILL before invoking any mcp__krometrail__* tool.
  Covers the three tool namespaces (debug_*, chrome_*, session_*), when to use each,
  common workflows, and pitfalls that cause wasted sessions or confusing errors.
---

# Krometrail MCP Reference

**Load this skill before invoking any `mcp__krometrail__*` tool.**

Krometrail provides runtime debugging and browser recording via MCP. There are three distinct tool namespaces — picking the wrong one wastes time.

---

## Tool Namespaces

### `debug_*` — Code debugger (DAP)
Steps through source code, inspects variables, evaluates expressions.

**Use when:** Debugging Python, Node.js, Go, TypeScript, Ruby, Java, Rust, C#, C++, Swift.

Tools: `debug_launch`, `debug_attach`, `debug_stop`, `debug_status`, `debug_continue`, `debug_step`, `debug_run_to`, `debug_set_breakpoints`, `debug_set_exception_breakpoints`, `debug_list_breakpoints`, `debug_evaluate`, `debug_variables`, `debug_stack_trace`, `debug_source`, `debug_watch`, `debug_action_log`, `debug_output`, `debug_threads`

**Do NOT use for:** Opening URLs in a browser. `debug_launch` runs shell commands — passing a URL will fail.

> **Language-specific setup:** Read the reference for your target language before launching.
> - Python → `references/python.md`
> - Node.js / TypeScript → `references/node.md`
> - Go → `references/go.md`
> - Chrome / browser JS → `references/chrome.md`
> - Other languages (Ruby, Java, Rust, C#, C++, Swift) → check `debug_status` for available adapters and prerequisites

---

### `chrome_*` — Browser recording & control (CDP)
Launches Chrome, records browser events, and optionally drives the browser with batch actions.

**Use when:** Observing a web app — reproducing a bug, recording a user flow, capturing network traffic. Also when you need to drive the browser (navigate, click, fill forms) as part of a debugging workflow.

Tools: `chrome_start`, `chrome_status`, `chrome_mark`, `chrome_run_steps`, `chrome_stop`

> **Chrome setup:** See `references/chrome.md` for how to handle existing Chrome instances, CDP errors, and headless environments.

> **Annotations:** When recording, application code can call `window.__krometrail?.mark('label')` to place lightweight annotations in the timeline without triggering screenshots. See `references/chrome.md` — "Annotations" section for details and when to use annotations vs markers.

**Do NOT use for:** Stepping through JavaScript source. For JS debugging, use `debug_attach` (see `references/node.md`).

---

### `session_*` — Browser session investigation (read-only)
Queries recorded browser sessions from the local database.

**Use when:** Investigating what happened in a recorded browser session.

Tools: `session_list`, `session_overview`, `session_search`, `session_inspect`, `session_diff`, `session_replay_context`

---

## Workflows

### Debug a program
1. Read the language reference (see branching above)
2. `debug_launch(command: '...', breakpoints: [...])`
3. `debug_variables(session_id: '...')` / `debug_evaluate(...)` / `debug_step(...)`
4. `debug_stop(session_id: '...')`

### Record a browser session and investigate it
1. See `references/chrome.md` for setup
2. `chrome_start(url: '...', profile: 'krometrail')`
3. `chrome_mark(label: '...')` at key moments
4. `chrome_stop()`
5. `session_list()` → `session_overview(session_id: 'latest')` → `session_search(...)` → `session_inspect(...)`
   - All `session_*` tools accept `session_id: "latest"` to target the most recent session

### Drive the browser with batch actions, then investigate
1. `chrome_start(url: '...', profile: 'krometrail')`
2. `chrome_run_steps({ steps: [{ action: "navigate", url: "/login" }, { action: "fill", selector: "#email", value: "test@example.com" }, ...] })`
   - Each step is auto-marked and auto-screenshotted — the recording captures everything
   - Save with `name` + `save: true`, replay later with just `name`
3. `chrome_stop()`
4. Investigate with `session_overview`, `session_search`, etc.

---

## Key Parameters

### `debug_launch`
| Param | Default | Notes |
|-------|---------|-------|
| `command` | required | Shell command — NOT a URL |
| `language` | auto-detected | `python`, `node`, `go`, `typescript`, `ruby`, `java`, `rust`, `csharp`, `cpp`, `swift` |
| `breakpoints` | — | `[{file: 'app.py', breakpoints: [{line: 42}]}]` |
| `stop_on_entry` | `false` | Pause on first line |
| `launch_config` | — | Use `.vscode/launch.json` instead of a command |

### `chrome_start`
| Param | Default | Notes |
|-------|---------|-------|
| `url` | — | Open this URL when launching |
| `profile` | — | Isolated Chrome profile (recommended — avoids conflicts) |
| `attach` | `false` | Attach to Chrome already running with `--remote-debugging-port` |
| `port` | `9222` | CDP port |
| `all_tabs` | `false` | Record all tabs (default: active tab only) |

### `chrome_run_steps`
| Param | Default | Notes |
|-------|---------|-------|
| `steps` | required* | Array of step objects. Each has `action` + action-specific params |
| `name` | — | Scenario name. With `save: true` stores it; alone replays it |
| `save` | `false` | Save steps under `name` for later replay |
| `capture` | `{ screenshot: "all", markers: true }` | Auto-capture config per step |

*Required unless replaying a named scenario (pass `name` only).

**Actions:** `navigate`, `reload`, `click`, `fill`, `select`, `submit`, `type`, `hover`, `scroll_to`, `scroll_by`, `wait`, `wait_for`, `wait_for_navigation`, `wait_for_network_idle`, `screenshot`, `mark`, `evaluate`
