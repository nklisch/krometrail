# Design: Phase 16 — Vue State Observer

## Overview

Full Vue 3 component state observation — component lifecycle tracking, state change diffing, Pinia/Vuex store integration, and bug pattern detection. The agent sees Vue-specific events in the timeline: component mounts/updates/unmounts, state diffs, store mutations, and detected anti-patterns.

**Scope:** Vue 3 only (Composition API + Options API). Vue 2 excluded per decision — can be added later.

**Depends on:** Phase 14 (FrameworkTracker, detection script, event types) and Phase 15 (React observer — establishes the file structure and injection patterns we follow).

---

## Implementation Units

### Unit 1: VueObserver Config & Class

**File**: `src/browser/recorder/framework/vue-observer.ts`

```typescript
import { buildVueInjectionScript } from "./vue-injection.js";

export interface VueObserverConfig {
	/** Max framework events per second reported via __BL__. Default: 10. */
	maxEventsPerSecond?: number;
	/** Max depth for state/props serialization. Default: 3. */
	maxSerializationDepth?: number;
	/** Component updates in 2s window before infinite loop warning. Default: 30. */
	infiniteLoopThreshold?: number;
	/** Max components visited per event batch (safety cap). Default: 5000. */
	maxComponentsPerBatch?: number;
	/** Max queued events before overflow (oldest dropped). Default: 1000. */
	maxQueueSize?: number;
	/** Enable Pinia/Vuex store observation. Default: true. */
	storeObservation?: boolean;
	/** Interval in ms for lazy store discovery polling. Default: 5000. */
	storeDiscoveryIntervalMs?: number;
}

/**
 * Manages the Vue 3 state observation injection script.
 * Instantiated by FrameworkTracker when "vue" is in the enabled frameworks.
 */
export class VueObserver {
	private config: Required<VueObserverConfig>;

	constructor(config: VueObserverConfig = {}) {
		this.config = {
			maxEventsPerSecond: config.maxEventsPerSecond ?? 10,
			maxSerializationDepth: config.maxSerializationDepth ?? 3,
			infiniteLoopThreshold: config.infiniteLoopThreshold ?? 30,
			maxComponentsPerBatch: config.maxComponentsPerBatch ?? 5000,
			maxQueueSize: config.maxQueueSize ?? 1000,
			storeObservation: config.storeObservation ?? true,
			storeDiscoveryIntervalMs: config.storeDiscoveryIntervalMs ?? 5000,
		};
	}

	/**
	 * Returns the injection script IIFE string.
	 * This script hooks into __VUE_DEVTOOLS_GLOBAL_HOOK__ (installed by detector.ts)
	 * to observe component lifecycle events and report state changes via __BL__.
	 */
	getInjectionScript(): string {
		return buildVueInjectionScript(this.config);
	}
}
```

**Implementation Notes**:
- Mirrors `ReactObserver` exactly — thin config wrapper that delegates to the injection script builder.
- `infiniteLoopThreshold` uses 30 (2s window) per the Vue SPEC, vs React's 15 (1s window). Vue's reactivity system can trigger more updates per cycle than React's batched commits.
- `storeObservation` and `storeDiscoveryIntervalMs` are Vue-specific — no React equivalent.

**Acceptance Criteria**:
- [ ] `new VueObserver()` with no args uses all defaults
- [ ] `new VueObserver({ maxEventsPerSecond: 20 })` merges partial config with defaults
- [ ] `getInjectionScript()` returns a non-empty string

---

### Unit 2: Vue Injection Script Builder

**File**: `src/browser/recorder/framework/vue-injection.ts`

```typescript
import { getVuePatternCode } from "./patterns/vue-patterns.js";
import type { VueObserverConfig } from "./vue-observer.js";

/**
 * Generate the Vue 3 observer injection script.
 * Returns a self-contained IIFE that hooks into __VUE_DEVTOOLS_GLOBAL_HOOK__
 * to observe component lifecycle and report via __BL__.
 *
 * Uses only `var` declarations — no let/const — for maximum browser compatibility.
 * All state is closure-local. Only side effect is patching the global hook.
 */
export function buildVueInjectionScript(config: Required<VueObserverConfig>): string;
```

The generated IIFE has 10 sections, mirroring the React injection script structure:

**Section 1: Configuration Constants**
```javascript
var MAX_EVENTS_PER_SECOND = ${config.maxEventsPerSecond};
var MAX_DEPTH = ${config.maxSerializationDepth};
var MAX_COMPONENTS_PER_BATCH = ${config.maxComponentsPerBatch};
var MAX_QUEUE_SIZE = ${config.maxQueueSize};
var INFINITE_LOOP_WINDOW_MS = 2000;
var STORE_OBSERVATION = ${config.storeObservation};
var STORE_DISCOVERY_INTERVAL_MS = ${config.storeDiscoveryIntervalMs};
```

