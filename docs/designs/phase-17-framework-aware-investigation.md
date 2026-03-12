# Design: Phase 17 — Framework-Aware Investigation

## Overview

Extends the investigation tools (`session_search`, `session_inspect`, `session_overview`, `session_diff`) to understand framework events natively. Agents can search by component name, filter by framework, see framework context in overviews and diffs, and get rich detail when inspecting framework events.

**Depends on:** Phases 15 (React) and 16 (Vue) — framework_detect, framework_state, and framework_error events must already flow through the pipeline.

**No new MCP tools.** All changes extend existing tool parameters and renderers. Framework events are queryable through the same `session_search`, visible in `session_overview`, inspectable via `session_inspect`, and comparable via `session_diff`.

---

## Implementation Units

### Unit 1: Query Engine Framework Filters

**File**: `src/browser/investigation/query-engine.ts`

Extend `SearchParams.filters` with three framework-specific post-filters. These follow the same post-filter pattern used by `statusCodes`, `consoleLevels`, and `containsText`.

```typescript
export interface SearchParams {
	query?: string;
	filters?: {
		eventTypes?: string[];
		statusCodes?: number[];
		urlPattern?: string;
		consoleLevels?: string[];
		timeRange?: { start: number; end: number };
		containsText?: string;
		aroundMarker?: string;

		// NEW: Framework-specific filters
		/** Filter by framework name. Implies eventTypes narrowed to framework_* types. */
		framework?: string;
		/** Filter by component name (substring match on summary). */
		component?: string;
		/** Filter by bug pattern name (exact match on framework_error events). */
		pattern?: string;
	};
	maxResults?: number;
}
```

**Implementation Notes**:

The three new filters work as post-filters on the summary string, which already encodes framework, component, and pattern info via `FrameworkTracker.buildSummary()`:

- `framework_detect` summary: `[react] React 18.2.0 detected (1 root)`
- `framework_state` summary: `[react] UserProfile: update (render #3)`
- `framework_error` summary: `[react:high] stale_closure in UserProfile`

Filter logic in `search()`:

