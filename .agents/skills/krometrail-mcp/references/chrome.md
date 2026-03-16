# Chrome Browser Recording (CDP)

## Quick Start
```
chrome_start(url: 'http://localhost:3000', profile: 'krometrail')
# ... interact in the browser ...
chrome_mark(label: 'submitted form')
chrome_stop()
session_list()
```

## Launching Chrome

### Isolated instance (recommended)
Always pass `profile` to avoid conflicting with your regular Chrome:
```
chrome_start(profile: 'krometrail', url: 'http://localhost:3000')
```
Creates a separate Chrome with its own user-data-dir under `~/.krometrail/chrome-profiles/krometrail`. Independent cookies, storage, login state.

### Attach to existing Chrome
Chrome must have been started with `--remote-debugging-port`:
```sh
google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/cdp-chrome
```
Then:
```
chrome_start(attach: true)
```

## CDP Connection Errors

**Error: "Chrome CDP not available after 10000ms"**

Likely cause: Chrome is already running without the debug port.

Fix options (returned in the error message):
1. `chrome_start(profile: 'krometrail')` — isolated instance, no conflict (recommended)
2. `pkill -f chrome && google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/...`
3. Start Chrome with debug port, then `chrome_start(attach: true)`

**Error: "Chrome not found"**

Chrome isn't in PATH. Install Chrome or specify the path manually.
Common locations:
- Linux: `/usr/bin/google-chrome`, `/usr/bin/chromium`
- macOS: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`

## Headless Environments
Chrome needs a display. Options:
- `DISPLAY=:0` if a display server is running
- Start `Xvfb :99 &` then `DISPLAY=:99 chrome_start(...)`
- Or: `chrome_start(attach: true)` and start Chrome manually with `--headless=new`

## Driving the Browser with Steps

Use `chrome_run_steps` to execute a batch of browser actions in one call. Requires an active recording session (`chrome_start`).

```
chrome_run_steps({
  steps: [
    { action: "navigate", url: "/login" },
    { action: "fill", selector: "#email", value: "test@example.com" },
    { action: "fill", selector: "#password", value: "secret" },
    { action: "submit", selector: "#login-form" },
    { action: "wait_for", selector: ".dashboard", timeout: 5000 },
    { action: "evaluate", expression: "document.title" },
    { action: "screenshot", label: "logged-in" }
  ]
})
```

Each step is auto-marked (e.g. `step:1:navigate:/login`) and auto-screenshotted. Use `session_search` with `around_marker` to investigate what happened at any step.

### All actions

| Action | Key Params | Notes |
|--------|-----------|-------|
| `navigate` | `url` | Absolute or relative (resolved against current origin) |
| `reload` | — | Reloads current page |
| `click` | `selector` | CSS selector — throws if not found |
| `fill` | `selector`, `value` | Sets value with React/Vue-compatible native setter |
| `select` | `selector`, `value` | Selects dropdown option by value |
| `submit` | `selector` | Submits form via `requestSubmit()` |
| `type` | `selector`, `text`, `delay_ms?` | Keystroke-by-keystroke (for autocomplete, etc.) |
| `hover` | `selector` | Dispatches mouse move + mouseover events |
| `scroll_to` | `selector` | Scrolls element into view |
| `scroll_by` | `x?`, `y?` | Scrolls page by pixel delta |
| `wait` | `ms` | Fixed delay |
| `wait_for` | `selector`, `state?`, `timeout?` | Wait for element visible/hidden/attached (default: visible, 5s) |
| `wait_for_navigation` | `url?`, `timeout?` | Wait for page navigation (default: 10s) |
| `wait_for_network_idle` | `idle_ms?`, `timeout?` | Wait for no network requests (default: 500ms idle, 10s timeout) |
| `screenshot` | `label?` | Explicit screenshot (beyond auto-capture) |
| `mark` | `label` | Explicit marker (beyond auto-markers) |
| `evaluate` | `expression` | Run JS in page, return value in `returnValue` |

### Capture config

```
chrome_run_steps({
  steps: [...],
  capture: {
    screenshot: "all",    // "all" (default) | "none" | "on_error"
    markers: true          // true (default) | false
  }
})
```

Per-step override: add `screenshot: false` to any step to skip its auto-screenshot.

### Named scenarios (save + replay)

```
// Save
chrome_run_steps({ name: "login-flow", steps: [...], save: true })

// Replay (no steps needed)
chrome_run_steps({ name: "login-flow" })
```

Scenarios are session-scoped (in-memory on the daemon). They disappear when the daemon stops, but recording data persists on disk.

### Error handling

Steps execute sequentially. If a step fails (element not found, timeout), execution stops and the result shows which step failed and why. Use `capture: { screenshot: "on_error" }` to only screenshot failures.

## Markers
Place markers at key moments so you can find them later:
```
chrome_mark(label: 'clicked submit')
chrome_mark(label: 'error appeared')
chrome_mark()  # unlabeled — timestamped only
```

Use `around_marker` in `session_overview` or `session_search` to center investigation on a marker.

## Annotations (Lightweight Markers)

When recording a browser session, you can instrument the application's source code
with lightweight annotations that appear in the recording timeline without triggering
expensive screenshots or persistence snapshots.

### When to use annotations vs markers

- **Annotations** (`window.__krometrail.mark()`): For frequent, programmatic events
  in application code — render cycles, state transitions, feature flag checks, API
  call starts/ends. Safe in loops. Automatically coalesced when fired rapidly.

- **Markers** (`chrome_mark` tool): For significant moments you want to investigate
  later — error reproduction points, "before" and "after" a user action. Triggers
  screenshot capture and event persistence.

### How to add annotations to application code

Add calls to the application source code (they no-op when krometrail isn't recording):

```javascript
// Simple annotation
window.__krometrail?.mark('checkout-started');

// With severity and context data
window.__krometrail?.mark('payment-failed', {
  severity: 'high',
  data: { errorCode: 'card_declined', amount: 42.99 }
});

// Promote to full marker (triggers screenshot + persistence)
window.__krometrail?.mark('critical-error', { marker: true });
```

### Querying annotations

Use `session_search` with `event_types: ["annotation"]` to find annotations,
or use `contains_text` to search by label.

## Tab Recording
```
chrome_start(all_tabs: true)                    # all tabs
chrome_start(tab_filter: '**/app/**')           # tabs matching URL glob
chrome_start()                                  # first/active tab only (default)
```

## Investigating Sessions

```
session_list()                                                  # list recorded sessions
session_overview(session_id: 'latest')                          # timeline, markers, errors
session_search(session_id: 'latest', status_codes: [422, 500]) # find bad requests
session_search(session_id: 'latest', query: 'validation error')# full-text search
session_inspect(session_id: 'latest', event_id: '...')         # full event detail + request bodies
session_diff(session_id: 'latest', before: '...', after: '...')# compare two moments
session_replay_context(session_id: 'latest', format: 'reproduction_steps')
session_replay_context(session_id: 'latest', format: 'test_scaffold', test_framework: 'playwright')
```

> **Tip:** All `session_*` tools accept `session_id: "latest"` to target the most recent session, or a specific UUID from `session_list()`.

## What Gets Recorded
- Navigation (URL changes, page loads)
- Network requests and responses (headers + bodies)
- Console output (log, warn, error)
- Unhandled JS errors and exceptions
- User input (clicks, form fills, keypresses)
- DOM mutations (significant changes)
- Form state snapshots
- Screenshots at key moments
- WebSocket frames
- Performance entries
- Storage changes (localStorage, sessionStorage, cookies)