**Section 2: Tracking State**
```javascript
var componentTracking = new Map();   // uid (number) -> tracking record
var eventQueue = [];
var lastFlushTime = 0;
var rafScheduled = false;
var apps = [];                       // tracked Vue 3 app instances
var storeUnsubscribers = [];         // cleanup functions for store subscriptions
var knownStoreIds = {};              // set of already-subscribed store IDs
var storeDiscoveryTimer = null;      // setInterval ID for lazy store polling
```

Note: Uses `Map` keyed by `instance.uid` (number) instead of `WeakMap`. Vue's devtools hook passes instances that may not maintain stable identity across the hook boundary. The `uid` is a stable monotonically-increasing integer. Entries are explicitly deleted on `component:removed`.

**Section 3: Reporting Helpers**

Identical to React — `blReport(type, data)`, `queueEvent(type, data)` with coalescing, `flushEvents()` with RAF batching. The coalescing key is `componentName` (same as React).

**Section 4: Serialization**

Identical `serialize(value, depth)` function from the React injection. Shared logic, duplicated in the IIFE for self-containment.

**Section 5: Vue Component Utilities**

```javascript
function getComponentName(instance) {
  // Vue 3: type.__name (SFC compiler) || type.name (explicit) || "Anonymous"
  var type = instance.type;
  if (!type) return 'Anonymous';
  return type.__name || type.name || 'Anonymous';
}

function getComponentPath(instance) {
  // Walk instance.parent chain, collect names, cap at 10 segments
  var parts = [];
  var current = instance;
  while (current) {
    var name = getComponentName(current);
    if (name !== 'Anonymous') parts.unshift(name);
    current = current.parent;
    if (parts.length > 10) break;
  }
  return parts.join(' > ');
}
```

**Section 6: State Extraction**

```javascript
function extractState(instance) {
  // Returns a shallow snapshot of component state for diffing.
  var state = {};

  // Composition API: setupState (auto-unwrapped refs via proxyRefs)
  try {
    var setup = instance.setupState;
    if (setup) {
      var skeys = Object.keys(setup);
      for (var i = 0; i < skeys.length; i++) {
        var k = skeys[i];
        if (k[0] === '$' || k[0] === '_') continue;  // skip internal
        if (typeof setup[k] === 'function') continue;  // skip methods
        state['setup.' + k] = setup[k];  // already unwrapped by proxyRefs
      }
    }
  } catch(e) {}

  // Options API: data()
  try {
    var data = instance.data;
    if (data && Object.keys(data).length > 0) {
      var dkeys = Object.keys(data);
      for (var j = 0; j < dkeys.length; j++) {
        state['data.' + dkeys[j]] = data[dkeys[j]];
      }
    }
  } catch(e) {}

  // Props
  try {
    var props = instance.props;
    if (props) {
      var pkeys = Object.keys(props);
      for (var p = 0; p < pkeys.length; p++) {
        state['props.' + pkeys[p]] = props[pkeys[p]];
      }
    }
  } catch(e) {}

  return state;
}

function diffState(prev, next) {
  // Returns array of { key, prev, next } or null if no changes.
  if (!prev) return null;
  var changes = [];
  var allKeys = {};
  var k;
  for (k in prev) allKeys[k] = true;
  for (k in next) allKeys[k] = true;
  for (k in allKeys) {
    var p = prev[k], n = next[k];
    if (p !== n) {
      changes.push({ key: k, prev: serialize(p), next: serialize(n) });
    }
  }
  return changes.length > 0 ? changes : null;
}
```

**Section 7: Trigger Source Detection**

```javascript
function detectTriggerSource(instance, prevState, nextState) {
  // Determine what caused the re-render
  var propsChanged = false;
  var stateChanged = false;
  for (var k in nextState) {
    if (k.indexOf('props.') === 0) {
      if (!prevState || prevState[k] !== nextState[k]) propsChanged = true;
    } else {
      if (!prevState || prevState[k] !== nextState[k]) stateChanged = true;
    }
  }
  if (stateChanged && !propsChanged) return 'state';
  if (propsChanged && !stateChanged) return 'props';
  if (propsChanged && stateChanged) return 'state';
  return 'parent';
}
```

**Section 8: Pattern Detection**

Injected from `getVuePatternCode(config)` — see Unit 4.

**Section 9: Event Handlers**

The core observation logic, hooking into the devtools hook's event emitter:

