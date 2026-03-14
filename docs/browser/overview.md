---
title: Browser Observation Overview
description: What browser observation captures, why it matters for AI agents, and how to get started.
---

# Browser Observation

Krometrail connects to Chrome via CDP and records everything happening in a browser session — without requiring any changes to the application code.

## Why It Matters for Agents

AI agents debugging web applications face a fundamental problem: they can read source code and error messages, but they cannot see what the browser is actually doing. Network requests that return unexpected data, console errors that appear only under specific conditions, React components re-rendering in loops — these are invisible without direct browser access.

Browser observation gives agents a complete session transcript they can query after the fact, or in real time.

## What Gets Captured

| Category | Details |
|----------|---------|
| **Network** | Every request/response with headers, bodies, status codes, timing, and WebSocket frames |
| **Console** | All console output with levels, arguments, and stack traces |
| **DOM mutations** | Structural changes: forms, dialogs, sections — not every attribute tweak |
| **User input** | Clicks, form submissions, field changes |
| **Screenshots** | Periodic snapshots and navigation-triggered captures |
| **Storage** | localStorage/sessionStorage mutations and cross-tab events |
| **Framework state** | React and Vue component lifecycles, state/prop diffs, store mutations |
| **Framework errors** | Auto-detected anti-patterns (stale closures, infinite re-renders, missing cleanup) |

## How It Works

1. Krometrail launches (or connects to) a Chrome instance via CDP
2. A recording session captures events into a SQLite-backed store
3. The agent investigates the recorded session using search, inspect, and diff tools
4. Framework state (if enabled) is captured via injected DevTools hook scripts that fire before any page code runs

## Quick Start

```bash
# Start recording
krometrail browser start http://localhost:3000 --framework-state

# Do things in the browser...
krometrail browser mark "submitted the form"

# Stop recording
krometrail browser stop

# Investigate
krometrail session list --has-errors
krometrail session search <session-id> --event-types network_response --status-codes 500
```

## Next Steps

- [Recording Sessions](./recording-sessions) — `chrome_start`, `chrome_stop`, markers, tab filtering
- [Search](./investigation-tools/search) — Full-text and structured event search
- [Inspect](./investigation-tools/inspect) — Deep-dive into individual events
- [Diff](./investigation-tools/diff) — Compare two moments in a session
- [React Observation](./framework-observation/react) — Component lifecycles and bug patterns
- [Vue Observation](./framework-observation/vue) — Vue 2/3, Pinia, and Vuex
