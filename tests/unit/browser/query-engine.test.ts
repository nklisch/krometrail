import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import { BrowserDatabase } from "../../../src/browser/storage/database.js";
import { EventWriter } from "../../../src/browser/storage/event-writer.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

let db: BrowserDatabase;
let engine: QueryEngine;
let tmpDir: string;
let recordingDir: string;
let writer: EventWriter;

const SESSION_ID = "sess-test-1";
const SESSION_ID_2 = "sess-test-2";
const BASE_TS = 1709826622000;

function makeEvent(id: string, type: string, summary: string, ts: number, data: Record<string, unknown> = {}): RecordedEvent {
	return {
		id,
		timestamp: ts,
		type: type as RecordedEvent["type"],
		tabId: "tab1",
		summary,
		data,
	};
}

beforeEach(() => {
	tmpDir = resolve(tmpdir(), "agent-lens-qe-test-" + crypto.randomUUID());
	recordingDir = resolve(tmpDir, "recordings", SESSION_ID);
	mkdirSync(resolve(recordingDir, "network"), { recursive: true });
	mkdirSync(resolve(recordingDir, "screenshots"), { recursive: true });

	db = new BrowserDatabase(resolve(tmpDir, "index.db"));
	engine = new QueryEngine(db, tmpDir);

	// Create sessions
	db.createSession({ id: SESSION_ID, startedAt: BASE_TS, tabUrl: "https://example.com/app", tabTitle: "Example App", recordingDir });
	db.createSession({ id: SESSION_ID_2, startedAt: BASE_TS + 3600_000, tabUrl: "https://other.com", tabTitle: "Other", recordingDir: resolve(tmpDir, "recordings", SESSION_ID_2) });

	// Write events to JSONL and index in DB
	writer = new EventWriter(resolve(recordingDir, "events.jsonl"));

	const events: RecordedEvent[] = [
		makeEvent("evt-nav-1", "navigation", "Navigated to /app", BASE_TS + 1000),
		makeEvent("evt-net-1", "network_response", "200 GET /api/data (50ms)", BASE_TS + 2000, { method: "GET", url: "/api/data", status: 200 }),
		makeEvent("evt-net-2", "network_response", "422 POST /api/submit (30ms)", BASE_TS + 3000, { method: "POST", url: "/api/submit", status: 422 }),
		makeEvent("evt-console-1", "console", "[log] User clicked submit", BASE_TS + 4000),
		makeEvent("evt-error-1", "page_error", "TypeError: Cannot read property", BASE_TS + 5000, { stackTrace: "at app.js:42" }),
		makeEvent("evt-console-err", "console", "[error] Failed to fetch", BASE_TS + 6000),
		makeEvent("evt-nav-2", "navigation", "Navigated to /checkout", BASE_TS + 7000),
	];

	for (const e of events) {
		const { offset, length } = writer.write(e);
		db.insertEvent({ sessionId: SESSION_ID, eventId: e.id, timestamp: e.timestamp, type: e.type, summary: e.summary, detailOffset: offset, detailLength: length });
	}

	// Markers
	db.insertMarker({ id: "marker-1", sessionId: SESSION_ID, timestamp: BASE_TS + 3500, label: "Validation error", autoDetected: false });
	db.insertMarker({ id: "marker-2", sessionId: SESSION_ID, timestamp: BASE_TS + 5500, label: "Page error detected", autoDetected: true, severity: "high" });

	// Network body for 422 event
	const bodyFile = "422-response.json";
	writeFileSync(resolve(recordingDir, "network", bodyFile), JSON.stringify({ error: "validation failed", field: "phone" }));
	db.insertNetworkBody({ eventId: "evt-net-2", sessionId: SESSION_ID, responseBodyPath: bodyFile, responseSize: 50, contentType: "application/json" });

	// Screenshot
	writeFileSync(resolve(recordingDir, "screenshots", `${BASE_TS + 3000}.jpg`), "fake-jpg-data");

	db.updateSessionCounts(SESSION_ID);
	db.updateSessionCounts(SESSION_ID_2);

	writer.close();
});

afterEach(() => {
	db.close();
});

describe("QueryEngine.listSessions", () => {
	it("returns all sessions when no filter", () => {
		const sessions = engine.listSessions();
		expect(sessions).toHaveLength(2);
	});

	it("filters by urlContains", () => {
		const sessions = engine.listSessions({ urlContains: "example.com" });
		expect(sessions).toHaveLength(1);
		expect(sessions[0].id).toBe(SESSION_ID);
	});

	it("filters by hasMarkers", () => {
		const sessions = engine.listSessions({ hasMarkers: true });
		expect(sessions).toHaveLength(1);
		expect(sessions[0].id).toBe(SESSION_ID);
	});

	it("filters by hasErrors", () => {
		const sessions = engine.listSessions({ hasErrors: true });
		expect(sessions).toHaveLength(1);
		expect(sessions[0].id).toBe(SESSION_ID);
	});

	it("returns SessionSummary shape", () => {
		const sessions = engine.listSessions({ limit: 1 });
		expect(sessions[0]).toMatchObject({
			id: expect.any(String),
			startedAt: expect.any(Number),
			duration: expect.any(Number),
			url: expect.any(String),
			title: expect.any(String),
			eventCount: expect.any(Number),
			markerCount: expect.any(Number),
			errorCount: expect.any(Number),
		});
	});

	it("respects limit", () => {
		const sessions = engine.listSessions({ limit: 1 });
		expect(sessions).toHaveLength(1);
	});

	it("filters by after timestamp", () => {
		const sessions = engine.listSessions({ after: BASE_TS + 1000 });
		expect(sessions).toHaveLength(1);
		expect(sessions[0].id).toBe(SESSION_ID_2);
	});
});