```javascript
function handleComponentAdded(instance, app) {
  // New mount
  try {
    var uid = instance.uid;
    if (uid === undefined) return;
    var state = extractState(instance);
    var record = {
      uid: uid,
      updateCount: 0,
      updateTimestamps: [],
      lastState: state,
      path: null,
      dirty: false
    };
    componentTracking.set(uid, record);
    record.updateCount++;

    queueEvent('state', {
      framework: 'vue',
      componentName: getComponentName(instance),
      componentPath: getComponentPath(instance),
      changeType: 'mount',
      renderCount: 1
    });
  } catch(e) {}
}

function handleComponentUpdated(instance, app) {
  try {
    var uid = instance.uid;
    if (uid === undefined) return;
    var record = componentTracking.get(uid);
    if (!record) {
      // Late discovery — treat as mount
      handleComponentAdded(instance, app);
      return;
    }
    record.updateCount++;
    var nowTs = Date.now();
    record.updateTimestamps.push(nowTs);

    // Trim timestamps older than 2s
    var cutoff = nowTs - 2000;
    var trimmed = [];
    for (var ti = 0; ti < record.updateTimestamps.length; ti++) {
      if (record.updateTimestamps[ti] > cutoff) trimmed.push(record.updateTimestamps[ti]);
    }
    record.updateTimestamps = trimmed;

    var state = extractState(instance);
    var changes = diffState(record.lastState, state);
    var componentName = getComponentName(instance);

    if (changes) {
      var triggerSource = detectTriggerSource(instance, record.lastState, state);
      queueEvent('state', {
        framework: 'vue',
        componentName: componentName,
        componentPath: record.path || (record.path = getComponentPath(instance)),
        changeType: 'update',
        changes: changes,
        renderCount: record.updateCount,
        triggerSource: triggerSource
      });
    }

    // Run pattern checks
    checkPatterns(instance, record, componentName);

    record.lastState = state;
  } catch(e) {}
}

function handleComponentRemoved(instance, app) {
  try {
    var uid = instance.uid;
    if (uid === undefined) return;
    var record = componentTracking.get(uid);
    queueEvent('state', {
      framework: 'vue',
      componentName: getComponentName(instance),
      componentPath: record ? record.path : getComponentPath(instance),
      changeType: 'unmount',
      renderCount: record ? record.updateCount : 0
    });
    componentTracking.delete(uid);
  } catch(e) {}
}

function handleAppInit(app, version) {
  apps.push(app);
  // Detect and observe stores
  if (STORE_OBSERVATION) {
    try { detectAndObserveStores(app); } catch(e) {}
    // Start lazy discovery polling for late-created stores
    if (!storeDiscoveryTimer) {
      storeDiscoveryTimer = setInterval(function() {
        for (var ai = 0; ai < apps.length; ai++) {
          try { detectAndObserveStores(apps[ai]); } catch(e) {}
        }
      }, STORE_DISCOVERY_INTERVAL_MS);
    }
  }
}

function handleAppUnmount(app) {
  // Remove from tracked apps
  for (var i = apps.length - 1; i >= 0; i--) {
    if (apps[i] === app) { apps.splice(i, 1); break; }
  }
  if (apps.length === 0 && storeDiscoveryTimer) {
    clearInterval(storeDiscoveryTimer);
    storeDiscoveryTimer = null;
  }
}
```

**Section 10: Hook Patching & Store Observation**

