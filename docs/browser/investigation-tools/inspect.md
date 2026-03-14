---
title: Inspect
description: Your agent can deep-dive into any recorded event — full request/response bodies, stack traces, component state, and a screenshot of what was on screen.
---

# Inspect

Your agent can deep-dive into any event — full request/response bodies, stack traces, component state, and a screenshot of what was on screen at that moment.

Inspect is typically used after [Search](./search) surfaces a suspicious event. Your agent takes the event ID from a search result and pulls up the complete picture.

## What Your Agent Sees

The details available depend on event type:

**Network events** — full URL, method, headers, request body, response status, response headers, response body, and timing breakdown showing DNS, connection, and transfer time.

**Console events** — full message text, log level, all arguments passed to the console call, and a stack trace with file, line, and column numbers.

**Framework state events** — component name, component tree path, change type (mount/update/unmount), full state and props diff, render count, and what triggered the change.

**Framework error events** — bug pattern name, severity, detailed explanation, evidence (render counts, unchanged deps, affected consumers, and similar), and the component tree path.

**DOM mutation events** — mutation type, target element selector, added and removed nodes, and attribute changes.

**Storage events** — key, old value, new value, storage type (localStorage or sessionStorage), and the originating tab.

All event types include: timestamp, event type, tab ID, and the nearest screenshot taken during the session.

## Screenshot Context

Every inspect response includes the nearest screenshot — whichever screenshot was taken closest in time to the event. This is particularly useful for:

- Seeing what the UI looked like when a network error occurred
- Confirming which user action triggered a React re-render loop
- Correlating a console error with a visible UI state

See [Markers & Screenshots](../markers-screenshots) for how screenshot capture works and how to take manual snaps.
