---
title: session_inspect
description: Deep-dive into a specific event with full context and the nearest screenshot.
---

# session_inspect

Get complete details for a specific event — full request/response bodies, stack traces, component state, and the nearest screenshot captured at that moment.

## Usage

::: code-group

```bash [CLI]
krometrail session inspect <session-id> --event-id <event-id>
```

```json [MCP: session_inspect]
{
	"session_id": "abc123",
	"event_id": "evt_98f3a"
}
```

:::

## What You Get

The output depends on event type:

**Network events** — full URL, method, headers, request body, response status, response headers, response body (up to limit), timing breakdown.

**Console events** — full message text, log level, all arguments, stack trace with file/line/column.

**Framework state events** — component name, component tree path, change type (mount/update/unmount), full state/props diff, render count, trigger source.

**Framework error events** — bug pattern name, severity, detailed explanation, evidence (render counts, unchanged deps, affected consumers, etc.), and the component tree path.

**DOM mutation events** — mutation type, target element selector, added/removed nodes, attribute changes.

**Storage events** — key, old value, new value, storage type (localStorage/sessionStorage), originating tab.

All event types include: timestamp, event type, tab ID, and the nearest screenshot taken during the session.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string | The recording session |
| `event_id` | string | The specific event to inspect (from `session_search` or `session_overview`) |
| `include_screenshot` | boolean | Include nearest screenshot in response (default: true) |

## Workflow

`session_inspect` is used after `session_search` narrows down candidates:

```bash
# 1. Find candidate events
krometrail session search <session-id> --event-types network_response --status-codes 500

# 2. Inspect the most suspicious one
krometrail session inspect <session-id> --event-id evt_98f3a

# 3. The response includes full body, headers, timing, and a screenshot
#    showing exactly what was on screen when the error occurred
```

## Screenshot Context

Every `session_inspect` response includes the nearest screenshot — whichever screenshot was taken closest in time to the event. This is particularly useful for:

- Seeing what the UI looked like when a network error occurred
- Confirming which user action triggered a React re-render loop
- Correlating a console error with a visible UI state

See [Markers & Screenshots](../markers-screenshots) for how screenshot capture is configured.