```javascript
// --- Hook patching ---
var hook = window.__VUE_DEVTOOLS_GLOBAL_HOOK__;
if (!hook) return;

// Register listeners on the hook's event emitter
hook.on('component:added', handleComponentAdded);
hook.on('component:updated', handleComponentUpdated);
hook.on('component:removed', handleComponentRemoved);
hook.on('app:init', handleAppInit);
hook.on('app:unmount', handleAppUnmount);

// Drain buffer — process events emitted before our listeners registered
var buffered = hook._buffer || [];
for (var bi = 0; bi < buffered.length; bi++) {
  var entry = buffered[bi];
  var event = entry[0];
  if (event === 'app:init') {
    try { handleAppInit(entry[1], entry[2]); } catch(e) {}
  } else if (event === 'component:added') {
    try { handleComponentAdded(entry[1], entry[2]); } catch(e) {}
  }
}

// --- Store observation ---
function detectAndObserveStores(app) {
  if (!app || !app._context) return;
  var provides = app._context.provides;
  if (!provides) return;

  // Pinia detection: look for object with _s Map
  var pinia = null;
  var provideKeys = Object.keys(provides);
  for (var pi = 0; pi < provideKeys.length; pi++) {
    var val = provides[provideKeys[pi]];
    if (val && typeof val === 'object' && val._s && val._s instanceof Map) {
      pinia = val;
      break;
    }
  }

  // Also check symbols (Pinia uses Symbol as provide key)
  if (!pinia) {
    try {
      var syms = Object.getOwnPropertySymbols(provides);
      for (var si = 0; si < syms.length; si++) {
        var sval = provides[syms[si]];
        if (sval && typeof sval === 'object' && sval._s && sval._s instanceof Map) {
          pinia = sval;
          break;
        }
      }
    } catch(e) {}
  }

  if (pinia) {
    pinia._s.forEach(function(store, id) {
      if (knownStoreIds[id]) return;
      knownStoreIds[id] = true;
      observePiniaStore(id, store);
    });
  }

  // Vuex detection (Vue 3 + Vuex 4)
  try {
    var vuexStore = app.config && app.config.globalProperties && app.config.globalProperties.$store;
    if (vuexStore && !knownStoreIds['__vuex__']) {
      knownStoreIds['__vuex__'] = true;
      observeVuexStore(vuexStore);
    }
  } catch(e) {}
}

function observePiniaStore(id, store) {
  try {
    var actionInFlight = null;

    // Subscribe to state mutations
    var unsub1 = store.$subscribe(function(mutation, state) {
      queueEvent('state', {
        framework: 'vue',
        componentName: '[Store] ' + id,
        changeType: 'store_mutation',
        storeId: id,
        mutationType: mutation.type,
        changes: serializeStoreState(id, state)
      });

      // Pattern: mutation outside action
      if (mutation.type === 'direct' && !actionInFlight) {
        try { checkPiniaMutationOutsideAction(id); } catch(e) {}
      }
    }, { detached: true });
    storeUnsubscribers.push(unsub1);

    // Subscribe to actions
    var unsub2 = store.$onAction(function(context) {
      actionInFlight = context.name;
      context.after(function() {
        queueEvent('state', {
          framework: 'vue',
          componentName: '[Store] ' + context.store.$id,
          changeType: 'store_mutation',
          storeId: context.store.$id,
          actionName: context.name
        });
        actionInFlight = null;
      });
      context.onError(function() {
        actionInFlight = null;
      });
    }, true);
    storeUnsubscribers.push(unsub2);
  } catch(e) {}
}

function observeVuexStore(store) {
  try {
    var unsub1 = store.subscribe(function(mutation, state) {
      var modulePath = mutation.type.split('/');
      var moduleId = modulePath.length > 1 ? modulePath.slice(0, -1).join('/') : 'root';
      queueEvent('state', {
        framework: 'vue',
        componentName: '[Store] vuex',
        changeType: 'store_mutation',
        storeId: moduleId,
        mutationType: mutation.type
      });
    });
    storeUnsubscribers.push(unsub1);

    var unsub2 = store.subscribeAction({
      after: function(action, state) {
        var modulePath = action.type.split('/');
        var moduleId = modulePath.length > 1 ? modulePath.slice(0, -1).join('/') : 'root';
        queueEvent('state', {
          framework: 'vue',
          componentName: '[Store] vuex',
          changeType: 'store_mutation',
          storeId: moduleId,
          actionName: action.type
        });
      }
    });
    storeUnsubscribers.push(unsub2);
  } catch(e) {}
}

function serializeStoreState(storeId, state) {
  // Diff against previous store snapshot
  var prevKey = '__store_' + storeId;
  var prev = componentTracking.get(prevKey);
  if (!prev) {
    componentTracking.set(prevKey, { lastState: {} });
    prev = componentTracking.get(prevKey);
  }
  var next = {};
  try {
    var keys = Object.keys(state);
    for (var i = 0; i < keys.length; i++) {
      next[keys[i]] = state[keys[i]];
    }
  } catch(e) {}
  var changes = diffState(prev.lastState, next);
  prev.lastState = next;
  return changes;
}
```

**Implementation Notes**:
- The script follows the same `parts.push("...")` line-by-line construction pattern as `react-injection.ts`.
- All code uses `var` only — no `let`/`const`.
- Event handlers are registered via `hook.on(...)` immediately (synchronously) to avoid Vue 3's 3-second timeout cutoff.
- Buffer drain processes `app:init` and `component:added` events that were emitted before our listener registered.
- Store discovery uses `Object.getOwnPropertySymbols` because Pinia's provide key is a Symbol.
- Store subscriptions use `{ detached: true }` to survive component unmounts.
- `componentTracking` uses a `Map<number, ...>` keyed by `instance.uid`. Map (not WeakMap) because Vue's hook passes ephemeral references; explicit cleanup on `component:removed`.
- Store state is tracked as synthetic entries in `componentTracking` with string keys `__store_${id}`.

**Acceptance Criteria**:
- [ ] `buildVueInjectionScript(config)` returns a non-empty IIFE string
- [ ] Script uses only `var` declarations (no `let`/`const`)
- [ ] Script contains `__BL__` and `console.debug` for reporting
- [ ] Script contains `component:added`, `component:updated`, `component:removed` listener registration
- [ ] Script contains `app:init` and `app:unmount` handlers
- [ ] Script contains `extractState` with setupState + data + props extraction
- [ ] Script contains `diffState` for state comparison
- [ ] Script contains store detection for Pinia (including Symbol provides) and Vuex
- [ ] Script contains buffer drain logic
- [ ] Script passes `new Function(script)` without syntax errors
- [ ] Config values are interpolated into constants

---

### Unit 3: FrameworkTracker Integration

**File**: `src/browser/recorder/framework/index.ts`

