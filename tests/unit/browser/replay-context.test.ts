import { describe, expect, it } from "vitest";
import type { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import { ReplayContextGenerator } from "../../../src/browser/investigation/replay-context.js";
import type { EventRow, MarkerRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

function makeEventRow(overrides: Partial<EventRow> = {}): EventRow {
	return {
		rowid: 1,
		session_id: "s1",
		event_id: "e1",
		timestamp: 1_700_000_000_000,
		type: "navigation",
		summary: "Navigated to https://example.com",
		detail_offset: 0,
		detail_length: 10,
		...overrides,
	};
}

function makeFullEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: "e1",
		timestamp: 1_700_000_000_000,
		type: "navigation",
		tabId: "tab1",
		summary: "Navigated to https://example.com",
		data: {},
		...overrides,
	};
}

function makeMarker(overrides: Partial<MarkerRow> = {}): MarkerRow {
	return {
		id: "M1",
		session_id: "s1",
		timestamp: 1_700_000_060_000,
		label: "Form submitted",
		auto_detected: 0,
		severity: null,
		...overrides,
	};
}

function makeQueryEngine(overrides: Partial<QueryEngine> = {}): QueryEngine {
	return {
		search: () => [],
		getFullEvent: () => null,
		getSession: () => ({
			id: "s1",
			started_at: 1_700_000_000_000,
			ended_at: null,
			tab_url: "https://example.com",
			tab_title: "Test",
			event_count: 10,
			marker_count: 1,
			error_count: 0,
			recording_dir: "/tmp/s1",
		}),
		getMarkers: () => [],
		getNetworkBody: () => undefined,
		readNetworkBody: () => undefined,
		listSessions: () => [],
		getOverview: () => ({ session: { id: "s1", startedAt: 0, url: "", title: "" }, markers: [], timeline: [], networkSummary: null, errorSummary: null, frameworkSummary: null }),
		inspect: () => {
			throw new Error("not implemented");
		},
		...overrides,
	} as unknown as QueryEngine;
}

const SESSION_ID = "s1";

