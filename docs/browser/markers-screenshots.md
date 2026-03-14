---
title: Markers & Screenshots
description: How to place timeline markers and how screenshot capture works.
---

# Markers & Screenshots

## Markers

Markers annotate the recording timeline at significant moments. They are your primary way to guide your agent's investigation — every search, diff, and replay context can be scoped to a marker range.

### Placing a Marker

Click the **◎ Mark** button in the control panel, or press **Ctrl+Shift+M** from anywhere in Chrome. A text input appears so you can label the moment — "form submitted", "error appeared", "payment failed".

When the marker is saved, the button flashes green and shows **"Marked!"** for one second, confirming it was recorded.

### Why Markers Matter

Markers anchor your agent's investigation to the moments that matter. When you mark "before submit" and "after error", your agent can:

- **Diff** the application state between those two moments — what changed in network activity, storage, component state, and console output
- **Scope a search** to just the events that happened between your markers
- **Generate reproduction steps** starting from a marker, rather than replaying the entire session

The more precisely you mark key moments, the more targeted your agent's analysis can be. You don't need to understand what went wrong — just mark the moment something looked unexpected.

## Screenshots

Screenshots are captured automatically during recording and can also be taken manually at any time.

### Manual Snaps

Click the **📷 Snap** button in the control panel, or press **Ctrl+Shift+S** from anywhere in Chrome, to immediately capture a screenshot.

When the screenshot is saved, the button flashes blue and shows **"Saved!"** for one second.

Manual snaps are useful for capturing moments that auto-capture might have missed — an unexpected loading state, a visual glitch, a dialog that appeared briefly.

### Auto-Capture

Screenshots are also captured automatically at two triggers:

**Periodic capture** — a screenshot is taken at a regular interval while recording is active. The current interval is shown in the control panel footer as **"auto: 5s"** (or whatever the configured interval is). If auto-capture is disabled, the footer shows **"auto: off"**.

**Navigation-triggered** — a screenshot is taken on every page navigation (URL change).

### Screenshots in Your Agent's Investigation

Every event your agent inspects includes the nearest screenshot — whichever screenshot was taken closest in time to the event. This lets your agent correlate a network error, a console message, or a component state change with what was actually visible on screen at that moment.

## Tips for Effective Marking

- **Mark before and after actions** — "before form submit" and "after form submit" gives your agent a precise window to analyze
- **Mark when errors appear** — place a marker as soon as you see unexpected behavior, even if you don't know what caused it
- **Name markers descriptively** — marker labels appear in diffs and replay output, so clear names make the results easier to read
- **Mark test scenario boundaries** — if you're testing multiple scenarios in one session, mark the start of each to keep them separable
- **Use Ctrl+Shift+M** — the keyboard shortcut lets you mark a moment without breaking your interaction flow in the app
