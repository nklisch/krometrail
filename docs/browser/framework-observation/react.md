---
title: React State Observation
description: Track React component lifecycles, state/prop diffs, re-renders, and auto-detected bug patterns.
---

# React State Observation

When enabled, Krometrail injects a DevTools hook script before page code runs and hooks into React's reconciler via `__REACT_DEVTOOLS_GLOBAL_HOOK__`. This captures component lifecycles, state changes, and re-render patterns — without modifying application code.

## Enabling React Observation

```bash
# CLI
krometrail browser start http://localhost:3000 --framework-state react

# Or enable all frameworks
krometrail browser start http://localhost:3000 --framework-state
```

```json
// MCP: chrome_start
{ "url": "http://localhost:3000", "framework_state": ["react"] }
```

::: warning Timing requirement
The injection script must run before React's module code executes. Start the recording session first, then navigate to the URL. If the page is already loaded when recording starts, framework observation will not capture initial mount events.
:::

## What Gets Tracked

**Component lifecycles** — mount, update, and unmount events for every user component in the tree. Each event includes:
- Component name (with ForwardRef/Memo unwrapping)
- Component path in the tree (e.g., `"App > Layout > Sidebar > UserProfile"`)
- Render count
- Change type: `mount`, `update`, or `unmount`

**State and prop diffs** — for update events, the before/after diff for each changed state hook and prop:
```json
{
	"changeType": "update",
	"changes": [
		{ "key": "state[0]", "prev": 5, "next": 6 },
		{ "key": "props.isLoading", "prev": true, "next": false }
	]
}
```

**Render trigger source** — why the component re-rendered:
- `"state"` — own state changed
- `"props"` — parent passed new props
- `"context"` — consumed context value changed
- `"parent"` — parent re-rendered, passing same-reference props

## Auto-Detected Bug Patterns

Krometrail detects these anti-patterns automatically during recording:

### Infinite Re-render (`infinite_rerender`)

A component renders more than 15 times in a 1-second window. Typically caused by `setState` inside `useEffect` without a deps array, or a deps array that always produces new values.

**Severity: high**

### Stale Closure (`stale_closure`)

A hook's dependency array has been unchanged for 5+ renders while the component's state has changed. Suggests the hook is reading a stale value from a closure.

**Severity: medium**

### Missing Cleanup (`missing_cleanup`)

A `useEffect` has no cleanup function but re-runs on re-renders (indicating it creates subscriptions or timers that may leak).

**Severity: low**

### Excessive Context Re-renders (`excessive_context_rerender`)

A context provider's value changed, causing 20+ consumer components to re-render. Suggests the context value should be memoized or the context should be split.

**Severity: medium**

## Searching Framework Events

After recording, find React events using `session_search`:

```bash
# Find all high-severity React errors
krometrail session search <session-id> --event-types framework_error --framework react

# Find infinite re-render patterns
krometrail session search <session-id> --framework react --pattern infinite_rerender

# Find all re-renders of a specific component
krometrail session search <session-id> "UserProfile" --event-types framework_state
```

## React Version Support

The observer hooks into React's fiber architecture via `__REACT_DEVTOOLS_GLOBAL_HOOK__`. Supported versions:

- React 16+ (fiber-based)
- Works alongside React DevTools extension (hooks are patched, not replaced)
- React 18 concurrent mode is supported