```typescript
// Framework filter — restrict to framework_* event types AND match [framework] prefix
if (params.filters?.framework) {
	const fw = params.filters.framework;
	// Auto-narrow to framework event types if not already specified
	if (!params.filters.eventTypes || params.filters.eventTypes.length === 0) {
		params = {
			...params,
			filters: {
				...params.filters,
				eventTypes: ["framework_detect", "framework_state", "framework_error"],
			},
		};
	}
	results = results.filter((e) => e.summary.startsWith(`[${fw}]`) || e.summary.startsWith(`[${fw}:`));
}

// Component filter — substring match on component name in summary
if (params.filters?.component) {
	const comp = params.filters.component;
	results = results.filter((e) =>
		e.type.startsWith("framework_") && e.summary.includes(comp)
	);
}

// Pattern filter — match pattern name in framework_error summaries
if (params.filters?.pattern) {
	const pat = params.filters.pattern;
	results = results.filter((e) =>
		e.type === "framework_error" && e.summary.includes(pat)
	);
}
```

The `needsPostFilter` check must be extended to include these new filters:

```typescript
const needsPostFilter = !!(
	params.filters?.statusCodes?.length ||
	params.filters?.urlPattern ||
	params.filters?.framework ||
	params.filters?.component ||
	params.filters?.pattern
);
```

**Acceptance Criteria**:
- [ ] `search(sid, { filters: { framework: "react" } })` returns only framework_* events with `[react]` in summary
- [ ] `search(sid, { filters: { component: "UserProfile" } })` returns framework events mentioning "UserProfile"
- [ ] `search(sid, { filters: { pattern: "stale_closure" } })` returns only framework_error events with that pattern
- [ ] Filters combine with existing filters (timeRange, aroundMarker, etc.)
- [ ] FTS5 `query` param still works alongside framework filters (e.g., `query: "UserProfile"` finds framework events via summary text)

---

### Unit 2: Framework Summary in Overview

**File**: `src/browser/investigation/query-engine.ts`

Add a `frameworkSummary` field to `SessionOverview` and populate it from framework_detect and framework_state events.

```typescript
export interface FrameworkSummary {
	/** Detected frameworks with version info. */
	frameworks: Array<{
		name: string;
		version: string;
		componentCount: number;
		storeDetected?: string;
	}>;
	/** Total framework_state events in the session. */
	stateEventCount: number;
	/** Total framework_error events, grouped by severity. */
	errors: { high: number; medium: number; low: number };
	/** Top components by update frequency (most active first). */
	topComponents: Array<{ name: string; updateCount: number }>;
}

export interface SessionOverview {
	session: { id: string; startedAt: number; url: string; title: string };
	markers: MarkerRow[];
	timeline: EventRow[];
	networkSummary: NetworkSummary | null;
	errorSummary: EventRow[] | null;
	// NEW
	frameworkSummary: FrameworkSummary | null;
}
```

**Implementation Notes**:

Add `"framework"` to the `OverviewOptions.include` union:

```typescript
export interface OverviewOptions {
	include?: ("timeline" | "markers" | "errors" | "network_summary" | "framework")[];
	aroundMarker?: string;
	timeRange?: { start: number; end: number };
}
```

In `getOverview()`, when `include` is omitted (default: all) or contains `"framework"`:

```typescript
// Framework summary
if (!options?.include || options.include.includes("framework")) {
	result.frameworkSummary = this.summarizeFramework(sessionId);
}
```

New private method:

```typescript
private summarizeFramework(sessionId: string): FrameworkSummary | null {
	// Get framework_detect events
	const detectEvents = this.db.queryEvents(sessionId, {
		types: ["framework_detect"],
	});
	if (detectEvents.length === 0) return null;

	// Parse framework info from detect event summaries + full events
	const frameworks: FrameworkSummary["frameworks"] = [];
	for (const e of detectEvents) {
		const full = this.getFullEvent(sessionId, e.event_id);
		if (full) {
			frameworks.push({
				name: full.data.framework as string,
				version: (full.data.version as string) ?? "unknown",
				componentCount: (full.data.componentCount as number) ?? 0,
				storeDetected: full.data.storeDetected as string | undefined,
			});
		}
	}

	// Count state and error events
	const stateEvents = this.db.queryEvents(sessionId, {
		types: ["framework_state"],
	});
	const errorEvents = this.db.queryEvents(sessionId, {
		types: ["framework_error"],
	});

	const errors = { high: 0, medium: 0, low: 0 };
	for (const e of errorEvents) {
		const severity = e.summary.match(/\[.*?:(high|medium|low)\]/)?.[1];
		if (severity && severity in errors) {
			errors[severity as keyof typeof errors]++;
		}
	}

	// Top components by update frequency
	const componentCounts = new Map<string, number>();
	for (const e of stateEvents) {
		// Summary format: "[react] ComponentName: update (render #N)"
		const match = e.summary.match(/\[.*?\] (.+?):/);
		if (match) {
			const name = match[1];
			componentCounts.set(name, (componentCounts.get(name) ?? 0) + 1);
		}
	}
	const topComponents = [...componentCounts.entries()]
		.sort((a, b) => b[1] - a[1])
		.slice(0, 10)
		.map(([name, updateCount]) => ({ name, updateCount }));

	return {
		frameworks,
		stateEventCount: stateEvents.length,
		errors,
		topComponents,
	};
}
```

**Acceptance Criteria**:
- [ ] `getOverview(sid)` returns `frameworkSummary` when framework events exist
- [ ] `getOverview(sid)` returns `frameworkSummary: null` when no framework detected
- [ ] `getOverview(sid, { include: ["timeline"] })` does NOT compute frameworkSummary
- [ ] `topComponents` lists the 10 most frequently updated components
- [ ] Error counts are correctly grouped by severity

---

### Unit 3: Framework-Aware Overview Rendering

**File**: `src/browser/investigation/renderers.ts`

Add a "Framework" section to `renderSessionOverview`. Priority 75 — between errors (80) and timeline (60).

```typescript
// In renderSessionOverview(), after the errors section:

// Framework summary (priority between errors and timeline)
if (overview.frameworkSummary) {
	const fs = overview.frameworkSummary;
	const fwLines: string[] = ["Framework:"];
	for (const fw of fs.frameworks) {
		const store = fw.storeDetected ? ` + ${fw.storeDetected}` : "";
		fwLines.push(`  ${fw.name} ${fw.version} (${fw.componentCount} components${store})`);
	}
	fwLines.push(`  ${fs.stateEventCount} state events`);
	if (fs.errors.high > 0 || fs.errors.medium > 0) {
		const parts: string[] = [];
		if (fs.errors.high > 0) parts.push(`${fs.errors.high} high`);
		if (fs.errors.medium > 0) parts.push(`${fs.errors.medium} medium`);
		if (fs.errors.low > 0) parts.push(`${fs.errors.low} low`);
		fwLines.push(`  Bug patterns: ${parts.join(", ")}`);
	}
	if (fs.topComponents.length > 0) {
		fwLines.push("  Most active components:");
		for (const c of fs.topComponents.slice(0, 5)) {
			fwLines.push(`    ${c.name} (${c.updateCount} updates)`);
		}
	}
	fwLines.push("");
	sections.push({ key: "framework", content: fwLines.join("\n"), priority: 75 });
}
```

**Output Example**:

```
Framework:
  react 18.3.1 (47 components)
  142 state events
  Bug patterns: 1 high, 2 medium
  Most active components:
    SearchBar (23 updates)
    UserProfile (18 updates)
    CartContext (15 updates)
```

**Acceptance Criteria**:
- [ ] Framework section appears in overview when framework events exist
- [ ] Framework section is omitted when no framework detected
- [ ] Section respects token budget (priority 75 — dropped before markers/errors, kept before timeline/network)

---

### Unit 4: Framework-Aware Search Result Rendering

**File**: `src/browser/investigation/renderers.ts`

Update `renderSearchResults` to format framework events more compactly. The existing format shows `[type] summary`, which for framework events would look like `[framework_state] [react] UserProfile: update (render #3)`. The type prefix is redundant with the summary bracket.

```typescript
export function renderSearchResults(results: EventRow[], tokenBudget = 2000): string {
	if (results.length === 0) return "No matching events found.";

	const lines: string[] = [`Found ${results.length} events:\n`];
	let tokens = estimateTokens(lines[0]);

	for (const r of results) {
		const line = formatSearchResultLine(r);
		const lineTokens = estimateTokens(line);
		if (tokens + lineTokens > tokenBudget) {
			lines.push(`  ... (${results.length - lines.length + 1} more results)`);
			break;
		}
		lines.push(line);
		tokens += lineTokens;
	}

	return lines.join("\n");
}

function formatSearchResultLine(r: EventRow): string {
	const time = formatTime(r.timestamp);
	// Framework events already have [framework] prefix in summary — skip redundant [type]
	if (r.type.startsWith("framework_")) {
		return `  ${time}  ${r.summary}  (id: ${r.event_id})`;
	}
	return `  ${time}  [${r.type}] ${r.summary}  (id: ${r.event_id})`;
}
```

**Output Example** (framework events in search results):

```
Found 5 events:

  14:23:01.445  [react] React 18.3.1 detected (1 root)  (id: e-abc123)
  14:23:02.112  [react] UserProfile: isLoading false→true (render #3)  (id: e-def456)
  14:23:02.200  [react:medium] stale_closure in SearchBar  (id: e-ghi789)
  14:23:03.500  [react:high] infinite_rerender in CartContext  (id: e-jkl012)
  14:23:04.001  [react] UserProfile: unmount  (id: e-mno345)
```

**Acceptance Criteria**:
- [ ] Framework events render without redundant `[framework_state]` prefix
- [ ] Non-framework events still render with `[type]` prefix
- [ ] Token budget is respected

---

### Unit 5: Framework-Aware Inspect Rendering

**File**: `src/browser/investigation/renderers.ts`

Add framework-specific detail rendering in `renderInspectResult`. When the inspected event is a framework event, display its data fields in a structured way.

```typescript
// In renderInspectResult(), extend the event detail section:

if (event.type === "framework_detect") {
	const d = event.data as Record<string, unknown>;
	eventLines.push(`Framework: ${d.framework} ${d.version}`);
	if (d.rootCount != null) eventLines.push(`Roots: ${d.rootCount}`);
	if (d.componentCount != null) eventLines.push(`Components: ${d.componentCount}`);
	if (d.storeDetected) eventLines.push(`Store: ${d.storeDetected}`);
	if (d.bundleType != null) eventLines.push(`Build: ${d.bundleType === 1 ? "development" : "production"}`);
}

if (event.type === "framework_state") {
	const d = event.data as Record<string, unknown>;
	if (d.componentPath) eventLines.push(`Path: ${d.componentPath}`);
	eventLines.push(`Change: ${d.changeType}`);
	if (d.triggerSource) eventLines.push(`Trigger: ${d.triggerSource}`);
	if (d.renderCount != null) eventLines.push(`Render #${d.renderCount}`);
	if (d.storeId) eventLines.push(`Store: ${d.storeId}`);
	if (d.actionName) eventLines.push(`Action: ${d.actionName}`);
	if (d.mutationType) eventLines.push(`Mutation type: ${d.mutationType}`);

	// State changes
	if (Array.isArray(d.changes) && d.changes.length > 0) {
		eventLines.push("Changes:");
		for (const change of d.changes as Array<{ key: string; prev: unknown; next: unknown }>) {
			const prev = formatValue(change.prev);
			const next = formatValue(change.next);
			eventLines.push(`  ${change.key}: ${prev} → ${next}`);
		}
	}
}