```typescript
import { VueObserver } from "./vue-observer.js";

// In FrameworkTracker class:
export class FrameworkTracker {
	private reactObserver: ReactObserver | null = null;
	private vueObserver: VueObserver | null = null;  // NEW

	getInjectionScripts(): string[] {
		// ... existing code ...

		// Phase 16: Vue observer
		if (this.config.frameworks.includes("vue")) {
			this.vueObserver = new VueObserver();
			scripts.push(this.vueObserver.getInjectionScript());
		}

		return scripts;
	}
}
```

**Implementation Notes**:
- Replace the `// Phase 16+: Vue, Solid, Svelte observers will be added here` comment at line 52 with the Vue observer instantiation.
- Vue injection script is added *after* the detection script (index 0) — same ordering as React.
- No changes needed to `processFrameworkEvent` or `buildSummary` — they already handle `framework: "vue"` generically.

**Acceptance Criteria**:
- [ ] `FrameworkTracker` instantiates `VueObserver` when "vue" is in the config
- [ ] `getInjectionScripts()` includes the Vue injection script after the detection script
- [ ] `processFrameworkEvent` correctly parses Vue `framework_state` events with `storeId`, `mutationType`, `actionName` fields

---

### Unit 4: Vue Bug Pattern Detectors

**File**: `src/browser/recorder/framework/patterns/vue-patterns.ts`

```typescript
import type { VueObserverConfig } from "../vue-observer.js";

/** Threshold constants — exported for unit testing. */
export const VUE_PATTERN_DEFAULTS = {
	infiniteLoopThreshold: 30,
	infiniteLoopWindowMs: 2000,
} as const;

/**
 * Returns the JavaScript code string for all Vue pattern detection functions.
 * Injected into the observer IIFE. All functions use `var` only.
 *
 * Generated functions:
 * - checkPatterns(instance, record, componentName)
 * - checkInfiniteLoop(instance, record, componentName)
 * - checkLostReactivity(instance, record, componentName)
 * - checkPiniaMutationOutsideAction(storeId)
 */
export function getVuePatternCode(config: Required<VueObserverConfig>): string;
```

**Pattern 1: `watcher_infinite_loop`** (severity: high)

Detects a component that re-renders >30 times in 2 seconds:

```javascript
function checkInfiniteLoop(instance, record, componentName) {
  var _now = Date.now();
  var _windowMs = 2000;
  var _threshold = ${config.infiniteLoopThreshold};
  var _recent = [];
  for (var _ri = 0; _ri < record.updateTimestamps.length; _ri++) {
    if (_now - record.updateTimestamps[_ri] < _windowMs) _recent.push(record.updateTimestamps[_ri]);
  }
  if (_recent.length > _threshold) {
    queueEvent('error', {
      framework: 'vue',
      pattern: 'watcher_infinite_loop',
      componentName: componentName,
      severity: 'high',
      detail: componentName + ' updated ' + _recent.length + ' times in ' + _windowMs + 'ms. Likely a watcher mutating its own dependency.',
      evidence: {
        updateCount: _recent.length,
        windowMs: _windowMs,
        lastKeys: record.lastState ? Object.keys(record.lastState) : []
      }
    });
  }
}
```

**Pattern 2: `lost_reactivity`** (severity: medium)

Detects non-reactive objects in `setupState` where a reactive Proxy or Ref is expected:

```javascript
function checkLostReactivity(instance, record, componentName) {
  try {
    var setup = instance.setupState;
    if (!setup) return;
    var keys = Object.keys(setup);
    for (var _lri = 0; _lri < keys.length; _lri++) {
      var _lrk = keys[_lri];
      if (_lrk[0] === '$' || _lrk[0] === '_') continue;
      var val = setup[_lrk];
      if (val === null || val === undefined) continue;
      if (typeof val !== 'object') continue;
      if (typeof val === 'function') continue;
      if (Array.isArray(val)) continue;

      // Check if value is reactive (Vue 3 Proxy-wrapped)
      var isReactive = val.__v_isReactive === true;
      var isRef = val.__v_isRef === true;
      var isReadonly = val.__v_isReadonly === true;
      var isShallow = val.__v_isShallow === true;

      // A plain object in setupState that is not reactive/ref/readonly is suspicious
      if (!isReactive && !isRef && !isReadonly && !isShallow) {
        queueEvent('error', {
          framework: 'vue',
          pattern: 'lost_reactivity',
          componentName: componentName,
          severity: 'medium',
          detail: '"' + _lrk + '" in ' + componentName + ' setupState is a plain object (not reactive). This often happens when destructuring a reactive() object or unwrapping a ref without .value.',
          evidence: {
            key: _lrk,
            actualType: typeof val,
            hasProxy: false
          }
        });
      }
    }
  } catch(e) {}
}
```

**Pattern 3: `pinia_mutation_outside_action`** (severity: low)

Detects Pinia store state mutation without an active action (called from store observation in the injection script):

