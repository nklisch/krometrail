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

Use the `krometrail chrome` commands to record browser sessions, drive Chrome with batch actions, and investigate what happened.

## When to use

- The user reproduced a bug in the browser and dropped markers — investigate the session
- A web app has network errors, console errors, or unexpected behavior — search and inspect the recording
- You need to drive the browser (navigate, click, fill forms, wait) as part of a reproduction
- You need to compare browser state before and after an action — diff between markers

## Recording

```bash
# Launch Chrome and record at a URL (isolated profile avoids conflicts)
krometrail chrome start --url http://localhost:3000 --profile krometrail

# With framework state capture (React, Vue)
krometrail chrome start --url http://localhost:3000 --framework-state auto

# Attach to an already-running Chrome (must have --remote-debugging-port=9222)
krometrail chrome start --attach

# Place markers at key moments
krometrail chrome mark "submitted form"
krometrail chrome mark "error appeared"

# Check status
krometrail chrome status

# Reload page and clear buffer (clean slate without restarting)
krometrail chrome refresh

# Stop recording
krometrail chrome stop
krometrail chrome stop --close-browser
```

## Batch browser actions

Drive the browser with a sequence of steps in one call. Requires an active recording (`start` first).

```bash
# From a JSON file
krometrail chrome run-steps --file steps.json

# Inline JSON
krometrail chrome run-steps --steps '[{"action":"navigate","url":"/login"},{"action":"fill","selector":"#email","value":"test@example.com"},{"action":"click","selector":"#submit"}]'

# Save a named scenario for later replay
krometrail chrome run-steps --file steps.json --name login-flow --save

# Replay a saved scenario (no steps needed)
krometrail chrome run-steps --name login-flow

# Screenshot only on errors
krometrail chrome run-steps --file steps.json --screenshot on_error

# Disable auto-markers
krometrail chrome run-steps --file steps.json --no-markers
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

**Available actions:** `navigate`, `reload`, `click`, `fill`, `select`, `submit`, `type`, `press_key`, `hover`, `scroll_to`, `scroll_by`, `wait`, `wait_for`, `wait_for_navigation`, `wait_for_network_idle`, `screenshot`, `mark`, `evaluate`

**Key tips:**
- Use `reload` (not navigate to the same URL) for a full page refresh — navigate may hit SPA client-side routing cache
- Use `press_key` with `key: "Enter"` to submit forms that lack a `<button type="submit">` — this is common in chat UIs, search bars, and custom form components
- Start with a `screenshot` step before interacting to understand the current page state

## Investigating sessions

```bash
# List recorded sessions
krometrail chrome sessions
krometrail chrome sessions --has-errors

# Overview of a session
krometrail chrome overview <session-id>
krometrail chrome overview <session-id> --around-marker <marker-id>

# Search events
krometrail chrome search <session-id> --query "payment failed"
krometrail chrome search <session-id> --status-codes 500
krometrail chrome search <session-id> --framework react --pattern stale_closure

# Inspect a specific event
krometrail chrome inspect <session-id> --event <event-id>
krometrail chrome inspect <session-id> --marker <marker-id>

# Compare two moments
krometrail chrome diff <session-id> --from <moment> --to <moment>

# Generate reproduction steps or test scaffolds
krometrail chrome replay-context <session-id> --format reproduction_steps
krometrail chrome replay-context <session-id> --format test_scaffold --framework playwright

# Export as HAR
krometrail chrome export <session-id> --format har --output session.har
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

### Chrome launch failures

If `chrome_start` fails with "Chrome exited immediately":
1. Ask the user to close their Chrome browser, then retry
2. If they can't close Chrome, ask them to run:
   `google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/krometrail-chrome <url>`
   Then use: `krometrail chrome start --attach`
3. Do NOT use `pkill -f chrome` — this kills Electron apps (Discord, VS Code, Unity Hub, etc.)

### Tips

- Markers are your anchors — search and diff around them
- Step markers (`step:N:action:detail`) appear automatically during `run-steps` execution
- Use `--around-marker` in overview to focus on a specific moment
- Framework patterns (stale closures, infinite re-renders, missing cleanup) are auto-detected when `--framework-state` is enabled
- Screenshots are captured automatically at markers, on errors, and during step execution
- **200 response but empty/broken page?** The response body may contain streaming errors or unexpected content. Use `inspect --event <id> --include network_body` to see the actual response. Also check server logs (Docker, process output) — the error may be server-side
- **Screenshot shows the same broken state after retry?** Use `reload` (not navigate) to force a full refresh. SPAs may restore cached client-side state on navigate
