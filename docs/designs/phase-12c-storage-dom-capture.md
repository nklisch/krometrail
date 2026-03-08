# Design: Phase 12c — Storage Change and DOM Mutation Capture

## Overview

Two `EventType` values exist in `src/browser/types.ts` (`storage_change`, `dom_mutation`) but are never emitted — the event normalizer has no handler for them, and the injection script doesn't report them.

This gap matters concretely:
- `SessionDiffer.diffStorage()` already queries `storage_change` events and expects `data.{ key, changeType, oldValue, newValue }` — but always returns empty because no such events exist.
- `dom_mutation` would allow the agent to see when significant UI elements appear or disappear between the `before` and `after` moments in a diff.

Both are implemented via the `__BL__` injection mechanism already used for click/submit/change/marker/CLS — the injected script calls `console.debug('__BL__', ...)`, the `InputTracker` processes it, and the `EventPipeline` routes it into the buffer.

**Not in scope**: CDP-based `DOMStorage` domain (it requires enabling a 6th CDP domain and has cross-frame limitations). Injected proxy is simpler and already wired.

---

## Implementation Units

### Unit 1: Capture storage_change events via injected script

**File**: `src/browser/recorder/input-tracker.ts`

Extend `getInjectionScript()` to proxy `localStorage` and `sessionStorage`:

```javascript
// Appended inside the IIFE, before closing `})();`:

// Storage change tracking
(function() {
  function proxyStorage(storage, storageName) {
    var origSetItem = storage.setItem.bind(storage);
    var origRemoveItem = storage.removeItem.bind(storage);
    var origClear = storage.clear.bind(storage);

    storage.setItem = function(key, value) {
      var oldValue;
      try { oldValue = storage.getItem(key); } catch(e) {}
      origSetItem(key, value);
      report('storage', {
        storageType: storageName,
        changeType: oldValue === null ? 'added' : 'set',
        key: key,
        oldValue: oldValue === null ? undefined : oldValue,
        newValue: String(value).slice(0, 500)
      });
    };

    storage.removeItem = function(key) {
      var oldValue;
      try { oldValue = storage.getItem(key); } catch(e) {}
      origRemoveItem(key);
      if (oldValue !== null) {
        report('storage', {
          storageType: storageName,
          changeType: 'removed',
          key: key,
          oldValue: String(oldValue).slice(0, 500)
        });
      }
    };

    storage.clear = function() {
      origClear();
      report('storage', { storageType: storageName, changeType: 'cleared' });
    };
  }

  try { proxyStorage(localStorage, 'local'); } catch(e) {}
  try { proxyStorage(sessionStorage, 'session'); } catch(e) {}

  // Also capture cross-tab storage events (other tabs mutating localStorage)
  window.addEventListener('storage', function(e) {
    if (e.storageArea === localStorage || e.storageArea === sessionStorage) {
      report('storage', {
        storageType: e.storageArea === localStorage ? 'local' : 'session',
        changeType: e.newValue === null ? 'removed' : (e.oldValue === null ? 'added' : 'set'),
        key: e.key || '',
        oldValue: e.oldValue ? String(e.oldValue).slice(0, 500) : undefined,
        newValue: e.newValue ? String(e.newValue).slice(0, 500) : undefined,
        crossTab: true
      });
    }
  });
})();
```

Extend `InputEventData`:

```typescript
interface InputEventData {
	type: "click" | "submit" | "change" | "marker" | "cls" | "storage" | "dom_mutation";
	ts: number;
	selector?: string;
	text?: string;
	tag?: string;
	action?: string;
	fields?: Record<string, string>;
	value?: string | number;
	label?: string;
	metric?: string;
	// Storage fields
	storageType?: "local" | "session";
	changeType?: "added" | "set" | "removed" | "cleared";
	key?: string;
	oldValue?: string;
	newValue?: string;
	crossTab?: boolean;
	// DOM mutation fields
	mutationType?: "added" | "removed" | "text_changed";
	elementSelector?: string;
	elementTag?: string;
	elementText?: string;
}
```

Handle `storage` in `processInputEvent()`:

```typescript
if (parsed.type === "storage") {
	const storageLabel = parsed.storageType === "local" ? "localStorage" : "sessionStorage";
	let summary: string;
	if (parsed.changeType === "cleared") {
		summary = `${storageLabel} cleared`;
	} else if (parsed.changeType === "removed") {
		summary = `${storageLabel}["${parsed.key}"] removed`;
	} else {
		summary = `${storageLabel}["${parsed.key}"] ${parsed.changeType}: ${(parsed.newValue ?? "").slice(0, 80)}`;
	}
	return this.buildEvent("storage_change", tabId, parsed.ts, summary, {
		storageType: parsed.storageType,
		changeType: parsed.changeType,
		key: parsed.key,
		oldValue: parsed.oldValue,
		newValue: parsed.newValue,
		crossTab: parsed.crossTab ?? false,
	});
}
```