if (event.type === "framework_error") {
	const d = event.data as Record<string, unknown>;
	eventLines.push(`Pattern: ${d.pattern}`);
	eventLines.push(`Severity: ${d.severity}`);
	if (d.detail) eventLines.push(`Detail: ${d.detail}`);
	if (d.evidence && typeof d.evidence === "object") {
		eventLines.push("Evidence:");
		for (const [k, v] of Object.entries(d.evidence as Record<string, unknown>)) {
			eventLines.push(`  ${k}: ${formatValue(v)}`);
		}
	}
}
```

Add a helper for formatting values compactly:

```typescript
function formatValue(v: unknown): string {
	if (v === null || v === undefined) return String(v);
	if (typeof v === "string") return v.length > 80 ? `"${v.slice(0, 80)}..."` : `"${v}"`;
	if (typeof v === "object") {
		try {
			const s = JSON.stringify(v);
			return s.length > 100 ? `${s.slice(0, 100)}...` : s;
		} catch {
			return "[Object]";
		}
	}
	return String(v);
}
```

**Output Example** (inspecting a framework_state event):

```
Event: [react] UserProfile: update (render #4)
Type: framework_state
Time: 14:23:02.118
ID: e-def456
Path: App > Layout > UserProfile
Change: update
Trigger: state
Render #4
Changes:
  state[0]: true → false
  props.data: null → {"id":482,"name":"Alice"}
```

**Output Example** (inspecting a framework_error event):

```
Event: [react:high] infinite_rerender in CartContext
Type: framework_error
Time: 14:23:03.500
ID: e-jkl012
Pattern: infinite_rerender
Severity: high
Detail: CartContext rendered 23 times in 1000ms. Likely setState in useEffect without proper deps.
Evidence:
  rendersInWindow: 23
  windowMs: 1000
  lastState: {"items":[...],"total":42}
```

**Acceptance Criteria**:
- [ ] `framework_detect` events show framework, version, root count, component count, store, build type
- [ ] `framework_state` events show component path, change type, trigger source, render count, and state diff
- [ ] `framework_error` events show pattern, severity, detail, and evidence
- [ ] Non-framework events render unchanged
- [ ] Token budget is respected (large evidence objects are truncated)

---

### Unit 6: Framework Context in Diffs

**File**: `src/browser/investigation/diff.ts`

Extend `DiffParams.include` and `DiffResult` with framework state comparison.

```typescript
export interface DiffParams {
	sessionId: string;
	before: string;
	after: string;
	include?: ("form_state" | "storage" | "url" | "console_new" | "network_new" | "framework_state")[];
}

export interface FrameworkDiffEntry {
	componentName: string;
	changeType: "mounted" | "unmounted" | "updated";
	/** State changes for updated components. */
	changes?: Array<{ key: string; prev: unknown; next: unknown }>;
}

export interface DiffResult {
	beforeTime: number;
	afterTime: number;
	durationMs: number;
	urlChange?: { before: string; after: string };
	formChanges?: Array<{ selector: string; before: string; after: string }>;
	storageChanges?: Array<{ key: string; type: "added" | "removed" | "changed"; before?: string; after?: string }>;
	newConsoleMessages?: Array<{ timestamp: number; level: string; summary: string }>;
	newNetworkRequests?: Array<{ timestamp: number; summary: string }>;

	// NEW
	/** Framework component changes between the two moments. */
	frameworkChanges?: FrameworkDiffEntry[];
	/** Store mutations between the two moments. */
	storeMutations?: Array<{ storeId: string; mutationType: string; actionName?: string; timestamp: number }>;
	/** Bug patterns active between the two moments. */
	frameworkErrors?: Array<{ pattern: string; componentName: string; severity: string; detail: string }>;
}
```

**Implementation Notes**:

New private method in `SessionDiffer`:

```typescript
private diffFrameworkState(sessionId: string, before: number, after: number): {
	frameworkChanges: FrameworkDiffEntry[];
	storeMutations: DiffResult["storeMutations"];
	frameworkErrors: DiffResult["frameworkErrors"];
} {
	// Get all framework_state events in the window
	const stateEvents = this.queryEngine.search(sessionId, {
		filters: {
			eventTypes: ["framework_state"],
			timeRange: { start: before, end: after },
		},
		maxResults: 100,
	});

	// Build component change list
	const componentChanges = new Map<string, FrameworkDiffEntry>();
	const storeMutations: NonNullable<DiffResult["storeMutations"]> = [];

	for (const e of stateEvents) {
		const full = this.queryEngine.getFullEvent(sessionId, e.event_id);
		if (!full) continue;

		const d = full.data;
		const name = d.componentName as string;
		const changeType = d.changeType as string;

		// Store mutations go to separate list
		if (changeType === "store_mutation") {
			storeMutations.push({
				storeId: (d.storeId as string) ?? "unknown",
				mutationType: (d.mutationType as string) ?? "direct",
				actionName: d.actionName as string | undefined,
				timestamp: full.timestamp,
			});
			continue;
		}

		// Component changes — keep latest per component
		const existing = componentChanges.get(name);
		const mappedType = changeType === "mount" ? "mounted" as const
			: changeType === "unmount" ? "unmounted" as const
			: "updated" as const;

		if (!existing || mappedType === "unmounted") {
			componentChanges.set(name, {
				componentName: name,
				changeType: mappedType,
				changes: Array.isArray(d.changes) ? d.changes as FrameworkDiffEntry["changes"] : undefined,
			});
		} else if (mappedType === "updated" && existing.changeType !== "mounted") {
			// Merge updates — keep latest changes
			existing.changes = Array.isArray(d.changes) ? d.changes as FrameworkDiffEntry["changes"] : existing.changes;
		}
	}

	// Get framework errors in the window
	const errorEvents = this.queryEngine.search(sessionId, {
		filters: {
			eventTypes: ["framework_error"],
			timeRange: { start: before, end: after },
		},
		maxResults: 20,
	});

	const frameworkErrors: NonNullable<DiffResult["frameworkErrors"]> = [];
	for (const e of errorEvents) {
		const full = this.queryEngine.getFullEvent(sessionId, e.event_id);
		if (!full) continue;
		frameworkErrors.push({
			pattern: (full.data.pattern as string) ?? "unknown",
			componentName: (full.data.componentName as string) ?? "?",
			severity: (full.data.severity as string) ?? "medium",
			detail: (full.data.detail as string) ?? "",
		});
	}

	return {
		frameworkChanges: [...componentChanges.values()],
		storeMutations: storeMutations.length > 0 ? storeMutations : undefined,
		frameworkErrors: frameworkErrors.length > 0 ? frameworkErrors : undefined,
	};
}
```

Wire into `diff()`:

```typescript
// In diff() method, after existing sections:
if (include.has("framework_state")) {
	const fw = this.diffFrameworkState(params.sessionId, beforeTs, afterTs);
	if (fw.frameworkChanges.length > 0) result.frameworkChanges = fw.frameworkChanges;
	if (fw.storeMutations) result.storeMutations = fw.storeMutations;
	if (fw.frameworkErrors) result.frameworkErrors = fw.frameworkErrors;
}
```

**Note**: `"framework_state"` is NOT in the default includes. It must be explicitly requested. This prevents expensive full-event reads for diffs that don't need framework context.

**Acceptance Criteria**:
- [ ] `diff({ include: ["framework_state"] })` returns component mount/unmount/update deltas
- [ ] Store mutations are listed separately with storeId, type, and action
- [ ] Framework errors detected between the two moments are included
- [ ] Default `diff()` call (no include) does NOT compute framework changes (not in default set)
- [ ] Multiple updates to the same component collapse to the latest state

---

### Unit 7: Framework Diff Rendering

**File**: `src/browser/investigation/renderers.ts`

Add framework sections to `renderDiff`. Priority levels:
- Framework errors: 65 (just above console)
- Framework component changes: 55 (between console and network)
- Store mutations: 45 (just below network)

```typescript
// In renderDiff(), after existing sections:

// Framework errors (high priority — bugs between the two moments)
if (diff.frameworkErrors && diff.frameworkErrors.length > 0) {
	const lines = ["Framework Bug Patterns:"];
	for (const e of diff.frameworkErrors) {
		lines.push(`  [${e.severity}] ${e.pattern} in ${e.componentName}`);
		if (e.detail) lines.push(`    ${e.detail.slice(0, 120)}`);
	}
	lines.push("");
	sections.push({ key: "framework_errors", content: lines.join("\n"), priority: 65 });
}

// Framework component changes
if (diff.frameworkChanges && diff.frameworkChanges.length > 0) {
	const lines = ["Component Changes:"];
	const mounted = diff.frameworkChanges.filter((c) => c.changeType === "mounted");
	const unmounted = diff.frameworkChanges.filter((c) => c.changeType === "unmounted");
	const updated = diff.frameworkChanges.filter((c) => c.changeType === "updated");

	if (mounted.length > 0) {
		lines.push(`  Mounted (${mounted.length}): ${mounted.map((c) => c.componentName).join(", ")}`);
	}
	if (unmounted.length > 0) {
		lines.push(`  Unmounted (${unmounted.length}): ${unmounted.map((c) => c.componentName).join(", ")}`);
	}
	for (const c of updated) {
		if (c.changes && c.changes.length > 0) {
			const changeSummary = c.changes
				.slice(0, 3)
				.map((ch) => `${ch.key}: ${formatValue(ch.prev)} → ${formatValue(ch.next)}`)
				.join(", ");
			const more = c.changes.length > 3 ? ` +${c.changes.length - 3} more` : "";
			lines.push(`  ~ ${c.componentName}: ${changeSummary}${more}`);
		} else {
			lines.push(`  ~ ${c.componentName}: updated`);
		}
	}
	lines.push("");
	sections.push({ key: "framework_components", content: lines.join("\n"), priority: 55 });
}

// Store mutations
if (diff.storeMutations && diff.storeMutations.length > 0) {
	const lines = [`Store Mutations (${diff.storeMutations.length}):`];
	for (const m of diff.storeMutations.slice(0, 10)) {
		const action = m.actionName ? ` (action: ${m.actionName})` : "";
		lines.push(`  ${formatTime(m.timestamp)}  ${m.storeId}: ${m.mutationType}${action}`);
	}
	if (diff.storeMutations.length > 10) {
		lines.push(`  ... (${diff.storeMutations.length - 10} more)`);
	}
	lines.push("");
	sections.push({ key: "store_mutations", content: lines.join("\n"), priority: 45 });
}
```

**Output Example**:

```
Diff: 14:23:01.000 → 14:23:05.000 (4s)

URL: /login → /dashboard

Framework Bug Patterns:
  [high] infinite_rerender in CartContext
    CartContext rendered 23 times in 1000ms. Likely setState in useEffect without proper deps.

Component Changes:
  Mounted (3): Dashboard, Sidebar, CartWidget
  Unmounted (1): LoginForm
  ~ UserProfile: state[0]: true → false, props.data: null → {"id":482}
  ~ SearchBar: updated

Store Mutations (2):
  14:23:02.500  cart: direct (action: addItem)
  14:23:03.200  cart: patch object

Form State Changes:
  #username  "" → "admin"
```

**Acceptance Criteria**:
- [ ] Framework errors section appears with severity and pattern name
- [ ] Component changes are grouped by mount/unmount/update
- [ ] Updated components show their state diffs inline
- [ ] Store mutations show timestamp, storeId, mutation type, and action name
- [ ] Sections respect token budget priorities

---

### Unit 8: MCP Tool Parameter Updates

**File**: `src/mcp/tools/browser.ts`

Update Zod schemas for the three extended tools.

#### session_search — new parameters

```typescript
server.tool(
	"session_search",
	"Search recorded browser session events. Supports natural language search (uses FTS5) " +
		"and structured filters (event type, status code, time range, framework, component, pattern). " +
		"Use natural language for exploratory search, structured filters for precise queries.",
	{
		// ... existing params ...
		event_types: z
			.array(z.enum([
				"navigation", "network_request", "network_response", "console",
				"page_error", "user_input", "websocket", "performance", "marker",
				// NEW: framework event types
				"framework_detect", "framework_state", "framework_error",
			]))
			.optional()
			.describe("Filter by event type"),
		// ... existing params ...

		// NEW params:
		framework: z
			.enum(["react", "vue", "solid", "svelte"])
			.optional()
			.describe("Filter by framework. Automatically narrows to framework event types."),
		component: z
			.string()
			.optional()
			.describe("Filter by component name (substring match), e.g. 'UserProfile'"),
		pattern: z
			.string()
			.optional()
			.describe("Filter by bug pattern name, e.g. 'stale_closure', 'infinite_rerender'"),
	},
	async ({ session_id, query, event_types, status_codes, time_range, around_marker,
		url_pattern, console_levels, contains_text, limit, token_budget,
		framework, component, pattern }) => {
		try {
			const results = queryEngine.search(session_id, {
				query,
				filters: {
					eventTypes: event_types,
					statusCodes: status_codes,
					timeRange: time_range
						? { start: new Date(time_range.start).getTime(), end: new Date(time_range.end).getTime() }
						: undefined,
					aroundMarker: around_marker,
					urlPattern: url_pattern,
					consoleLevels: console_levels,
					containsText: contains_text,
					// NEW
					framework,
					component,
					pattern,
				},
				maxResults: limit ?? 10,
			});
			return {
				content: [{ type: "text" as const, text: renderSearchResults(results, token_budget ?? 2000) }],
			};
		} catch (err) {
			return errorResponse(err);
		}
	},
);
```

#### session_overview — add "framework" to include

```typescript
include: z
	.array(z.enum(["timeline", "markers", "errors", "network_summary", "framework"]))
	.optional()
	.describe("What to include. Default: all"),
```

#### session_diff — add "framework_state" to include

```typescript
include: z
	.array(z.enum(["form_state", "storage", "url", "console_new", "network_new", "framework_state"]))
	.optional()
	.describe("What to diff. Default: form_state, storage, url, console_new, network_new (framework_state must be explicitly requested)"),
```

**Acceptance Criteria**:
- [ ] `session_search` accepts `framework`, `component`, and `pattern` parameters
- [ ] `session_search` `event_types` enum includes `framework_detect`, `framework_state`, `framework_error`
- [ ] `session_overview` `include` enum includes `"framework"`
- [ ] `session_diff` `include` enum includes `"framework_state"`
- [ ] Zod validation rejects invalid framework names

---

## Implementation Order

1. **Unit 1: Query Engine Framework Filters** — Core filtering logic. No dependencies.
2. **Unit 2: Framework Summary in Overview** — Depends on query engine types but no filter logic.
3. **Unit 8: MCP Tool Parameter Updates** — Wire new params through; depends on Units 1 & 2 types.
4. **Unit 4: Framework-Aware Search Result Rendering** — Independent formatting change.
5. **Unit 3: Framework-Aware Overview Rendering** — Depends on Unit 2 (FrameworkSummary type).
6. **Unit 5: Framework-Aware Inspect Rendering** — Independent formatting change.
7. **Unit 6: Framework Context in Diffs** — Diff logic; depends on query engine.
8. **Unit 7: Framework Diff Rendering** — Depends on Unit 6 (DiffResult types).

Units 3, 4, 5 can be implemented in parallel after Units 1 and 2.

---

## Testing

### Unit Tests: `tests/unit/browser/query-engine-framework.test.ts`

Tests for the new framework filter logic in `QueryEngine.search()`. Follow the pattern from `query-engine-filters.test.ts`.

**Setup**: Create a session with framework events:

```typescript
const frameworkEvents: RecordedEvent[] = [
	{
		id: "fw-detect",
		timestamp: BASE_TS,
		type: "framework_detect",
		tabId: "tab1",
		summary: "[react] React 18.2.0 detected (1 root)",
		data: { framework: "react", version: "18.2.0", rootCount: 1, componentCount: 12 },
	},
	{
		id: "fw-state-1",
		timestamp: BASE_TS + 1000,
		type: "framework_state",
		tabId: "tab1",
		summary: "[react] UserProfile: mount (render #1)",
		data: { framework: "react", componentName: "UserProfile", changeType: "mount", renderCount: 1 },
	},
	{
		id: "fw-state-2",
		timestamp: BASE_TS + 2000,
		type: "framework_state",
		tabId: "tab1",
		summary: "[react] SearchBar: update (render #5)",
		data: { framework: "react", componentName: "SearchBar", changeType: "update", renderCount: 5,
			changes: [{ key: "state[0]", prev: "hello", next: "hello w" }] },
	},
	{
		id: "fw-error-1",
		timestamp: BASE_TS + 3000,
		type: "framework_error",
		tabId: "tab1",
		summary: "[react:high] infinite_rerender in CartContext",
		data: { framework: "react", pattern: "infinite_rerender", componentName: "CartContext",
			severity: "high", detail: "CartContext rendered 23 times in 1000ms",
			evidence: { rendersInWindow: 23, windowMs: 1000 } },
	},
	{
		id: "fw-vue-detect",
		timestamp: BASE_TS + 500,
		type: "framework_detect",
		tabId: "tab1",
		summary: "[vue] Vue 3.4.21 detected (1 root)",
		data: { framework: "vue", version: "3.4.21", rootCount: 1, componentCount: 8 },
	},
];
```

**Key test cases**:

```typescript
describe("framework filters", () => {
	test("framework filter returns only matching framework events", () => {
		const results = engine.search(SID, { filters: { framework: "react" } });
		expect(results.every((r) => r.summary.startsWith("[react]") || r.summary.startsWith("[react:"))).toBe(true);
		expect(results.length).toBe(3); // detect + state + error (not vue)
	});

	test("component filter matches component name in summary", () => {
		const results = engine.search(SID, { filters: { component: "UserProfile" } });
		expect(results.length).toBe(1);
		expect(results[0].event_id).toBe("fw-state-1");
	});

	test("pattern filter matches framework_error events only", () => {
		const results = engine.search(SID, { filters: { pattern: "infinite_rerender" } });
		expect(results.length).toBe(1);
		expect(results[0].type).toBe("framework_error");
	});

	test("filters combine with timeRange", () => {
		const results = engine.search(SID, {
			filters: { framework: "react", timeRange: { start: BASE_TS, end: BASE_TS + 1500 } },
		});
		expect(results.length).toBe(2); // detect + state-1
	});

	test("framework filter auto-narrows event types", () => {
		const results = engine.search(SID, { filters: { framework: "react" } });
		expect(results.every((r) => r.type.startsWith("framework_"))).toBe(true);
	});
});
```

### Unit Tests: `tests/unit/browser/renderers-framework.test.ts`

Tests for framework-aware rendering. Follow the pattern from `renderers.test.ts`.

**Key test cases**:

```typescript
describe("framework rendering", () => {
	test("renderSessionOverview includes framework section", () => {
		const overview = makeOverview({ frameworkSummary: makeFrameworkSummary() });
		const text = renderSessionOverview(overview);
		expect(text).toContain("Framework:");
		expect(text).toContain("react 18.2.0");
		expect(text).toContain("Most active components:");
	});

	test("renderSessionOverview omits framework section when null", () => {
		const overview = makeOverview({ frameworkSummary: null });
		const text = renderSessionOverview(overview);
		expect(text).not.toContain("Framework:");
	});

	test("renderSearchResults omits type prefix for framework events", () => {
		const results = [makeEventRow({ type: "framework_state", summary: "[react] Foo: update (render #2)" })];
		const text = renderSearchResults(results);
		expect(text).not.toContain("[framework_state]");
		expect(text).toContain("[react] Foo: update");
	});

	test("renderInspectResult shows framework_state detail", () => {
		const result = makeInspectResult({
			event: makeEvent({
				type: "framework_state",
				data: { componentPath: "App > Foo", changeType: "update", triggerSource: "state", renderCount: 3,
					changes: [{ key: "count", prev: 1, next: 2 }] },
			}),
		});
		const text = renderInspectResult(result);
		expect(text).toContain("Path: App > Foo");
		expect(text).toContain("Trigger: state");
		expect(text).toContain("count: 1 → 2");
	});

	test("renderDiff includes framework changes", () => {
		const diff = makeDiffResult({
			frameworkChanges: [
				{ componentName: "Dashboard", changeType: "mounted" },
				{ componentName: "LoginForm", changeType: "unmounted" },
			],
			frameworkErrors: [
				{ pattern: "infinite_rerender", componentName: "Cart", severity: "high", detail: "rendered 23 times" },
			],
		});
		const text = renderDiff(diff);
		expect(text).toContain("Component Changes:");
		expect(text).toContain("Mounted (1): Dashboard");
		expect(text).toContain("Unmounted (1): LoginForm");
		expect(text).toContain("Framework Bug Patterns:");
		expect(text).toContain("infinite_rerender in Cart");
	});
});
```

### Unit Tests: `tests/unit/browser/diff-framework.test.ts`

Tests for `SessionDiffer.diffFrameworkState()`. Follow the pattern from `diff.test.ts`.

**Key test cases**:

```typescript
describe("framework diff", () => {
	test("includes framework_state when explicitly requested", () => {
		const diff = differ.diff({
			sessionId: SID, before: t(0), after: t(5000),
			include: ["framework_state"],
		});
		expect(diff.frameworkChanges).toBeDefined();
	});

	test("excludes framework_state by default", () => {
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000) });
		expect(diff.frameworkChanges).toBeUndefined();
	});

	test("groups mount/unmount/update correctly", () => {
		// ... setup with mount + unmount + update events
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });
		const mounted = diff.frameworkChanges!.filter((c) => c.changeType === "mounted");
		const unmounted = diff.frameworkChanges!.filter((c) => c.changeType === "unmounted");
		expect(mounted.length).toBeGreaterThan(0);
		expect(unmounted.length).toBeGreaterThan(0);
	});

	test("store mutations listed separately", () => {
		// ... setup with store_mutation events
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });
		expect(diff.storeMutations).toBeDefined();
		expect(diff.storeMutations![0].storeId).toBe("cart");
	});
});
```

### E2E Tests: `tests/e2e/browser/framework-investigation.test.ts`

Full pipeline test using real React fixture app. Follow the pattern from `react-observer.test.ts`.

```typescript
describe("framework-aware investigation", () => {
	test("session_search with framework filter", async () => {
		// chrome_start with frameworkState → interact → chrome_mark → chrome_stop
		// session_search with framework: "react"
		// verify only react framework events returned
	});

	test("session_search with component filter", async () => {
		// search with component: "Counter"
		// verify events mention Counter component
	});

	test("session_overview includes framework summary", async () => {
		// session_overview with include: ["framework"]
		// verify framework section in output
	});

	test("session_inspect framework_state event", async () => {
		// search for framework_state → inspect the event
		// verify component path, state changes visible
	});

	test("session_diff with framework_state", async () => {
		// place marker before → interact → place marker after
		// diff with include: ["framework_state"]
		// verify component changes between markers
	});
});
```

**Fixture apps**: Use existing `tests/fixtures/browser/react-counter/` and `tests/fixtures/browser/vue3-counter/`. No new fixtures needed — the existing apps generate framework_detect and framework_state events.

---

## Verification Checklist

```bash
# Unit tests
bun run test:unit -- tests/unit/browser/query-engine-framework.test.ts
bun run test:unit -- tests/unit/browser/renderers-framework.test.ts
bun run test:unit -- tests/unit/browser/diff-framework.test.ts

# E2E tests (requires Chrome)
bun run test:e2e -- tests/e2e/browser/framework-investigation.test.ts

# Lint
bun run lint

# Full test suite
bun run test
```