describe("QueryEngine.getOverview", () => {
	it("returns session overview with all sections by default", () => {
		const overview = engine.getOverview(SESSION_ID);
		expect(overview.session.id).toBe(SESSION_ID);
		expect(overview.markers).toHaveLength(2);
		expect(overview.timeline.length).toBeGreaterThan(0);
		expect(overview.networkSummary).not.toBeNull();
		expect(overview.errorSummary).not.toBeNull();
	});

	it("includes navigation events in timeline", () => {
		const overview = engine.getOverview(SESSION_ID);
		const types = overview.timeline.map((e) => e.type);
		expect(types).toContain("navigation");
	});

	it("summarizes network requests", () => {
		const overview = engine.getOverview(SESSION_ID);
		expect(overview.networkSummary!.total).toBe(2);
		expect(overview.networkSummary!.succeeded).toBe(1);
		expect(overview.networkSummary!.failed).toBe(1);
		expect(overview.networkSummary!.notable).toHaveLength(1);
	});

	it("identifies error events", () => {
		const overview = engine.getOverview(SESSION_ID);
		const errorTypes = overview.errorSummary!.map((e) => e.type);
		expect(errorTypes).toContain("page_error");
	});

	it("focuses timeline around a marker", () => {
		const overview = engine.getOverview(SESSION_ID, { aroundMarker: "marker-1" });
		// Timeline should only contain events within ±60s of marker-1 (BASE_TS+3500)
		for (const e of overview.timeline) {
			expect(Math.abs(e.timestamp - (BASE_TS + 3500))).toBeLessThanOrEqual(60_000);
		}
	});

	it("respects include filter — timeline only", () => {
		const overview = engine.getOverview(SESSION_ID, { include: ["timeline"] });
		expect(overview.timeline.length).toBeGreaterThan(0);
		expect(overview.networkSummary).toBeNull();
		expect(overview.errorSummary).toBeNull();
	});

	it("throws for unknown session", () => {
		expect(() => engine.getOverview("no-such-session")).toThrow("Session not found");
	});
});

describe("QueryEngine.search", () => {
	it("finds events by FTS query", () => {
		const results = engine.search(SESSION_ID, { query: "submit" });
		expect(results.length).toBeGreaterThan(0);
	});

	it("filters by event type", () => {
		const results = engine.search(SESSION_ID, { filters: { eventTypes: ["page_error"] } });
		expect(results).toHaveLength(1);
		expect(results[0].type).toBe("page_error");
	});

	it("filters by status code", () => {
		const results = engine.search(SESSION_ID, { filters: { statusCodes: [422] } });
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-net-2");
	});

	it("filters by time range", () => {
		const results = engine.search(SESSION_ID, {
			filters: { timeRange: { start: BASE_TS + 1000, end: BASE_TS + 2500 } },
		});
		expect(results.length).toBeGreaterThan(0);
		for (const r of results) {
			expect(r.timestamp).toBeGreaterThanOrEqual(BASE_TS + 1000);
			expect(r.timestamp).toBeLessThanOrEqual(BASE_TS + 2500);
		}
	});

	it("respects maxResults", () => {
		const results = engine.search(SESSION_ID, { maxResults: 2 });
		expect(results.length).toBeLessThanOrEqual(2);
	});
});

describe("QueryEngine.inspect", () => {
	it("inspects by event_id and reads from JSONL", () => {
		const result = engine.inspect(SESSION_ID, { eventId: "evt-nav-1" });
		expect(result.event.id).toBe("evt-nav-1");
		expect(result.event.type).toBe("navigation");
	});

	it("inspects by marker_id", () => {
		const result = engine.inspect(SESSION_ID, { markerId: "marker-1" });
		// Should find the event closest to the marker timestamp (BASE_TS+3500)
		expect(result.event).toBeDefined();
	});

	it("inspects by timestamp", () => {
		const result = engine.inspect(SESSION_ID, { timestamp: BASE_TS + 2000 });
		expect(result.event).toBeDefined();
	});

	it("includes surrounding events by default", () => {
		const result = engine.inspect(SESSION_ID, { eventId: "evt-console-1" });
		expect(result.surroundingEvents.length).toBeGreaterThan(0);
	});

	it("loads network response body", () => {
		const result = engine.inspect(SESSION_ID, { eventId: "evt-net-2", include: ["network_body"] });
		expect(result.networkBody).not.toBeNull();
		expect(result.networkBody!.response).toContain("validation failed");
		expect(result.networkBody!.contentType).toBe("application/json");
	});

	it("finds nearest screenshot", () => {
		const result = engine.inspect(SESSION_ID, { eventId: "evt-net-2", include: ["screenshot"] });
		expect(result.screenshot).not.toBeNull();
		expect(result.screenshot).toContain("screenshots");
	});

	it("throws for unknown event_id", () => {
		expect(() => engine.inspect(SESSION_ID, { eventId: "no-such-event" })).toThrow("Event not found");
	});

	it("throws when no locator provided", () => {
		expect(() => engine.inspect(SESSION_ID, {})).toThrow("Must provide eventId, markerId, or timestamp");
	});
});