**How diffStorage() uses this data**: `diff.ts:198` reads `full.data.changeType`, `full.data.oldValue`, `full.data.newValue`, `full.data.key` — exactly the fields emitted above. No changes needed to `SessionDiffer`.

**Implementation Notes**:
- `value.slice(0, 500)`: limits large JSON blobs. The diff renderer will truncate further per token budget.
- `null` vs `undefined`: `getItem()` returns `null` when key doesn't exist. We map `null → undefined` so the emitted data doesn't serialize `"oldValue": null` for new keys.
- `storage` window event fires in the current tab only when _another_ tab mutates the storage — not for same-tab mutations. The proxy covers same-tab; the `storage` listener covers cross-tab. Together they're complete.
- `clear()` only emits one event with `changeType: "cleared"`, not one per key. The diff should handle this as "all keys removed" — `diffStorage()` doesn't currently handle `"cleared"`, but it will show no changes (no per-key events). Acceptable for now — `clear()` is rare in real apps.
- `report` function is already defined at the top of the IIFE (used for input events). Storage tracking reuses it directly.

**Acceptance Criteria**:
- [ ] `localStorage.setItem(k, v)` emits a `storage_change` event with `changeType: "added"` (first set) or `"set"` (update), including `oldValue` and `newValue`
- [ ] `localStorage.removeItem(k)` emits `storage_change` with `changeType: "removed"` and `oldValue`
- [ ] `sessionStorage` mutations produce `storageType: "session"` events
- [ ] Cross-tab `storage` window events are captured
- [ ] `SessionDiffer.diffStorage()` returns populated changes when storage events exist
- [ ] Events are routed through the existing `__BL__` processing path in `EventPipeline`

---

### Unit 2: Capture dom_mutation events via injected MutationObserver

**File**: `src/browser/recorder/input-tracker.ts`

DOM mutations are extremely high volume (React re-renders fire thousands of mutations per second). The injection script must be aggressive about filtering and debouncing.

**Filter criteria for "meaningful" mutations**:
- Element added or removed from the DOM (not attribute changes, not text nodes within already-tracked elements)
- The element must be a block-level or interactive element: `div, section, article, main, nav, aside, header, footer, form, dialog, [role], button, input, select, textarea, a, h1-h6, p, ul, ol, table`
- The element must have at least one of: an `id`, a `data-testid`, a `role` attribute, or be a semantic HTML element (`form`, `dialog`, heading tags)
- Skip mutations inside `script`, `style`, `head`, `noscript`
- Debounce: batch all mutations within a 500ms window, emit one event summarizing the batch

```javascript
// Appended inside the IIFE, before closing `})();`:

(function() {
  var MEANINGFUL_TAGS = {
    FORM: 1, DIALOG: 1, SECTION: 1, ARTICLE: 1, MAIN: 1, NAV: 1,
    ASIDE: 1, HEADER: 1, FOOTER: 1, H1: 1, H2: 1, H3: 1, H4: 1,
    H5: 1, H6: 1, TABLE: 1
  };
  var SKIP_CONTAINERS = { SCRIPT: 1, STYLE: 1, HEAD: 1, NOSCRIPT: 1 };

  function isMeaningful(el) {
    if (!el || el.nodeType !== 1) return false;
    var tag = el.tagName;
    if (SKIP_CONTAINERS[tag]) return false;
    if (MEANINGFUL_TAGS[tag]) return true;
    if (el.id || el.getAttribute('data-testid') || el.getAttribute('role')) return true;
    return false;
  }

  function selFor(el) {
    if (el.id) return '#' + el.id;
    if (el.getAttribute('data-testid')) return '[data-testid="' + el.getAttribute('data-testid') + '"]';
    if (el.getAttribute('role')) return el.tagName.toLowerCase() + '[role="' + el.getAttribute('role') + '"]';
    return el.tagName.toLowerCase();
  }

  var pendingAdded = [];
  var pendingRemoved = [];
  var debounceTimer = null;

  function flush() {
    debounceTimer = null;
    var added = pendingAdded.splice(0);
    var removed = pendingRemoved.splice(0);
    if (added.length === 0 && removed.length === 0) return;
    report('dom_mutation', {
      added: added.slice(0, 10),    // cap at 10 per batch
      removed: removed.slice(0, 10)
    });
  }

  try {
    var observer = new MutationObserver(function(mutations) {
      for (var i = 0; i < mutations.length; i++) {
        var m = mutations[i];
        for (var j = 0; j < m.addedNodes.length; j++) {
          var n = m.addedNodes[j];
          if (isMeaningful(n)) {
            pendingAdded.push({
              selector: selFor(n),
              tag: n.tagName.toLowerCase(),
              text: (n.textContent || '').trim().slice(0, 100)
            });
          }
        }
        for (var k = 0; k < m.removedNodes.length; k++) {
          var r = m.removedNodes[k];
          if (isMeaningful(r)) {
            pendingRemoved.push({
              selector: selFor(r),
              tag: r.tagName.toLowerCase()
            });
          }
        }
      }
      if ((pendingAdded.length > 0 || pendingRemoved.length > 0) && !debounceTimer) {
        debounceTimer = setTimeout(flush, 500);
      }
    });

    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: false,
      characterData: false
    });
  } catch(e) {}
})();
```

