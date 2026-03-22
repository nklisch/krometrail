---
name: krometrail-mcp
description: >
  Krometrail MCP navigation guide. Load this skill first to understand which tool namespace to use.
  Covers the three tool namespaces (debug_*, chrome_*, session_*), when to use each,
  common pitfalls, and pointers to the detailed skill for each namespace.
---

# Krometrail MCP — Navigation Guide

**Load this skill first.** Then load the detailed skill for what you're doing.

Krometrail exposes three MCP tool namespaces. Picking the wrong one wastes sessions and produces confusing errors.

---

## Tool Namespaces

### `debug_*` — Code debugger (DAP)

Steps through source code, inspects variables, evaluates expressions.

**Use when:** Debugging Python, Node.js, Go, TypeScript, Ruby, Java, Rust, C#, C++, Swift.

**Load the skill:** Load `krometrail-debug` for the full tool reference, language setup, breakpoint syntax, and debugging strategies.

**Pitfall:** `debug_launch` runs a shell command — do NOT pass a URL. It will fail.

---

### `chrome_*` — Browser recording & control (CDP)

Launches Chrome, records browser events, and drives the browser with batch actions.

**Use when:** Observing a web app — reproducing a bug, recording a user flow, capturing network traffic, driving the browser (navigate, click, fill forms), or refreshing the page for a clean slate (`chrome_refresh`).

**Load the skill:** Load `krometrail-chrome` for the full tool reference, Chrome setup, step actions, and investigation strategies.

**Pitfall:** Always pass `profile: 'krometrail'` to `chrome_start` to avoid conflicts with any existing Chrome instance.

---

### `session_*` — Browser session investigation (read-only)

Queries recorded browser sessions from the local database.

**Use when:** Investigating what happened in a browser session recorded with `chrome_*` tools.

**Load the skill:** Covered in `krometrail-chrome` — session investigation tools are part of that skill.

---

## Which skill to load

| Goal | Load skill |
|------|-----------|
| Debug Python, JS, Go, Rust, Java, C++, C# | `krometrail-debug` |
| Record or observe a web app in Chrome | `krometrail-chrome` |
| Investigate a recorded browser session | `krometrail-chrome` |
| Drive the browser with clicks/navigation | `krometrail-chrome` |

---

## Common pitfalls

- **Don't pass URLs to `debug_launch`.** It runs shell commands. Use `chrome_start` for browser tasks.
- **Chrome conflicts.** If Chrome is already running, `chrome_start` without `profile` may fail with a CDP error. Always use `profile: 'krometrail'` for an isolated instance.
- **Chrome launch absorbed.** On Linux, Chrome wrapper scripts (e.g., `/usr/bin/google-chrome`) may delegate to an existing Chrome and exit immediately, causing `chrome_start` to fail. If this happens, ask the user to close their Chrome browser and retry. Do NOT suggest `pkill` — it can kill Electron apps (Discord, VS Code, etc.) that have `chrome` in their process names.
- **Session IDs.** All `debug_*` tools require a `session_id` returned by `debug_launch`. All `session_*` tools accept `session_id: "latest"` for convenience.
- **Always call `debug_stop`.** Debug sessions don't self-terminate. Call `debug_stop(session_id: '...')` when done or the debugger process will linger.
- **Language-specific prerequisites.** Each language needs its debugger binary installed (debugpy for Python, dlv for Go, etc.). See the `krometrail-debug` skill for per-language setup.

---

## Language references (for `debug_*`)

When using debug tools, read the reference for your target language:

- Python → [`references/python.md`](references/python.md)
- Node.js / TypeScript → [`references/node.md`](references/node.md)
- Go → [`references/go.md`](references/go.md)
- Other languages (Ruby, Java, Rust, C#, C++, Swift) → covered in the `krometrail-debug` skill
