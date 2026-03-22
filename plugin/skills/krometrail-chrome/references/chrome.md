# Browser / Chrome Recording Reference

## Starting a recording

```bash
# Launch Chrome and record at a URL
krometrail chrome start --url http://localhost:3000

# With framework state capture (React, Vue)
krometrail chrome start --url http://localhost:3000 --framework-state

# Attach to an already-running Chrome
krometrail chrome start --attach
```

The `--profile krometrail` flag uses a dedicated Chrome profile so recordings don't interfere with your regular browser.

## During recording

```bash
# Place a marker (annotate a moment for later investigation)
krometrail chrome mark "submitted the checkout form"
krometrail chrome mark "saw error message"

# Check recording status
krometrail chrome status
```

## Driving the browser with batch steps (MCP)

Use `chrome_run_steps` to execute a sequence of browser actions in one call:

```
chrome_run_steps({
  steps: [
    { action: "navigate", url: "/login" },
    { action: "fill", selector: "#email", value: "test@example.com" },
    { action: "fill", selector: "#password", value: "secret" },
    { action: "submit", selector: "#login-form" },
    { action: "wait_for", selector: ".dashboard", timeout: 5000 },
    { action: "screenshot", label: "after-login" }
  ]
})
```

Each step is auto-marked and auto-screenshotted. All events are captured in the recording.

**Actions:** `navigate`, `reload`, `click`, `fill`, `select`, `submit`, `type`, `press_key`, `hover`, `scroll_to`, `scroll_by`, `wait`, `wait_for`, `wait_for_navigation`, `wait_for_network_idle`, `screenshot`, `mark`, `evaluate`

**`reload` vs `navigate`:** Use `reload` for a full page refresh (clears all client-side state). Navigating to the same URL may hit SPA client-side routing cache and not actually re-fetch.

**`press_key`:** Press Enter, Tab, Escape, arrow keys, etc. Use for submitting forms without a submit button (`{ action: "press_key", key: "Enter", selector: "#chat-input" }`), keyboard shortcuts, or navigation.

**Capture config:** `capture: { screenshot: "all" | "none" | "on_error", markers: true | false }`

**Named scenarios:** Save with `name` + `save: true`, replay later with just `name`. Session-scoped (in-memory).

## Refreshing (clean slate)

```bash
# Reload the page and clear all events/markers — start fresh without restarting
krometrail chrome refresh
```

MCP: `chrome_refresh()` — no parameters. Reloads the current page, clears the event buffer and markers, waits for page load, and returns updated session info. The recording session stays active.

Use this after making code changes that require a page reload, or when you want to start a fresh investigation on the same page.

## Stopping

```bash
krometrail chrome stop
krometrail chrome stop --close-browser
```

## Investigating recorded sessions

```bash
# List sessions
krometrail chrome sessions
krometrail chrome sessions --has-errors

# Overview of a session
krometrail chrome overview <session-id>
krometrail chrome overview <session-id> --around-marker <marker-id>

# Search events
krometrail chrome search <session-id> --query "payment failed"
krometrail chrome search <session-id> --status-codes 422,500
krometrail chrome search <session-id> --framework react --pattern stale_closure

# Inspect a specific event
krometrail chrome inspect <session-id> --event <event-id>
krometrail chrome inspect <session-id> --marker <marker-id>

# Diff two moments
krometrail chrome diff <session-id> --before <timestamp> --after <timestamp>
krometrail chrome diff <session-id> --from-marker "loaded" --to-marker "error"

# Generate reproduction steps
krometrail chrome replay-context <session-id>
krometrail chrome replay-context <session-id> --format playwright

# Export as HAR
krometrail chrome export <session-id> --format har --output session.har
```

## What gets captured

- **Network**: All XHR/fetch requests with headers, bodies, status codes, timing
- **Console**: All console output with levels and stack traces
- **DOM mutations**: Structural changes (forms, dialogs, sections)
- **User input**: Clicks, form submissions, field changes
- **Screenshots**: At markers and on errors
- **Storage**: localStorage/sessionStorage mutations
- **Framework state** (if enabled): React/Vue component lifecycles, state diffs, bug patterns

## Common investigation workflow

1. User reproduces the bug in Chrome, drops markers at key moments
2. `krometrail chrome sessions --has-errors` — find the session
3. `krometrail chrome overview <id>` — get the big picture (event counts, markers, errors)
4. `krometrail chrome search <id> --status-codes 500` — find failing requests
5. `krometrail chrome inspect <id> --event <event-id>` — get full request/response details
6. `krometrail chrome diff <id> --from-marker "loaded" --to-marker "error"` — what changed

**Gotcha:** If a request returns 200 but the page is empty/broken, the response body may contain streaming errors or error JSON. Use `inspect --event <id> --include network_body` to see the actual content. Also check server logs — the error may be server-side and not visible in the browser recording.
