---
title: session_diff
description: Compare two moments in a session — URL, storage, network, console, and framework state changes.
---

# session_diff

Compare the state of the application between two points in a recorded session. Use this to understand what changed between "working" and "broken" states.

## Usage

::: code-group

```bash [CLI]
# Compare by timestamp (ms from session start)
krometrail session diff <session-id> --from 5000 --to 15000

# Compare relative to markers
krometrail session diff <session-id> --from-marker "page loaded" --to-marker "error appeared"
```

```json [MCP: session_diff]
// By timestamp
{
	"session_id": "abc123",
	"from_ms": 5000,
	"to_ms": 15000
}

// By marker names
{
	"session_id": "abc123",
	"from_marker": "page loaded",
	"to_marker": "error appeared"
}
```

:::

## What Gets Compared

**URL changes** — navigation events between the two timestamps. Shows the sequence of pages visited.

**Storage diffs** — localStorage and sessionStorage changes. Keys added, removed, or modified with old/new values.

**Network summary** — requests made in the window, grouped by status code. Highlights new failures that appeared in the `to` window but not the `from` window.

**Console changes** — new console errors or warnings that appeared between the timestamps.

**Framework state changes** — component mount/unmount events and state changes. Shows which React or Vue components changed state during the window.

**Screenshot comparison** — nearest screenshots at each timestamp for visual reference.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string | The recording session |
| `from_ms` | number | Start timestamp (ms from session start) |
| `to_ms` | number | End timestamp (ms from session start) |
| `from_marker` | string | Use a named marker as the start point |
| `to_marker` | string | Use a named marker as the end point |

Use marker names or timestamps — not both. Get timestamps and marker names from `session_overview`.

## Example: Diagnosing a Form Submission Bug

```bash
# Record a session with markers
krometrail browser start http://localhost:3000
# ... fill out the form in Chrome ...
krometrail browser mark "form submitted"
# ... observe the error ...
krometrail browser mark "error displayed"
krometrail browser stop

# Diff between the two markers
krometrail session diff <session-id> \
	--from-marker "form submitted" \
	--to-marker "error displayed"
```

The diff output shows:
- Which network request failed (and its response body)
- Whether localStorage was modified unexpectedly
- Which React components re-rendered and with what state changes
- Console errors logged in that window

This is often enough to identify the root cause without inspecting individual events.