```javascript
function checkPiniaMutationOutsideAction(storeId) {
  queueEvent('error', {
    framework: 'vue',
    pattern: 'pinia_mutation_outside_action',
    componentName: '[Store] ' + storeId,
    severity: 'low',
    detail: 'Pinia store "' + storeId + '" state was directly mutated outside of an action. This bypasses devtools tracking and time-travel debugging.',
    evidence: {
      storeId: storeId,
      mutationType: 'direct'
    }
  });
}
```

**Pattern dispatcher:**

```javascript
function checkPatterns(instance, record, componentName) {
  try { checkInfiniteLoop(instance, record, componentName); } catch(e) {}
  try { checkLostReactivity(instance, record, componentName); } catch(e) {}
}
```

Note: `checkPiniaMutationOutsideAction` is not called from `checkPatterns` — it is called inline from `observePiniaStore` when a direct mutation is detected without an active action.

**Implementation Notes**:
- Follows the exact same code-generation pattern as `react-patterns.ts`: exports `getVuePatternCode(config)` returning a raw JS string.
- All functions use `var` only, prefixed variable names to avoid collisions.
- Each detector wrapped in try/catch by the dispatcher.
- `lost_reactivity` only checks setupState (Composition API). Options API `data()` is always reactive by construction — Vue wraps it in `reactive()` internally.
- `computed_deps_not_tracked` (from SPEC.md) is **deferred** — requires `onTrack`/`onTrigger` hooks which can only be attached at creation time. Would require invasive patching of `computed()` calls. Not worth the complexity for Phase 16.

**Acceptance Criteria**:
- [ ] `getVuePatternCode(config)` returns a valid JS code string
- [ ] Generated code uses only `var` declarations
- [ ] Contains `checkInfiniteLoop` with configurable threshold
- [ ] Contains `checkLostReactivity` checking `__v_isReactive`/`__v_isRef`/`__v_isReadonly`
- [ ] Contains `checkPiniaMutationOutsideAction`
- [ ] Contains `checkPatterns` dispatcher with try/catch isolation

---

### Unit 5: Fixture App — Vue 3 Counter (Composition API)

**File**: `tests/fixtures/browser/vue3-counter/index.html`

```html
<!DOCTYPE html>
<html>
<head><title>Vue 3 Counter</title></head>
<body>
  <div id="app"></div>
  <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
  <script>
    const { createApp, ref, computed } = Vue;

    const Counter = {
      name: 'Counter',
      setup() {
        const count = ref(0);
        const doubled = computed(() => count.value * 2);
        const increment = () => count.value++;
        return { count, doubled, increment };
      },
      template: `
        <div>
          <p id="count">Count: {{ count }}</p>
          <p id="doubled">Doubled: {{ doubled }}</p>
          <button id="increment" @click="increment">+1</button>
        </div>
      `
    };

    const App = {
      name: 'App',
      components: { Counter },
      template: '<div><h1>Vue 3 Counter</h1><Counter /></div>'
    };

    createApp(App).mount('#app');
  </script>
</body>
</html>
```

**File**: `tests/fixtures/browser/vue3-counter/server.ts`

```typescript
// Minimal Bun HTTP server serving the fixture HTML
const file = Bun.file(new URL("index.html", import.meta.url).pathname);

export function startServer(port = 0): { server: ReturnType<typeof Bun.serve>; url: string } {
	const server = Bun.serve({
		port,
		fetch() {
			return new Response(file, { headers: { "Content-Type": "text/html" } });
		},
	});
	return { server, url: `http://localhost:${server.port}` };
}
```

**Acceptance Criteria**:
- [ ] HTML loads Vue 3 from CDN and mounts a counter app
- [ ] Counter has named components (`App`, `Counter`)
- [ ] Uses Composition API (`ref`, `computed`, `setup()`)
- [ ] Has clickable button with `id="increment"` for E2E interaction
- [ ] `server.ts` serves the HTML on a random port

---

### Unit 6: Fixture App — Vue 3 Pinia Store

**File**: `tests/fixtures/browser/vue3-pinia/index.html`

```html
<!DOCTYPE html>
<html>
<head><title>Vue 3 + Pinia</title></head>
<body>
  <div id="app"></div>
  <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
  <script src="https://unpkg.com/pinia@2/dist/pinia.iife.js"></script>
  <script>
    const { createApp, computed } = Vue;
    const { createPinia, defineStore } = Pinia;

    const useCounterStore = defineStore('counter', {
      state: () => ({ count: 0, name: 'Counter Store' }),
      getters: {
        doubleCount: (state) => state.count * 2,
      },
      actions: {
        increment() { this.count++; },
      },
    });

    const StoreDisplay = {
      name: 'StoreDisplay',
      setup() {
        const store = useCounterStore();
        return { store };
      },
      template: `
        <div>
          <p id="store-count">Store count: {{ store.count }}</p>
          <p id="store-double">Double: {{ store.doubleCount }}</p>
          <button id="store-increment" @click="store.increment()">Action +1</button>
          <button id="store-direct" @click="store.count++">Direct +1</button>
        </div>
      `,
    };

    const app = createApp({
      name: 'App',
      components: { StoreDisplay },
      template: '<div><h1>Pinia Store Test</h1><StoreDisplay /></div>',
    });
    app.use(createPinia());
    app.mount('#app');
  </script>