Handle `dom_mutation` in `processInputEvent()`:

```typescript
if (parsed.type === "dom_mutation") {
	const added = (parsed.added as Array<{ selector: string; tag: string; text?: string }>) ?? [];
	const removed = (parsed.removed as Array<{ selector: string; tag: string }>) ?? [];
	const parts: string[] = [];
	if (added.length > 0) parts.push(`+${added.length} ${added.map((e) => e.selector).join(", ")}`);
	if (removed.length > 0) parts.push(`-${removed.length} ${removed.map((e) => e.selector).join(", ")}`);
	const summary = `DOM: ${parts.join("; ")}`.slice(0, 300);
	return this.buildEvent("dom_mutation", tabId, parsed.ts, summary, {
		added,
		removed,
	});
}
```

**Implementation Notes**:
- `attributes: false, characterData: false`: We only care about structural changes (nodes added/removed), not every React prop update.
- The 500ms debounce batches a rapid React render cycle (multiple synchronous mutations) into a single event.
- Capping `added`/`removed` at 10 each per batch prevents a single "app bootstrapped" event from flooding storage.
- `textContent.slice(0, 100)` for added nodes gives the agent enough context to understand "a modal appeared with text 'Are you sure?'".
- `childList + subtree: true` without `attributes` means we observe structural changes only — React re-renders that only update existing DOM attributes/text don't fire this observer.
- These events are low enough priority that they shouldn't be added to the auto-detect rules (no rule checks `dom_mutation` type).

**What the agent can do with this data**: Currently nothing in the investigation tools specifically surfaces `dom_mutation` events, but they will appear in:
- `session_search` results (can search for "dialog appeared" or filter by `event_types: ["dom_mutation"]`)
- `session_inspect` surrounding events context
- `session_overview` timeline (if navigation + dom_mutation events are included)

A future `session_diff` "dom" include option could reconstruct "what elements appeared/disappeared" between two moments by aggregating `dom_mutation` events in range.

**Acceptance Criteria**:
- [ ] Adding a `<dialog>` to the DOM emits a `dom_mutation` event with it in `data.added`
- [ ] Removing an element with `id` emits a `dom_mutation` event with it in `data.removed`
- [ ] Rapid mutations within 500ms are batched into a single event
- [ ] React re-renders that only change attributes (class, style) do not emit events
- [ ] Changes inside `<script>` and `<style>` tags are ignored
- [ ] `div` elements without `id`, `data-testid`, or `role` are ignored
- [ ] Events appear in `session_search` results when filtering `event_types: ["dom_mutation"]`

---

### Unit 3: Route storage and dom_mutation events through EventPipeline

**File**: `src/browser/recorder/event-pipeline.ts`

No changes required. The `__BL__` routing path (lines 49-67) calls `inputTracker.processInputEvent(args[1].value, tabId)` and then routes the result through `buffer.push(event)` → `persistence.onNewEvent()` → `checkAutoDetect()`. Since `processInputEvent` now handles `storage` and `dom_mutation` types, they will flow through automatically.

**Verification**: The `EventPipeline` doesn't filter by event type — it routes whatever `InputTracker` returns. No changes needed.

---

### Unit 4: Update event normalizer switch comment

**File**: `src/browser/recorder/event-normalizer.ts`

No code changes. The normalizer's `switch` will remain as-is since storage and DOM changes come through the `Runtime.consoleAPICalled` path (already handled) rather than as named CDP events. Add a comment to document this so future developers don't add redundant CDP handlers:

```typescript
// Note: storage_change and dom_mutation events are captured via the __BL__
// injection mechanism (InputTracker), not via CDP domain events. This is because:
// - DOMStorage domain has cross-frame limitations and requires an additional CDP domain
// - MutationObserver cannot be observed from CDP directly
// See src/browser/recorder/input-tracker.ts for the injected scripts.
```

Add this comment before the `default: return null;` case in the `normalize()` switch.

---

## Implementation Order

1. **Unit 1** (storage_change) — extends `InputEventData`, adds to injection script, adds `processInputEvent` branch. Feeds immediately into the existing `diffStorage()` which is already written.
2. **Unit 2** (dom_mutation) — same structure as Unit 1, independent.
3. **Unit 3** — just verification (no code changes needed).
4. **Unit 4** — one-line comment.

