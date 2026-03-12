import { describe, expect, it } from "vitest";
import type { InspectResult, SessionOverview, SessionSummary } from "../../../src/browser/investigation/query-engine.js";
import { renderInspectResult, renderSearchResults, renderSessionList, renderSessionOverview } from "../../../src/browser/investigation/renderers.js";
import type { EventRow, MarkerRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

const BASE_TS = 1709826622000;

function makeSessionSummary(overrides: Partial<SessionSummary> = {}): SessionSummary {
	return {
		id: "sess-abc123",
		startedAt: BASE_TS,
		duration: 120_000,
		url: "https://example.com/app",
		title: "Example App",
		eventCount: 50,
		markerCount: 2,
		errorCount: 1,
		...overrides,
	};
}

function makeEventRow(id: string, type: string, summary: string, ts = BASE_TS): EventRow {
	return {
		rowid: 1,
		session_id: "sess-abc123",
		event_id: id,
		timestamp: ts,
		type,
		summary,
		detail_offset: 0,
		detail_length: 100,
	};
}

function makeMarkerRow(id: string, label: string | null, autoDetected = false): MarkerRow {
	return {
		id,
		session_id: "sess-abc123",
		timestamp: BASE_TS + 5000,
		label,
		auto_detected: autoDetected ? 1 : 0,
		severity: null,
	};
}

function makeOverview(overrides: Partial<SessionOverview> = {}): SessionOverview {
	return {
		session: { id: "sess-abc123", startedAt: BASE_TS, url: "https://example.com/app", title: "Example App" },
		markers: [makeMarkerRow("m1", "Form submitted", false), makeMarkerRow("m2", "Error detected", true)],
		timeline: [makeEventRow("e1", "navigation", "Navigated to /app", BASE_TS + 1000), makeEventRow("e2", "navigation", "Navigated to /checkout", BASE_TS + 10000)],
		networkSummary: { total: 10, succeeded: 8, failed: 2, notable: ["422 POST /api/submit (30ms)", "500 GET /api/health (200ms)"] },
		errorSummary: [makeEventRow("e3", "page_error", "TypeError: Cannot read property", BASE_TS + 5000)],
		frameworkSummary: null,
		...overrides,
	};
}

function makeInspectResult(overrides: Partial<InspectResult> = {}): InspectResult {
	const event: RecordedEvent = {
		id: "evt-net-2",
		timestamp: BASE_TS + 3000,
		type: "network_response",
		tabId: "tab1",
		summary: "422 POST /api/submit (30ms)",
		data: { method: "POST", url: "/api/submit", status: 422, durationMs: 30 },
	};
	return {
		event,
		surroundingEvents: [makeEventRow("e1", "console", "[log] User clicked submit", BASE_TS + 2900), makeEventRow("evt-net-2", "network_response", "422 POST /api/submit (30ms)", BASE_TS + 3000)],
		networkBody: {
			request: '{"phone": "not-a-phone"}',
			response: '{"error": "validation failed", "field": "phone"}',
			contentType: "application/json",
			size: 50,
		},
		screenshot: "/tmp/screenshots/1709826625000.jpg",
		...overrides,
	};
}

describe("renderSessionList", () => {
	it("returns 'No recorded sessions found.' for empty list", () => {
		expect(renderSessionList([])).toBe("No recorded sessions found.");
	});

	it("renders session list with all fields", () => {
		const result = renderSessionList([makeSessionSummary()]);
		expect(result).toContain("sess-abc123");
		expect(result).toContain("example.com");
		expect(result).toContain("50 events");
		expect(result).toContain("2 markers");
		expect(result).toContain("1 errors");
		expect(result).toContain("2m 0s"); // 120s duration
	});

	it("does not include markers/errors line when counts are 0", () => {
		const result = renderSessionList([makeSessionSummary({ markerCount: 0, errorCount: 0 })]);
		expect(result).not.toContain("markers");
		expect(result).not.toContain("errors");
	});

	it("renders multiple sessions", () => {
		const sessions = [makeSessionSummary({ id: "a" }), makeSessionSummary({ id: "b" })];
		const result = renderSessionList(sessions);
		expect(result).toContain("Sessions (2)");
		expect(result).toContain("  a");
		expect(result).toContain("  b");
	});
});

describe("renderSessionOverview", () => {
	it("includes header always", () => {
		const result = renderSessionOverview(makeOverview());
		expect(result).toContain("Session: sess-abc123");
		expect(result).toContain("URL: https://example.com/app");
	});

	it("renders markers section", () => {
		const result = renderSessionOverview(makeOverview());
		expect(result).toContain("Markers:");
		expect(result).toContain("[user]");
		expect(result).toContain("[auto]");
		expect(result).toContain("Form submitted");
	});

	it("renders errors section", () => {
		const result = renderSessionOverview(makeOverview());
		expect(result).toContain("Errors:");
		expect(result).toContain("page_error");
		expect(result).toContain("TypeError");
	});

	it("renders network summary", () => {
		const result = renderSessionOverview(makeOverview());
		expect(result).toContain("Network:");
		expect(result).toContain("10 requests");
		expect(result).toContain("8 succeeded");
		expect(result).toContain("2 failed");
	});

	it("renders timeline", () => {
		const result = renderSessionOverview(makeOverview());
		expect(result).toContain("Timeline:");
		expect(result).toContain("Navigated to /app");
	});

	it("drops low-priority sections when budget is tight", () => {
		// With a very tight budget, only the header should be included
		const result = renderSessionOverview(makeOverview(), 30);
		expect(result).toContain("Session:");
		// Network summary (priority 40) should be dropped
		expect(result).not.toContain("Network:");
	});

	it("handles empty markers gracefully", () => {
		const result = renderSessionOverview(makeOverview({ markers: [] }));
		expect(result).not.toContain("Markers:");
	});

	it("handles null network summary", () => {
		const result = renderSessionOverview(makeOverview({ networkSummary: null }));
		expect(result).not.toContain("Network:");
	});
});

describe("renderSearchResults", () => {
	it("returns 'No matching events found.' for empty results", () => {
		expect(renderSearchResults([])).toBe("No matching events found.");
	});

	it("renders event list", () => {
		const events = [makeEventRow("evt1", "console", "[log] Something happened")];
		const result = renderSearchResults(events);
		expect(result).toContain("Found 1 events:");
		expect(result).toContain("[console]");
		expect(result).toContain("[log] Something happened");
		expect(result).toContain("(id: evt1)");
	});

	it("truncates at token budget with 'more results' message", () => {
		// Create many events to exceed a tiny budget
		const events = Array.from({ length: 20 }, (_, i) => makeEventRow(`e${i}`, "console", `[log] Event number ${i} with some text to fill up tokens`));
		const result = renderSearchResults(events, 100);
		expect(result).toContain("more results");
	});

	it("renders all events when within budget", () => {
		const events = [makeEventRow("e1", "console", "[log] A"), makeEventRow("e2", "console", "[log] B")];
		const result = renderSearchResults(events, 10000);
		expect(result).toContain("e1");
		expect(result).toContain("e2");
	});
});

describe("renderInspectResult", () => {
	it("includes event detail", () => {
		const result = renderInspectResult(makeInspectResult());
		expect(result).toContain("Event:");
		expect(result).toContain("422 POST /api/submit");
		expect(result).toContain("Type: network_response");
		expect(result).toContain("Status: 422");
	});

	it("includes network body when present", () => {
		const result = renderInspectResult(makeInspectResult());
		expect(result).toContain("Request Body:");
		expect(result).toContain("Response Body");
		expect(result).toContain("validation failed");
		expect(result).toContain("application/json");
	});

	it("includes surrounding events", () => {
		const result = renderInspectResult(makeInspectResult());
		expect(result).toContain("Context");
		expect(result).toContain("[log] User clicked submit");
		// Current event should be marked with arrow
		expect(result).toContain("→");
	});

	it("includes screenshot reference", () => {
		const result = renderInspectResult(makeInspectResult());
		expect(result).toContain("Screenshot:");
		expect(result).toContain("screenshots");
	});

	it("drops screenshot reference when budget is tight", () => {
		// Budget of 80: event (p=100) + body (p=90) consume ~72 tokens leaving ~8, not enough for screenshot (p=20, ~12 tokens)
		const result = renderInspectResult(makeInspectResult(), 80);
		expect(result).toContain("Event:");
		expect(result).not.toContain("Screenshot:");
	});

	it("handles missing network body", () => {
		const result = renderInspectResult(makeInspectResult({ networkBody: null }));
		expect(result).not.toContain("Request Body:");
	});

	it("handles empty surrounding events", () => {
		const result = renderInspectResult(makeInspectResult({ surroundingEvents: [] }));
		expect(result).not.toContain("Context");
	});

	it("includes stack trace for page_error events", () => {
		const errEvent: RecordedEvent = {
			id: "err1",
			timestamp: BASE_TS,
			type: "page_error",
			tabId: "tab1",
			summary: "TypeError: undefined",
			data: { stackTrace: "at app.js:42" },
		};
		const result = renderInspectResult(
			makeInspectResult({
				event: errEvent,
				networkBody: null,
				surroundingEvents: [],
				screenshot: null,
			}),
		);
		expect(result).toContain("Stack: at app.js:42");
	});
});
