---
title: Browser Observation Overview
description: What browser observation captures, why it matters for AI agents, and how to get started.
---

# Browser Observation

Krometrail connects to Chrome via CDP and records everything happening in a browser session — without requiring any changes to your application code.

## Why It Matters

When you're debugging a web app, you can see the bug happen — a spinner that never stops, a form that silently fails, a page that loads wrong data. But when you hand the problem to your coding agent, all it has is source code and maybe an error message.

Browser observation bridges that gap. You browse your app normally, drop markers when something goes wrong, and your agent gets a complete session transcript — network requests, console errors, framework state, screenshots — everything it needs to investigate without you describing the bug in chat.

## What Gets Captured

| Category | Details |
|----------|---------|
| **Network** | Every request/response with headers, bodies, status codes, timing, and WebSocket frames |
| **Console** | All console output with levels, arguments, and stack traces |
| **DOM mutations** | Structural changes: forms, dialogs, sections — not every attribute tweak |
| **User input** | Clicks, form submissions, field changes |
| **Screenshots** | Periodic snapshots, navigation-triggered captures, and manual snaps |
| **Storage** | localStorage/sessionStorage mutations and cross-tab events |
| **Framework state** | React and Vue component lifecycles, state/prop diffs, store mutations |
| **Framework errors** | Auto-detected anti-patterns (stale closures, infinite re-renders, missing cleanup) |

## How It Works

The work is split between you and your agent:

```
You (in Chrome)                     Your Agent
─────────────────                   ──────────
Browse your app normally
Click ◎ Mark at key moments
Click 📷 Snap to capture the screen
                                    Searches the session for errors
                                    Inspects individual events
                                    Diffs state between markers
                                    Generates reproduction steps
```

While you use your app and annotate important moments with the in-browser control panel, your agent works through the recorded session to trace the bug to its source.

## Typical Workflow

1. **Your agent** starts a recording session and opens Chrome to your app
2. **You** use the app — click around, fill forms, reproduce the bug
3. **You** click **◎ Mark** in the control panel at key moments ("form submitted", "page broke")
4. **Your agent** searches the recorded session, inspects events, diffs state changes, and traces the bug to source code

## Next Steps

- [Recording & Controls](./recording-sessions) — Starting a recording, the in-browser control panel, and keyboard shortcuts
- [Markers & Screenshots](./markers-screenshots) — How to annotate your session and how screenshot capture works
- [What Your Agent Sees](./investigation-tools/search) — Search, inspect, diff, and replay context
- [React Observation](./framework-observation/react) — Component lifecycles and bug patterns
- [Vue Observation](./framework-observation/vue) — Vue 2/3, Pinia, and Vuex
