---
name: krometrail-chrome
description: Browser observation for AI agents. Use when the user has recorded a browser session with markers for you to investigate, when a web app has network errors or unexpected behavior, or when you need to understand what happened in the browser at a specific moment. Captures network requests, console output, DOM mutations, framework state (React, Vue), and screenshots — all searchable, inspectable, and diffable.
license: MIT
compatibility: Requires Chrome/Chromium. Works with any MCP-compatible agent or via CLI.
metadata:
  author: krometrail
  version: "0.1"
allowed-tools: Bash(krometrail:*)
---

# Krometrail — Browser Observation

Use krometrail's browser tools when the user has recorded a browser session for you to investigate, or when you need to capture and analyze what's happening in a web application.

## When to use

- The user reproduced a bug in the browser and dropped markers — investigate the session
- A web app has network errors, console errors, or unexpected behavior — search and inspect the recording
- You need to understand what happened in the browser at a specific moment — use inspect and diff
- You need to compare browser state before and after an action — use diff between markers

## MCP tools

### Recording control

| Tool | Purpose |
|------|---------|
| `chrome_start` | Launch Chrome and start recording (network, console, DOM, framework state) |
| `chrome_status` | Check recording state |
| `chrome_mark` | Place a named marker at the current moment |
| `chrome_stop` | Stop recording — session is saved for investigation |

### Session investigation

| Tool | Purpose |
|------|---------|
| `session_list` | List recorded sessions (filter by errors, URL, etc.) |
| `session_overview` | Structured overview of a session — event counts, markers, errors |
| `session_search` | Search events by text, status codes, event types, framework patterns |
| `session_inspect` | Deep-dive into a specific event, marker, or timestamp |
| `session_diff` | Compare two moments in a session (before/after a marker) |
| `session_replay_context` | Generate reproduction steps or test scaffolds (Playwright, Cypress) |

### Example: investigate a user-reported browser bug

The user reproduced the bug and placed markers. You investigate:

```
session_list({ has_errors: true })
# → Shows sessions with errors

session_overview({ session_id: "abc123", around_marker: "checkout broke" })
# → Network errors, console errors, framework state around the marker

session_search({ session_id: "abc123", status_codes: [500], query: "payment" })
# → POST /api/orders → 500, response body with error details

session_inspect({ session_id: "abc123", event_id: "evt_42" })
# → Full request/response headers, body, timing

session_diff({ session_id: "abc123", from: "marker:form loaded", to: "marker:checkout broke" })
# → What changed: new network errors, state mutations, console errors
```

## CLI commands

If using krometrail via CLI:

**Recording:**
```bash
krometrail browser start --url http://localhost:3000 --profile krometrail
krometrail browser start --url http://localhost:3000 --framework-state
krometrail browser start --attach
krometrail browser mark "submitted form"
krometrail browser status
krometrail browser stop
```

**Investigation:**
```bash
krometrail browser sessions --has-errors
krometrail browser overview <session-id>
krometrail browser overview <session-id> --around-marker <marker-id>
krometrail browser search <session-id> --query "payment failed"
krometrail browser search <session-id> --status-codes 500
krometrail browser search <session-id> --framework react --pattern stale_closure
krometrail browser inspect <session-id> --event <event-id>
krometrail browser inspect <session-id> --marker <marker-id>
krometrail browser diff <session-id> --before <ts> --after <ts>
krometrail browser diff <session-id> --from-marker "loaded" --to-marker "error"
krometrail browser replay-context <session-id> --format playwright
krometrail browser export <session-id> --format har --output session.har
```

See [references/chrome.md](references/chrome.md) for the full browser reference.

## What gets captured

- **Network**: All XHR/fetch requests with headers, bodies, status codes, timing, WebSocket frames
- **Console**: All console output with levels, arguments, and stack traces
- **DOM mutations**: Structural changes — forms, dialogs, sections
- **User input**: Clicks, form submissions, field changes
- **Screenshots**: At markers and on errors
- **Storage**: localStorage/sessionStorage mutations
- **Framework state** (if enabled): React/Vue component lifecycles, state diffs, bug patterns

## Investigation strategy

1. **Start with the overview.** `session_overview` shows event counts, markers, and errors at a glance.
2. **Narrow with search.** Use `session_search` to find specific errors, status codes, or framework patterns.
3. **Deep-dive with inspect.** Once you find a suspicious event, `session_inspect` gives full details.
4. **Compare with diff.** Use `session_diff` between markers to see what changed — new errors, state mutations, network failures.
5. **Generate reproduction.** Use `session_replay_context` to produce test scaffolds or step-by-step reproduction instructions.

### Tips

- Markers are your anchors — search and diff around them
- Use `--around-marker` in overview to focus on a specific moment
- Framework patterns (stale closures, infinite re-renders, missing cleanup) are auto-detected when `--framework-state` is enabled
- Screenshots are captured automatically at markers and on errors — visual proof of what the user saw