describe("ReplayContextGenerator", () => {
	describe("summary format", () => {
		it("includes navigation, errors, and user actions", () => {
			const events: EventRow[] = [
				makeEventRow({ event_id: "e1", type: "navigation", summary: "Navigated to https://example.com/form" }),
				makeEventRow({ event_id: "e2", type: "user_input", summary: "Input change on #email" }),
				makeEventRow({ event_id: "e3", type: "page_error", summary: "TypeError: null.foo" }),
			];

			const qe = makeQueryEngine({
				search: () => events,
			});
			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "summary" });

			expect(text).toContain("Navigation Path");
			expect(text).toContain("https://example.com/form");
			expect(text).toContain("Errors");
			expect(text).toContain("TypeError: null.foo");
			expect(text).toContain("User Actions");
		});

		it("omits sections that have no events", () => {
			const qe = makeQueryEngine({ search: () => [] });
			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "summary" });

			expect(text).not.toContain("Navigation Path");
			expect(text).not.toContain("Errors");
		});
	});

	describe("reproduction_steps format", () => {
		it("generates numbered steps from navigation and user input events", () => {
			const events: EventRow[] = [
				makeEventRow({ event_id: "nav", type: "navigation", summary: "Navigated to https://example.com/checkout" }),
				makeEventRow({ event_id: "click1", type: "user_input", summary: "Click on #submit-btn" }),
				makeEventRow({ event_id: "submit1", type: "user_input", summary: "Form submitted" }),
			];

			const qe = makeQueryEngine({
				search: () => events,
				getFullEvent: (_, id) => {
					if (id === "click1") return makeFullEvent({ data: { type: "click", selector: "#submit-btn", text: "Submit" } });
					if (id === "submit1") return makeFullEvent({ data: { type: "submit", selector: "#checkout-form", fields: { name: "Alice", email: "alice@test.com" } } });
					return null;
				},
			});

			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "reproduction_steps" });

			expect(text).toContain("1. Navigate to https://example.com/checkout");
			expect(text).toContain("Click #submit-btn");
			expect(text).toContain("Submit form #checkout-form");
			expect(text).toContain("alice@test.com");
		});

		it("includes expected/actual when network errors present", () => {
			const events: EventRow[] = [
				makeEventRow({ event_id: "nav", type: "navigation", summary: "Navigated to https://example.com" }),
				makeEventRow({ event_id: "err", type: "network_response", summary: "422 POST /api/submit" }),
			];

			const qe = makeQueryEngine({ search: () => events });
			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "reproduction_steps" });

			expect(text).toContain("Expected");
			expect(text).toContain("Actual");
			expect(text).toContain("422");
		});
	});

	describe("test_scaffold format", () => {
		const events: EventRow[] = [
			makeEventRow({ event_id: "nav", type: "navigation", summary: "Navigated to https://example.com/login" }),
			makeEventRow({ event_id: "fill1", type: "user_input" }),
			makeEventRow({ event_id: "submit1", type: "user_input" }),
		];

		const qe = makeQueryEngine({
			search: () => events,
			getFullEvent: (_, id) => {
				if (id === "fill1") return makeFullEvent({ data: { type: "change", selector: "#email", value: "user@example.com" } });
				if (id === "submit1")
					return makeFullEvent({
						data: {
							type: "submit",
							selector: "#login-form",
							fields: { email: "user@example.com", password: "[MASKED]", remember: "true" },
						},
					});
				return null;
			},
		});

		it("generates syntactically valid Playwright test", () => {
			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "test_scaffold", testFramework: "playwright" });

			expect(text).toContain("import { test, expect } from '@playwright/test'");
			expect(text).toContain("test(");
			expect(text).toContain("async ({ page }) => {");
			expect(text).toContain("await page.goto('https://example.com/login')");
			expect(text).toContain("await page.fill('#email'");
			// Password fields should be excluded
			expect(text).not.toContain("[MASKED]");
		});

		it("generates syntactically valid Cypress test", () => {
			const gen = new ReplayContextGenerator(qe);
			const text = gen.generate({ sessionId: SESSION_ID, format: "test_scaffold", testFramework: "cypress" });

			expect(text).toContain("describe(");
			expect(text).toContain("it(");
			expect(text).toContain("cy.visit('https://example.com/login')");
			expect(text).toContain("cy.get('#email')");
			// Password fields should be excluded
			expect(text).not.toContain("[MASKED]");
		});

		it("excludes masked (password) fields from test scaffold", () => {
			const gen = new ReplayContextGenerator(qe);
			const playwrightText = gen.generate({ sessionId: SESSION_ID, format: "test_scaffold", testFramework: "playwright" });
			const cypressText = gen.generate({ sessionId: SESSION_ID, format: "test_scaffold", testFramework: "cypress" });

			expect(playwrightText).not.toContain("[MASKED]");
			expect(cypressText).not.toContain("[MASKED]");
		});
	});

	describe("getRelevantEvents", () => {
		it("uses aroundMarker time range when provided", () => {
			const markerTs = 1_700_000_060_000;
			const marker = makeMarker({ id: "M1", timestamp: markerTs });
			let capturedTimeRange: { start: number; end: number } | undefined;

			const qe = makeQueryEngine({
				getMarkers: () => [marker],
				search: (_, params) => {
					capturedTimeRange = params.filters?.timeRange;
					return [];
				},
			});

			const gen = new ReplayContextGenerator(qe);
			gen.getRelevantEvents({ sessionId: SESSION_ID, format: "summary", aroundMarker: "M1" });

			expect(capturedTimeRange?.start).toBe(markerTs - 120_000);
			expect(capturedTimeRange?.end).toBe(markerTs + 30_000);
		});

		it("throws when aroundMarker not found", () => {
			const qe = makeQueryEngine({ getMarkers: () => [] });
			const gen = new ReplayContextGenerator(qe);
			expect(() => gen.getRelevantEvents({ sessionId: SESSION_ID, format: "summary", aroundMarker: "nonexistent" })).toThrow();
		});
	});
});