</body>
</html>
```

**File**: `tests/fixtures/browser/vue3-pinia/server.ts`

Same pattern as vue3-counter.

**Acceptance Criteria**:
- [ ] Loads Vue 3 + Pinia from CDN
- [ ] Defines a Pinia store with state, getters, and actions
- [ ] Has button for action-based mutation (`id="store-increment"`)
- [ ] Has button for direct mutation (`id="store-direct"`) — triggers `pinia_mutation_outside_action` pattern

---

### Unit 7: Fixture App — Vue 3 Bugs

**File**: `tests/fixtures/browser/vue-bugs/index.html`

```html
<!DOCTYPE html>
<html>
<head><title>Vue 3 Bugs</title></head>
<body>
  <div id="app"></div>
  <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
  <script>
    const { createApp, ref, reactive, watch } = Vue;

    // Bug 1: Infinite watcher loop
    const InfiniteWatcher = {
      name: 'InfiniteWatcher',
      setup() {
        const count = ref(0);
        const trigger = ref(false);

        watch(count, (val) => {
          // Mutates its own dependency — infinite loop
          if (val < 100) count.value = val + 1;
        });

        const start = () => { trigger.value = !trigger.value; count.value = 1; };
        return { count, start };
      },
      template: '<div><p id="inf-count">{{ count }}</p><button id="inf-start" @click="start">Start Loop</button></div>'
    };

    // Bug 2: Lost reactivity (destructured reactive)
    const LostReactivity = {
      name: 'LostReactivity',
      setup() {
        const state = reactive({ x: 1, y: 2 });
        // Bug: destructuring loses reactivity
        const { x, y } = state;
        const plainObj = { nested: { value: 42 } };  // plain object in setupState
        return { x, y, plainObj, state };
      },
      template: '<div><p id="lr-x">x={{ x }}</p><p id="lr-y">y={{ y }}</p></div>'
    };

    const App = {
      name: 'App',
      components: { InfiniteWatcher, LostReactivity },
      template: '<div><InfiniteWatcher /><LostReactivity /></div>'
    };

    createApp(App).mount('#app');
  </script>
</body>
</html>
```

**File**: `tests/fixtures/browser/vue-bugs/server.ts`

Same pattern.

**Acceptance Criteria**:
- [ ] `InfiniteWatcher` triggers >30 updates in 2s when button clicked
- [ ] `LostReactivity` has `plainObj` (non-reactive object) in setupState
- [ ] Named components for pattern detection

---

## Implementation Order

1. **Unit 4: Vue pattern detectors** (`vue-patterns.ts`) — Pure code generation, no dependencies on other new code. Can be unit tested immediately.

2. **Unit 1: VueObserver config class** (`vue-observer.ts`) — Thin wrapper, depends on Unit 3 for the actual script but can be stubbed.

3. **Unit 2: Vue injection script builder** (`vue-injection.ts`) — The largest unit. Depends on Unit 4 for pattern code. Core implementation work.

4. **Unit 3: FrameworkTracker integration** (`framework/index.ts`) — Small edit: add import + instantiation. Depends on Units 1 and 2.

5. **Unit 5: Vue 3 counter fixture** — Independent, but needed for E2E testing of Units 1-3.

6. **Unit 6: Vue 3 Pinia fixture** — Independent, needed for store observation E2E tests.

7. **Unit 7: Vue 3 bugs fixture** — Independent, needed for pattern detection E2E tests.

Recommended implementation sequence: **4 → 1 → 2 → 3 → 5 → 6 → 7**, with unit tests after each unit and E2E tests after Unit 7.

---

## Testing

### Unit Tests: `tests/unit/browser/vue-observer.test.ts`

Mirrors `react-observer.test.ts`:

```typescript
describe("VueObserver", () => {
  describe("constructor", () => {
    it("uses default config when no args provided");
    it("merges partial config with defaults");
    it("respects all config overrides");
  });

  describe("getInjectionScript", () => {
    it("returns a non-empty string");
    it("is a self-contained IIFE");
    it("uses only var declarations (no let/const)");
    it("contains __BL__ reporting");
    it("contains component:added listener registration");
    it("contains component:updated listener registration");
    it("contains component:removed listener registration");
    it("contains app:init handler");
    it("contains buffer drain logic");
    it("interpolates config values into the script");
    it("contains extractState function");
    it("contains diffState function");
    it("contains getComponentName function");
    it("contains getComponentPath function");
    it("contains pattern detection functions");
    it("contains store observation (Pinia detection)");
    it("contains store observation (Vuex detection)");
    it("has no syntax errors (new Function parse check)");
  });
});
```

### Unit Tests: `tests/unit/browser/vue-injection.test.ts`

Mirrors `react-injection.test.ts`:

```typescript
describe("buildVueInjectionScript", () => {
  it("returns a non-empty string");
  it("is a self-contained IIFE");
  it("uses only var declarations (no let/const)");
  it("contains __BL__ reporting");
  it("contains component lifecycle listener registration");
  it("wraps hook.on calls for component events");
  it("handles missing hook gracefully (early return)");
  it("interpolates maxEventsPerSecond config");
  it("interpolates maxSerializationDepth config");
  it("interpolates pattern thresholds from config");
  it("interpolates maxComponentsPerBatch");
  it("interpolates maxQueueSize");
  it("generated script has no syntax errors (new Function parse check)");
  it("contains extractState function");
  it("contains diffState function");
  it("contains serialize function");
  it("contains getComponentName function");
  it("contains getComponentPath function");
  it("contains pattern detection functions");
  it("contains coalescing in queueEvent");
  it("contains MAX_QUEUE_SIZE overflow protection");
  it("contains Pinia store detection");
  it("contains Vuex store detection");
  it("contains buffer drain for _buffer");
  it("contains store discovery interval setup");
});
```

### Unit Tests: `tests/unit/browser/vue-patterns.test.ts`

Mirrors `react-patterns.test.ts`:

```typescript
describe("getVuePatternCode", () => {
  it("returns a non-empty string");
  it("uses only var declarations (no let/const)");
  it("contains checkPatterns dispatcher");
  it("contains checkInfiniteLoop with configurable threshold");
  it("contains checkLostReactivity");
  it("contains checkPiniaMutationOutsideAction");
  it("has no syntax errors when embedded in a function body");
  it("interpolates infiniteLoopThreshold from config");
});

