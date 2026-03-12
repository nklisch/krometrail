import { describe, expect, it } from "vitest";
import type { DiffResult } from "../../../src/browser/investigation/diff.js";
import type { FrameworkSummary, InspectResult, SessionOverview } from "../../../src/browser/investigation/query-engine.js";
import { renderDiff, renderInspectResult, renderSearchResults, renderSessionOverview } from "../../../src/browser/investigation/renderers.js";
import type { EventRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

const BASE_TS = 1709826622000;

function makeEventRow(overrides: Partial<EventRow> = {}): EventRow {
	return {
		rowid: 1,
		session_id: "sess-abc123",
		event_id: "e1",
		timestamp: BASE_TS,
		type: "console",
		summary: "[log] test",
		detail_offset: 0,
		detail_length: 100,
		...overrides,
	};
}

function makeFrameworkSummary(overrides: Partial<FrameworkSummary> = {}): FrameworkSummary {
	return {
		frameworks: [{ name: "react", version: "18.2.0", componentCount: 47 }],
		stateEventCount: 142,
		errors: { high: 1, medium: 2, low: 0 },
		topComponents: [
			{ name: "SearchBar", updateCount: 23 },
			{ name: "UserProfile", updateCount: 18 },
			{ name: "CartContext", updateCount: 15 },
		],
		...overrides,
	};
}

function makeOverview(overrides: Partial<SessionOverview> = {}): SessionOverview {
	return {
		session: { id: "sess-abc123", startedAt: BASE_TS, url: "https://example.com/app", title: "Example App" },
		markers: [],
		timeline: [],
		networkSummary: null,
		errorSummary: null,
		frameworkSummary: null,
		...overrides,
	};
}

function makeRecordedEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: "evt1",
		timestamp: BASE_TS,
		type: "navigation",
		tabId: "tab1",
		summary: "test",
		data: {},
		...overrides,
	};
}

function makeInspectResult(overrides: Partial<InspectResult> = {}): InspectResult {
	return {
		event: makeRecordedEvent(),
		surroundingEvents: [],
		networkBody: null,
		screenshot: null,
		...overrides,
	};
}

function makeDiffResult(overrides: Partial<DiffResult> = {}): DiffResult {
	return {
		beforeTime: BASE_TS,
		afterTime: BASE_TS + 4000,
		durationMs: 4000,
		...overrides,
	};
}

describe("renderSessionOverview — framework section", () => {
	it("includes framework section when frameworkSummary present", () => {
		const overview = makeOverview({ frameworkSummary: makeFrameworkSummary() });
		const text = renderSessionOverview(overview);
		expect(text).toContain("Framework:");
		expect(text).toContain("react 18.2.0");
		expect(text).toContain("47 components");
		expect(text).toContain("142 state events");
	});

	it("shows bug patterns when errors exist", () => {
		const overview = makeOverview({ frameworkSummary: makeFrameworkSummary() });
		const text = renderSessionOverview(overview);
		expect(text).toContain("Bug patterns:");
		expect(text).toContain("1 high");
		expect(text).toContain("2 medium");
	});

	it("shows most active components", () => {
		const overview = makeOverview({ frameworkSummary: makeFrameworkSummary() });
		const text = renderSessionOverview(overview);
		expect(text).toContain("Most active components:");
		expect(text).toContain("SearchBar (23 updates)");
		expect(text).toContain("UserProfile (18 updates)");
	});

	it("omits framework section when frameworkSummary is null", () => {
		const overview = makeOverview({ frameworkSummary: null });
		const text = renderSessionOverview(overview);
		expect(text).not.toContain("Framework:");
	});

	it("shows store detected in framework line", () => {
		const overview = makeOverview({
			frameworkSummary: makeFrameworkSummary({
				frameworks: [{ name: "vue", version: "3.4.21", componentCount: 8, storeDetected: "pinia" }],
			}),
		});
		const text = renderSessionOverview(overview);
		expect(text).toContain("+ pinia");
	});

	it("omits bug patterns line when no errors", () => {
		const overview = makeOverview({
			frameworkSummary: makeFrameworkSummary({ errors: { high: 0, medium: 0, low: 0 } }),
		});
		const text = renderSessionOverview(overview);
		expect(text).not.toContain("Bug patterns:");
	});
});

describe("renderSearchResults — framework event formatting", () => {
	it("omits type prefix for framework_state events", () => {
		const results = [makeEventRow({ type: "framework_state", summary: "[react] Foo: update (render #2)" })];
		const text = renderSearchResults(results);
		expect(text).not.toContain("[framework_state]");
		expect(text).toContain("[react] Foo: update");
	});

	it("omits type prefix for framework_detect events", () => {
		const results = [makeEventRow({ type: "framework_detect", summary: "[react] React 18.2.0 detected (1 root)" })];
		const text = renderSearchResults(results);
		expect(text).not.toContain("[framework_detect]");
		expect(text).toContain("[react] React 18.2.0 detected");
	});

	it("omits type prefix for framework_error events", () => {
		const results = [makeEventRow({ type: "framework_error", summary: "[react:high] infinite_rerender in CartContext" })];
		const text = renderSearchResults(results);
		expect(text).not.toContain("[framework_error]");
		expect(text).toContain("[react:high] infinite_rerender");
	});

	it("keeps type prefix for non-framework events", () => {
		const results = [makeEventRow({ type: "console", summary: "[log] something" })];
		const text = renderSearchResults(results);
		expect(text).toContain("[console]");
	});

	it("includes event id in framework result line", () => {
		const results = [makeEventRow({ event_id: "fw-abc", type: "framework_state", summary: "[react] Foo: update" })];
		const text = renderSearchResults(results);
		expect(text).toContain("(id: fw-abc)");
	});
});