Units 1 and 2 are fully independent and can be implemented in parallel.

---

## Testing

### Unit Tests: `tests/unit/browser/input-tracker-storage.test.ts`

```typescript
describe("InputTracker storage events", () => {
	const tracker = new InputTracker();

	it("processes storage 'added' into storage_change event", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({ type: "storage", ts: Date.now(), storageType: "local", changeType: "added", key: "draft", newValue: '{"name":"Alice"}' }),
			"tab1"
		);
		expect(result?.type).toBe("storage_change");
		expect(result?.data.changeType).toBe("added");
		expect(result?.data.key).toBe("draft");
		expect(result?.data.newValue).toBe('{"name":"Alice"}');
	});

	it("processes storage 'removed' into storage_change event with oldValue", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({ type: "storage", ts: Date.now(), storageType: "session", changeType: "removed", key: "token", oldValue: "abc123" }),
			"tab1"
		);
		expect(result?.type).toBe("storage_change");
		expect(result?.data.changeType).toBe("removed");
		expect(result?.data.oldValue).toBe("abc123");
		expect(result?.data.storageType).toBe("session");
	});

	it("includes localStorage vs sessionStorage distinction", () => {
		const local = tracker.processInputEvent(
			JSON.stringify({ type: "storage", ts: Date.now(), storageType: "local", changeType: "set", key: "x", newValue: "1" }),
			"tab1"
		);
		const session = tracker.processInputEvent(
			JSON.stringify({ type: "storage", ts: Date.now(), storageType: "session", changeType: "set", key: "x", newValue: "1" }),
			"tab1"
		);
		expect(local?.data.storageType).toBe("local");
		expect(session?.data.storageType).toBe("session");
	});

	it("summary contains key name", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({ type: "storage", ts: Date.now(), storageType: "local", changeType: "added", key: "cart", newValue: "[]" }),
			"tab1"
		);
		expect(result?.summary).toContain("cart");
	});
});
```

### Unit Tests: `tests/unit/browser/input-tracker-dom-mutation.test.ts`

```typescript
describe("InputTracker dom_mutation events", () => {
	const tracker = new InputTracker();

	it("processes dom_mutation into dom_mutation event", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({
				type: "dom_mutation",
				ts: Date.now(),
				added: [{ selector: "#modal", tag: "dialog", text: "Are you sure?" }],
				removed: []
			}),
			"tab1"
		);
		expect(result?.type).toBe("dom_mutation");
		expect(result?.data.added).toHaveLength(1);
		expect((result?.data.added as Array<{ selector: string }>)[0].selector).toBe("#modal");
	});

	it("processes removals", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({
				type: "dom_mutation",
				ts: Date.now(),
				added: [],
				removed: [{ selector: "[data-testid=\"loading-spinner\"]", tag: "div" }]
			}),
			"tab1"
		);
		expect(result?.data.removed).toHaveLength(1);
	});

	it("summary contains selector names", () => {
		const result = tracker.processInputEvent(
			JSON.stringify({ type: "dom_mutation", ts: Date.now(), added: [{ selector: "#confirm-dialog", tag: "dialog" }], removed: [] }),
			"tab1"
		);
		expect(result?.summary).toContain("#confirm-dialog");
	});
});
```

### Unit Tests: `tests/unit/browser/input-tracker-cls.test.ts` (existing)

The injection script tests are already in this file. Add one for storage:

```typescript
it("getInjectionScript includes localStorage proxy", () => {
	const script = tracker.getInjectionScript();
	expect(script).toContain("localStorage");
	expect(script).toContain("setItem");
	expect(script).toContain("removeItem");
});

it("getInjectionScript includes MutationObserver", () => {
	const script = tracker.getInjectionScript();
	expect(script).toContain("MutationObserver");
	expect(script).toContain("childList");
});
```

### Integration verification

The full path (injection → `__BL__` console event → `EventPipeline` → `storage_change` event → `SessionDiffer.diffStorage()`) is best verified via an end-to-end test or manual testing with a real browser session. At unit test level, the individual stages are covered: `InputTracker` (above) and `SessionDiffer.diffStorage()` (existing diff.test.ts — currently tests with no data; once events flow, the test in diff.test.ts line 195-199 for `storageChanges` will naturally get coverage).

---

## Verification Checklist

```bash
bun run test:unit   # New storage/dom-mutation tracker tests pass
bun run lint        # No lint errors
bun run build       # Compiles cleanly
```

Manual smoke test (requires Chrome):
1. Start browser recorder, navigate to any app using localStorage
2. Set a localStorage value → verify `storage_change` event appears in `session_search`
3. Run `session_diff` over a range where storage was mutated → verify `storageChanges` is populated
4. Navigate to a page that shows/hides modals → verify `dom_mutation` events appear in search results
