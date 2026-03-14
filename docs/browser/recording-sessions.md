---
title: Recording Sessions
description: Start, mark, and stop browser recording sessions with chrome_start, chrome_mark, and chrome_stop.
---

# Recording Sessions

A recording session captures everything happening in Chrome from start to stop. Recordings are stored in a local SQLite database and can be investigated with the session tools afterward.

## Starting a Recording

::: code-group

```bash [CLI]
# Basic recording
krometrail browser start http://localhost:3000

# With framework state observation (React + Vue)
krometrail browser start http://localhost:3000 --framework-state

# Framework-specific
krometrail browser start http://localhost:3000 --framework-state react

# Filter to specific tabs
krometrail browser start http://localhost:3000 --tab-filter "localhost:3000"
```

```json [MCP: chrome_start]
// Basic
{ "url": "http://localhost:3000" }

// With framework state
{ "url": "http://localhost:3000", "framework_state": true }

// React only
{ "url": "http://localhost:3000", "framework_state": ["react"] }

// React + Vue
{ "url": "http://localhost:3000", "framework_state": ["react", "vue"] }
```

:::

`chrome_start` returns a `session_id` used to reference this recording in investigation tools.

## Checking Recording Status

::: code-group

```bash [CLI]
krometrail browser status
```

```json [MCP: chrome_status]
{}
```

:::

Returns current recording state, event counts by type, and active tab list.

## Placing Markers

Markers annotate the timeline at significant moments. They show up in session overviews and make it easy to target `session_diff` comparisons.

::: code-group

```bash [CLI]
krometrail browser mark "user submitted the checkout form"
krometrail browser mark "error appeared"
```

```json [MCP: chrome_mark]
{ "label": "user submitted the checkout form" }
```

:::

Markers are timestamped and included in the session overview. Use them liberally — they cost nothing and make investigation much easier.

## Stopping a Recording

::: code-group

```bash [CLI]
krometrail browser stop
```

```json [MCP: chrome_stop]
{}
```

:::

Persists all buffered events to the database and closes the recording. The session remains queryable afterward.

## Listing Recorded Sessions

::: code-group

```bash [CLI]
# All sessions
krometrail session list

# Only sessions with errors
krometrail session list --has-errors

# Filter by URL
krometrail session list --url "localhost:3000"
```

```json [MCP: session_list]
{ "has_errors": true }
{ "url_filter": "localhost:3000" }
```

:::

## Session Overview

Get a structured summary of what happened during a session:

```bash
krometrail session overview <session-id>
```

Returns: navigation sequence, markers, error events, network summary (counts by status code), and framework summary (if framework state was enabled).

## Tips

- **Enable framework state before loading the page** — the injection scripts must run before React or Vue module code executes. Starting the recording with `--framework-state` and then navigating to the URL ensures correct timing.
- **Use markers at decision points** — before clicking a button, after a form submits, when an error appears. This makes `session_diff` comparisons precise.
- **Multiple sessions** — each `chrome_start` creates a new session. Old sessions persist until you delete them.