describe("renderInspectResult — framework event detail", () => {
	it("shows framework_detect fields", () => {
		const event = makeRecordedEvent({
			type: "framework_detect",
			summary: "[react] React 18.2.0 detected",
			data: { framework: "react", version: "18.2.0", rootCount: 2, componentCount: 47, storeDetected: undefined, bundleType: 1 },
		});
		const text = renderInspectResult(makeInspectResult({ event }));
		expect(text).toContain("Framework: react 18.2.0");
		expect(text).toContain("Roots: 2");
		expect(text).toContain("Components: 47");
		expect(text).toContain("Build: development");
	});

	it("shows framework_state fields including changes", () => {
		const event = makeRecordedEvent({
			type: "framework_state",
			summary: "[react] UserProfile: update (render #4)",
			data: {
				componentPath: "App > Layout > UserProfile",
				changeType: "update",
				triggerSource: "state",
				renderCount: 4,
				changes: [{ key: "count", prev: 1, next: 2 }],
			},
		});
		const text = renderInspectResult(makeInspectResult({ event }));
		expect(text).toContain("Path: App > Layout > UserProfile");
		expect(text).toContain("Change: update");
		expect(text).toContain("Trigger: state");
		expect(text).toContain("Render #4");
		expect(text).toContain("Changes:");
		expect(text).toContain("count: 1 → 2");
	});

	it("shows framework_error fields including evidence", () => {
		const event = makeRecordedEvent({
			type: "framework_error",
			summary: "[react:high] infinite_rerender in CartContext",
			data: {
				pattern: "infinite_rerender",
				severity: "high",
				detail: "CartContext rendered 23 times in 1000ms",
				evidence: { rendersInWindow: 23, windowMs: 1000 },
			},
		});
		const text = renderInspectResult(makeInspectResult({ event }));
		expect(text).toContain("Pattern: infinite_rerender");
		expect(text).toContain("Severity: high");
		expect(text).toContain("Detail: CartContext rendered 23 times");
		expect(text).toContain("Evidence:");
		expect(text).toContain("rendersInWindow: 23");
	});

	it("non-framework events render unchanged", () => {
		const event = makeRecordedEvent({
			type: "console",
			summary: "[error] Something failed",
			data: { stackTrace: "at app.js:42" },
		});
		const text = renderInspectResult(makeInspectResult({ event }));
		expect(text).toContain("Type: console");
		expect(text).toContain("Stack: at app.js:42");
		expect(text).not.toContain("Pattern:");
		expect(text).not.toContain("Framework:");
	});
});

describe("renderDiff — framework sections", () => {
	it("includes component changes", () => {
		const diff = makeDiffResult({
			frameworkChanges: [
				{ componentName: "Dashboard", changeType: "mounted" },
				{ componentName: "LoginForm", changeType: "unmounted" },
			],
		});
		const text = renderDiff(diff);
		expect(text).toContain("Component Changes:");
		expect(text).toContain("Mounted (1): Dashboard");
		expect(text).toContain("Unmounted (1): LoginForm");
	});

	it("shows updated components with state diffs", () => {
		const diff = makeDiffResult({
			frameworkChanges: [
				{
					componentName: "UserProfile",
					changeType: "updated",
					changes: [{ key: "state[0]", prev: true, next: false }],
				},
			],
		});
		const text = renderDiff(diff);
		expect(text).toContain("~ UserProfile: state[0]: true → false");
	});

	it("includes framework bug patterns", () => {
		const diff = makeDiffResult({
			frameworkErrors: [{ pattern: "infinite_rerender", componentName: "Cart", severity: "high", detail: "rendered 23 times" }],
		});
		const text = renderDiff(diff);
		expect(text).toContain("Framework Bug Patterns:");
		expect(text).toContain("[high] infinite_rerender in Cart");
		expect(text).toContain("rendered 23 times");
	});

	it("includes store mutations", () => {
		const diff = makeDiffResult({
			storeMutations: [{ storeId: "cart", mutationType: "direct", actionName: "addItem", timestamp: BASE_TS + 500 }],
		});
		const text = renderDiff(diff);
		expect(text).toContain("Store Mutations (1):");
		expect(text).toContain("cart: direct (action: addItem)");
	});

	it("omits framework sections when not present", () => {
		const diff = makeDiffResult();
		const text = renderDiff(diff);
		expect(text).not.toContain("Component Changes:");
		expect(text).not.toContain("Framework Bug Patterns:");
		expect(text).not.toContain("Store Mutations:");
	});
});
