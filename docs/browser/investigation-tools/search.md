---
title: session_search
description: Full-text and structured search across recorded browser session events.
---

# session_search

Search recorded events by text, type, status code, time range, or framework pattern. Use this to locate specific network failures, console errors, or framework bugs within a recording.

## Basic Usage

::: code-group

```bash [CLI]
# Full-text search
krometrail session search <session-id> "payment failed"

# Filter by event type
krometrail session search <session-id> --event-types network_response

# Filter by HTTP status code
krometrail session search <session-id> --event-types network_response --status-codes 500,503

# Framework pattern search
krometrail session search <session-id> --framework react --pattern stale_closure

# Combine filters
krometrail session search <session-id> "checkout" --event-types network_request,network_response --status-codes 4xx
```

```json [MCP: session_search]
// Full-text
{ "session_id": "abc123", "query": "payment failed" }

// Event type filter
{ "session_id": "abc123", "event_types": ["network_response"], "status_codes": [500, 503] }

// Framework pattern
{ "session_id": "abc123", "framework": "react", "pattern": "stale_closure" }

// Time range (milliseconds since session start)
{ "session_id": "abc123", "from_ms": 5000, "to_ms": 15000 }
```

:::

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `session_id` | string | The recording session to search |
| `query` | string | Full-text search across event data |
| `event_types` | string[] | Filter to specific event types (see below) |
| `status_codes` | (number \| string)[] | HTTP status codes. Accepts exact numbers or patterns like `"4xx"`, `"5xx"` |
| `framework` | string | Filter to framework events: `"react"`, `"vue"` |
| `pattern` | string | Framework bug pattern: `"stale_closure"`, `"infinite_rerender"`, `"missing_cleanup"` |
| `from_ms` | number | Start of time range (ms from session start) |
| `to_ms` | number | End of time range (ms from session start) |
| `limit` | number | Max results to return (default: 50) |

## Event Types

| Type | Description |
|------|-------------|
| `network_request` | Outgoing HTTP requests |
| `network_response` | HTTP responses (includes body) |
| `console` | Console.log/warn/error output |
| `page_error` | Uncaught exceptions |
| `dom_mutation` | Structural DOM changes |
| `user_input` | Clicks, form submissions, field changes |
| `storage_change` | localStorage/sessionStorage mutations |
| `screenshot` | Screenshot captures |
| `framework_state` | Component mount/update/unmount |
| `framework_error` | Detected bug patterns |
| `marker` | Named markers placed during recording |

## Example Workflows

**Find all failed API calls:**
```bash
krometrail session search <session-id> --event-types network_response --status-codes 4xx,5xx
```

**Find React component errors:**
```bash
krometrail session search <session-id> --event-types framework_error --framework react
```

**Search for errors around a specific marker:**
```bash
# First get the marker timestamp from session overview
krometrail session overview <session-id>
# Then search a time window around it
krometrail session search <session-id> --event-types console,page_error --from-ms 12000 --to-ms 18000
```

Use `session_inspect` to get full event details for any result returned by search.