describe("VUE_PATTERN_DEFAULTS", () => {
  it("has expected default values");
});
```

### Unit Tests: `tests/unit/browser/framework-tracker.test.ts` (extend existing)

Add tests for Vue observer integration:

```typescript
describe("FrameworkTracker with Vue", () => {
  it("includes Vue injection script when 'vue' is in config");
  it("includes both React and Vue scripts when both are in config");
  it("processes Vue framework_state events with store fields");
  it("builds correct summary for Vue framework_detect events");
  it("builds correct summary for Vue framework_state store_mutation events");
});
```

### E2E Tests: `tests/e2e/browser/vue-observer.test.ts`

Mirrors `react-observer.test.ts`:

```typescript
describe("Vue observer E2E", () => {
  describe("vue3-counter fixture", () => {
    it("detects Vue 3 framework");
    it("captures component mount events");
    it("captures state update events on button click");
    it("shows component path in events");
    it("includes state diff in update events");
  });

  describe("vue3-pinia fixture", () => {
    it("detects Pinia store");
    it("captures store mutation via action");
    it("captures direct store mutation");
    it("detects pinia_mutation_outside_action pattern on direct mutation");
  });

  describe("vue-bugs fixture", () => {
    it("detects watcher_infinite_loop when InfiniteWatcher triggered");
    it("detects lost_reactivity for plainObj in LostReactivity component");
  });
});
```

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Vue 3 only | Skip Vue 2 | Vue 2 is EOL (Dec 2023). Avoids version-branching complexity. Can be added later. |
| `Map` not `WeakMap` for tracking | `Map<uid, record>` with explicit delete | Vue's devtools hook may pass different object references for the same instance across events. `uid` is the stable identity. |
| RAF batching (not setTimeout debounce) | Follow React pattern | Consistency with React observer. RAF is naturally tied to rendering frames. |
| Deferred `computed_deps_not_tracked` | Out of scope | Requires invasive patching of `computed()` at creation time. Low ROI for Phase 16. |
| Same `maxEventsPerSecond` default (10) | Consistency | Both React and Vue share the same `__BL__` channel and budget. |
| Infinite loop: 30/2s (not 15/1s) | Vue-specific tuning | Vue's reactivity can trigger more granular updates than React's batched commits. Higher threshold avoids false positives. |
| Store discovery polling (5s interval) | Match ARCH.md | Pinia stores are lazily created. Polling catches stores created after app init (e.g., on route change). |
| Symbol-aware Pinia detection | Check `Object.getOwnPropertySymbols` | Pinia uses a Symbol key for its provide injection. `Object.keys()` misses it. |

## Verification Checklist

```bash
# Unit tests
bun run test:unit -- --grep "VueObserver"
bun run test:unit -- --grep "buildVueInjectionScript"
bun run test:unit -- --grep "getVuePatternCode"
bun run test:unit -- --grep "FrameworkTracker with Vue"

# E2E tests
bun run test:e2e -- --grep "Vue observer"

# Lint
bun run lint

# Full suite (regression)
bun run test
```
