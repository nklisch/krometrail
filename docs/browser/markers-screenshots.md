---
title: Markers & Screenshots
description: How to place timeline markers and how screenshot capture works.
---

# Markers & Screenshots

## Markers

Markers annotate the recording timeline at significant moments. They are the primary tool for targeting `session_diff` comparisons and scoping `session_replay_context`.

### Placing Markers

::: code-group

```bash [CLI]
krometrail browser mark "user submitted checkout form"
krometrail browser mark "error modal appeared"
krometrail browser mark "payment failed"
```

```json [MCP: chrome_mark]
{ "label": "user submitted checkout form" }
```

:::

Markers are timestamped at the moment they are created. They appear in `session_overview` output and can be referenced by name in `session_diff` and `session_replay_context`.

### Using Markers in Investigation

```bash
# Get all markers from a session
krometrail session overview <session-id>

# Diff between two marker points
krometrail session diff <session-id> \
	--from-marker "user submitted checkout form" \
	--to-marker "error modal appeared"

# Generate reproduction steps scoped to marker range
krometrail session replay-context <session-id> \
	--from-marker "page loaded" \
	--to-marker "error modal appeared"
```

## Screenshots

Screenshots are captured automatically during recording at two triggers:

**Periodic capture** — a screenshot is taken at a configurable interval (default: every 5 seconds) while recording is active.

**Navigation-triggered** — a screenshot is taken on every page navigation (URL change).

### Screenshots in Investigation

Every `session_inspect` response includes the nearest screenshot to the event being inspected. This shows what the UI looked like at the moment the event occurred — useful for correlating a network error with a visible UI state.

```bash
krometrail session inspect <session-id> --event-id <event-id>
# Response includes: event details + nearest screenshot
```

### Screenshot Format

Screenshots are stored as PNG files in the session database and returned as base64-encoded data in API responses, or saved to disk when using the CLI with `--save-screenshots`.

## Tips for Effective Marking

- **Mark before and after actions** — "before form submit" and "after form submit" gives `session_diff` a precise window
- **Mark when errors appear** — place a marker immediately when you see unexpected behavior in the browser
- **Name markers descriptively** — marker labels appear in diffs and replay contexts, so descriptive names make the output easier to read
- **Mark test scenario boundaries** — if testing multiple scenarios in one session, mark the start of each scenario to keep them separable
