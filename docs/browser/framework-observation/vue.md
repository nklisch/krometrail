---
title: Vue State Observation
description: Track Vue component lifecycles, Pinia/Vuex store mutations, computed watchers, and framework events.
---

# Vue State Observation

Krometrail observes Vue applications by installing a `__VUE_DEVTOOLS_GLOBAL_HOOK__` event emitter before Vue initializes. This captures component lifecycles, store mutations, and reactivity events — for both Vue 2 and Vue 3.

## Enabling Vue Observation

```bash
# CLI
krometrail browser start http://localhost:3000 --framework-state vue

# Or enable all frameworks
krometrail browser start http://localhost:3000 --framework-state
```

```json
// MCP: chrome_start
{ "url": "http://localhost:3000", "framework_state": ["vue"] }
```

::: warning Timing requirement
The hook must be installed before `createApp()` (Vue 3) or `new Vue()` (Vue 2) runs. Start the recording session first, then navigate to the URL to ensure correct injection timing.
:::

## What Gets Tracked

**Component lifecycles** — mount, update, and unmount events for every user component:
- Component name
- Component path in the tree
- Change type: `mount`, `update`, or `unmount`

**State and prop diffs** — for update events, the before/after values for changed `setupState` keys (Composition API), `$data` keys (Options API), and props.

**Render trigger source** — whether the update was triggered by state, props, or a parent re-render.

**Pinia store mutations** — when a Pinia store's state changes, the mutation type (`"direct"`, `"patch object"`, `"patch function"`), store ID, and state snapshot are recorded.

**Vuex mutations** — mutation type path (e.g., `"cart/SET_ITEMS"`), payload, and the resulting state.

**Store actions** — action calls including arguments, return values, and errors.

## Composition API vs Options API

Krometrail extracts state from both APIs:

| Source | Vue 3 | Vue 2 |
|--------|-------|-------|
| Reactive data | `instance.setupState` (Composition) | `vm.$data` |
| Props | `instance.props` | `vm.$props` |
| Computed | `instance.proxy[key]` | `vm._computedWatchers` |
| Store | Pinia via `app._context.provides` | Vuex via `vm.$store` |

## Searching Vue Events

```bash
# Find all Vue component errors
krometrail browser search <session-id> --event-types framework_error --framework vue

# Find all store mutations
krometrail browser search <session-id> --event-types framework_state "pinia"

# Find component updates with state changes
krometrail browser search <session-id> --framework vue --event-types framework_state
```

## Vue Version Support

- **Vue 3** — full support via `__VUE_DEVTOOLS_GLOBAL_HOOK__` event emitter
- **Vue 2** — supported via the same hook mechanism; component access uses `$children` tree traversal and `_computedWatchers`
- Works alongside Vue Devtools extension (listeners are added, not replaced)

::: tip Vue 3 3-second timeout
Vue 3 stops emitting devtools events if no consumer connects within 3 seconds of the first `app:init` event. Krometrail's injection script registers listeners synchronously at injection time, well before this cutoff.
:::
