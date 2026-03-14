---
title: Recording & Controls
description: Start a browser recording session and use the in-browser control panel to annotate what you see.
---

# Recording & Controls

A recording session captures everything happening in Chrome from start to stop. Recordings are stored locally and can be investigated by your agent afterward.

## Starting a Recording

Your agent typically starts a recording for you as part of its debugging workflow. If you want to start one yourself, the CLI command is:

```bash
krometrail browser start http://localhost:3000
```

If your app uses React or Vue and you want component-level state captured, you can enable framework observation:

```bash
# Enable all supported frameworks
krometrail browser start http://localhost:3000 --framework-state

# Enable only React
krometrail browser start http://localhost:3000 --framework-state react
```

> **Note:** Framework state must be enabled before the page loads. The injection scripts need to run before React or Vue module code executes. Starting the recording and then navigating to the URL ensures the timing is correct.

## The Control Panel

When a recording is active, Krometrail injects a floating control panel into Chrome — fixed in the bottom-right corner of the window, 16px from the edges.

```
┌─────────────────────────┐
│ ● krometrail            │  ← green dot = recording active
├─────────────────────────┤
│  ◎ Mark    📷 Snap      │  ← two action buttons
├─────────────────────────┤
│  ⏱ auto: 5s             │  ← screenshot interval
└─────────────────────────┘
```

The panel has a dark slate background and stays on top of your app's UI without interfering with it.

### The Recording Indicator

The green dot next to "krometrail" confirms the recording is active. If you don't see the panel or the dot is missing, the session may not have started correctly.

### ◎ Mark

Click **Mark** to place a named marker on the timeline at the current moment. A text input appears so you can describe what just happened — "form submitted", "error appeared", "payment failed".

When the marker is saved, the button flashes green and shows **"Marked!"** for one second.

Markers are the primary way to help your agent focus its investigation. Your agent can diff the state between two markers, scope a search to a marker range, or use markers to generate reproduction steps.

### 📷 Snap

Click **Snap** to immediately capture a screenshot of the current page.

When the screenshot is saved, the button flashes blue and shows **"Saved!"** for one second.

Manual snaps are useful when something catches your eye — an unexpected UI state, a visual glitch, a loading spinner that shouldn't be there.

### Auto-Capture Footer

The footer shows the current periodic screenshot interval, e.g. **"auto: 5s"**. A screenshot is also taken automatically on every page navigation. If auto-capture is disabled, the footer shows **"auto: off"**.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+M` | Place a marker (same as clicking ◎ Mark) |
| `Ctrl+Shift+S` | Capture a screenshot (same as clicking 📷 Snap) |

Keyboard shortcuts work even when you're focused inside your app, so you don't have to click the panel while in the middle of an interaction.

## Stopping a Recording

Your agent handles stopping the recording when it's done. If you need to stop it yourself, you can close Chrome or ask your agent to stop the session. All buffered events are persisted to the database on stop, and the session remains queryable afterward.

## Tips

- **Mark before and after the bug** — "before form submit" and "after error appeared" gives your agent a precise window to diff
- **Mark when you see something unexpected** — you don't need to understand what went wrong, just annotate the moment
- **Use descriptive labels** — marker names appear in your agent's diffs and reproduction steps, so clear names make the output easier to read
- **Mark scenario boundaries** — if you're testing multiple scenarios in one session, mark the start of each to keep them separable
