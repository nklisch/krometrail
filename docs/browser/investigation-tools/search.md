---
title: Search
description: Your agent searches recorded session events by text, type, status code, framework pattern, and more.
---

# Search

Your agent can search everything recorded in your session — every network request, console message, framework event, storage change, and marker. Search is usually the first thing your agent does after you finish reproducing a bug.

## What Your Agent Can Search For

Your agent can search and filter the session in a variety of ways:

- **Full-text search** — search across all event data for a keyword or phrase, like "payment failed" or "card_declined"
- **By event type** — narrow to specific categories like network responses, console errors, or framework state changes
- **By HTTP status code** — find all 4xx or 5xx responses, or a specific code like 500 or 401
- **By framework pattern** — search for detected bug patterns like stale closures, infinite re-renders, or missing cleanup
- **By time range** — search only events that occurred within a specific window of the session
- **Around markers** — scope the search to events between two named markers you placed during recording

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

**Finding failed API calls:** Your agent searches for network responses with 4xx or 5xx status codes, returning every request that received an error response during the session.

**Finding React component errors:** Your agent searches for `framework_error` events and filters to React patterns — stale closures, infinite re-renders, and similar detected problems.

**Narrowing to a specific moment:** After you place markers at "form submitted" and "error appeared", your agent can search for console errors and network failures scoped to just that window — filtering out everything else that happened in the session.

**Tracing a keyword through the session:** Your agent can search for a specific string like an error message or a user ID across all event types, finding every place it appeared in the network traffic, console output, and storage changes.

Search results include event IDs that your agent uses to pull up full event details with [Inspect](./inspect).
