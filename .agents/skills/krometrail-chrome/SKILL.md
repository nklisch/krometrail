---
name: krometrail-chrome
description: Browser recording and control via the krometrail CLI. Use when recording a browser session, driving the browser with batch actions, or investigating recorded sessions. Captures network, console, DOM, framework state, screenshots.
license: MIT
compatibility: Requires Chrome/Chromium.
metadata:
  author: krometrail
  version: "0.2"
allowed-tools: Bash(krometrail:*)
---

# Krometrail — Browser Recording & Control (CLI)

Use the `krometrail browser` commands to record browser sessions, drive Chrome with batch actions, and investigate what happened.

## When to use

- The user reproduced a bug in the browser and dropped markers — investigate the session
- A web app has network errors, console errors, or unexpected behavior — search and inspect the recording
- You need to drive the browser (navigate, click, fill forms, wait) as part of a reproduction
- You need to compare browser state before and after an action — diff between markers

## Recording

```bash
# Launch Chrome and record at a URL (isolated profile avoids conflicts)
krometrail browser start --url http://localhost:3000 --profile krometrail

# With framework state capture (React, Vue)
krometrail browser start --url http://localhost:3000 --framework-state auto

# Attach to an already-running Chrome (must have --remote-debugging-port=9222)
krometrail browser start --attach

# Place markers at key moments
krometrail browser mark "submitted form"
krometrail browser mark "error appeared"

# Check status
krometrail browser status

# Stop recording
krometrail browser stop
krometrail browser stop --close-browser
```

## Batch browser actions

Drive the browser with a sequence of steps in one call. Requires an active recording (`start` first).

```bash
# From a JSON file
krometrail browser run-steps --file steps.json

# Inline JSON
krometrail browser run-steps --steps '[{"action":"navigate","url":"/login"},{"action":"fill","selector":"#email","value":"test@example.com"},{"action":"click","selector":"#submit"}]'

# Save a named scenario for later replay
krometrail browser run-steps --file steps.json --name login-flow --save

# Replay a saved scenario (no steps needed)
krometrail browser run-steps --name login-flow

# Screenshot only on errors
krometrail browser run-steps --file steps.json --screenshot on_error

# Disable auto-markers
krometrail browser run-steps --file steps.json --no-markers
```

Each step is auto-marked (`step:1:navigate:/login`, `step:2:fill:#email`, etc.) and auto-screenshotted for investigation.

### Steps JSON format

```json
[
  { "action": "navigate", "url": "/login" },
  { "action": "fill", "selector": "#email", "value": "test@example.com" },
  { "action": "fill", "selector": "#password", "value": "secret" },
  { "action": "submit", "selector": "#login-form" },
  { "action": "wait_for", "selector": ".dashboard", "timeout": 5000 },
  { "action": "screenshot", "label": "after-login" }
]
```

**Available actions:** `navigate`, `reload`, `click`, `fill`, `select`, `submit`, `type`, `hover`, `scroll_to`, `scroll_by`, `wait`, `wait_for`, `wait_for_navigation`, `wait_for_network_idle`, `screenshot`, `mark`, `evaluate`

## Investigating sessions

```bash
# List recorded sessions
krometrail browser sessions
krometrail browser sessions --has-errors

# Overview of a session
krometrail browser overview <session-id>
krometrail browser overview <session-id> --around-marker <marker-id>

# Search events
krometrail browser search <session-id> --query "payment failed"
krometrail browser search <session-id> --status-codes 500
krometrail browser search <session-id> --framework react --pattern stale_closure

# Inspect a specific event
krometrail browser inspect <session-id> --event <event-id>
krometrail browser inspect <session-id> --marker <marker-id>

# Compare two moments
krometrail browser diff <session-id> --from <moment> --to <moment>

# Generate reproduction steps or test scaffolds
krometrail browser replay-context <session-id> --format reproduction_steps
krometrail browser replay-context <session-id> --format test_scaffold --framework playwright

# Export as HAR
krometrail browser export <session-id> --format har --output session.har
```

See [references/chrome.md](references/chrome.md) for the full reference.

## What gets captured

- **Network**: All XHR/fetch requests with headers, bodies, status codes, timing, WebSocket frames
- **Console**: All console output with levels, arguments, and stack traces
- **DOM mutations**: Structural changes — forms, dialogs, sections
- **User input**: Clicks, form submissions, field changes
- **Screenshots**: At markers, on errors, and during step execution
- **Storage**: localStorage/sessionStorage mutations
- **Framework state** (if enabled): React/Vue component lifecycles, state diffs, bug patterns

## Investigation strategy

1. **Start with the overview.** Shows event counts, markers, and errors at a glance.
2. **Narrow with search.** Find specific errors, status codes, or framework patterns.
3. **Deep-dive with inspect.** Full event detail — request/response bodies, surrounding events, screenshots.
4. **Compare with diff.** What changed between two moments — new errors, state mutations, network failures.
5. **Generate reproduction.** Test scaffolds or step-by-step reproduction instructions.

### Tips

- Markers are your anchors — search and diff around them
- Step markers (`step:N:action:detail`) appear automatically during `run-steps` execution
- Use `--around-marker` in overview to focus on a specific moment
- Framework patterns (stale closures, infinite re-renders, missing cleanup) are auto-detected when `--framework-state` is enabled
- Screenshots are captured automatically at markers, on errors, and during step execution
