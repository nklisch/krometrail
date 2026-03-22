import { mkdirSync } from "node:fs";
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

const SESSION_ID = "sess-filter-test";
const BASE_TS = 1700000000000;

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
	tmpDir = resolve(tmpdir(), `krometrail-filter-test-${crypto.randomUUID()}`);
	recordingDir = resolve(tmpDir, "recordings", SESSION_ID);
	mkdirSync(resolve(recordingDir, "network"), { recursive: true });

	db = new BrowserDatabase(resolve(tmpDir, "index.db"));
	engine = new QueryEngine(db, tmpDir);

	db.createSession({ id: SESSION_ID, startedAt: BASE_TS, tabUrl: "https://example.com", tabTitle: "Test", recordingDir });

	const writer = new EventWriter(resolve(recordingDir, "events.jsonl"));

	const events: RecordedEvent[] = [
		makeEvent("evt-nav", "navigation", "Navigated to https://example.com/app", BASE_TS + 1000),
		makeEvent("evt-api-1", "network_response", "200 GET https://example.com/api/patients (50ms)", BASE_TS + 2000),
		makeEvent("evt-api-2", "network_response", "422 POST https://example.com/api/submit (30ms)", BASE_TS + 3000),
		makeEvent("evt-console-err", "console", "[error] Failed to load resource", BASE_TS + 4000),
		makeEvent("evt-console-warn", "console", "[warn] Deprecated API used", BASE_TS + 5000),
		makeEvent("evt-console-log", "console", "[log] User logged in", BASE_TS + 6000),
		makeEvent("evt-page-err", "page_error", "TypeError: Cannot read property", BASE_TS + 7000),
		makeEvent("evt-ws", "websocket", "WS SEND: hello", BASE_TS + 8000),
	];

	for (const e of events) {
		const { offset, length } = writer.write(e);
		db.insertEvent({ sessionId: SESSION_ID, eventId: e.id, timestamp: e.timestamp, type: e.type, summary: e.summary, detailOffset: offset, detailLength: length });
	}

	db.insertMarker({ id: "marker-test", sessionId: SESSION_ID, timestamp: BASE_TS + 4000, label: "Test marker", autoDetected: false });
	db.updateSessionCounts(SESSION_ID);

	writer.close();
});

afterEach(() => {
	db.close();
});

describe("QueryEngine search filters — urlPattern", () => {
	it("filters events by glob pattern matching summary", () => {
		const results = engine.search(SESSION_ID, { filters: { urlPattern: "**/api/patients**" }, maxResults: 100 });
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-api-1");
	});

	it("** matches across path separators", () => {
		const results = engine.search(SESSION_ID, { filters: { urlPattern: "**/api/**" }, maxResults: 100 });
		expect(results).toHaveLength(2);
	});

	it("returns all events when urlPattern is absent", () => {
		const results = engine.search(SESSION_ID, { maxResults: 100 });
		expect(results.length).toBeGreaterThan(1);
	});

	it("is case-insensitive", () => {
		const results = engine.search(SESSION_ID, { filters: { urlPattern: "**/API/PATIENTS**" }, maxResults: 100 });
		expect(results).toHaveLength(1);
	});

	it("returns empty when no events match pattern", () => {
		const results = engine.search(SESSION_ID, { filters: { urlPattern: "**/no-match/**" }, maxResults: 100 });
		expect(results).toHaveLength(0);
	});
});

describe("QueryEngine search filters — consoleLevels", () => {
	it("filters console events by single level, passes through non-console events", () => {
		const results = engine.search(SESSION_ID, { filters: { eventTypes: ["console"], consoleLevels: ["error"] }, maxResults: 100 });
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-console-err");
	});

	it("filters console events by multiple levels, passes through non-console events", () => {
		const results = engine.search(SESSION_ID, { filters: { eventTypes: ["console"], consoleLevels: ["error", "warn"] }, maxResults: 100 });
		expect(results).toHaveLength(2);
		const ids = results.map((r) => r.event_id);
		expect(ids).toContain("evt-console-err");
		expect(ids).toContain("evt-console-warn");
	});

	it("passes through non-console events (consoleLevels only filters console events)", () => {
		const results = engine.search(SESSION_ID, { filters: { consoleLevels: ["error"] }, maxResults: 100 });
		// Should include the console error AND all non-console events
		const types = results.map((r) => r.type);
		expect(types).toContain("console");
		// Non-console events pass through — consoleLevels doesn't exclude them
		expect(results.length).toBeGreaterThan(1);
	});

	it("returns only non-console events when no console events at that level exist", () => {
		const results = engine.search(SESSION_ID, { filters: { eventTypes: ["console"], consoleLevels: ["debug"] }, maxResults: 100 });
		expect(results).toHaveLength(0);
	});

	it("returns page_error events alongside console errors when both types requested", () => {
		const results = engine.search(SESSION_ID, {
			filters: { eventTypes: ["console", "page_error"], consoleLevels: ["error"] },
			maxResults: 100,
		});
		const ids = results.map((r) => r.event_id);
		expect(ids).toContain("evt-console-err");
		expect(ids).toContain("evt-page-err");
		expect(results).toHaveLength(2);
	});
});

describe("QueryEngine search filters — containsText", () => {
	it("filters by case-insensitive substring match on summary", () => {
		const results = engine.search(SESSION_ID, { filters: { containsText: "FAILED" }, maxResults: 100 });
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-console-err");
	});

	it("returns all events whose summary contains the text", () => {
		const results = engine.search(SESSION_ID, { filters: { containsText: "navigated" }, maxResults: 100 });
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-nav");
	});

	it("returns empty when no events match", () => {
		const results = engine.search(SESSION_ID, { filters: { containsText: "xyzzy-no-match" }, maxResults: 100 });
		expect(results).toHaveLength(0);
	});
});

describe("QueryEngine search filters — aroundMarker", () => {
	it("resolves marker ID to time range and returns events in window", () => {
		// marker-test is at BASE_TS+4000; window is BASE_TS-116000 to BASE_TS+34000
		// All our events are within this window
		const results = engine.search(SESSION_ID, { filters: { aroundMarker: "marker-test" }, maxResults: 100 });
		expect(results.length).toBeGreaterThan(0);
		for (const r of results) {
			expect(r.timestamp).toBeGreaterThanOrEqual(BASE_TS + 4000 - 120_000);
			expect(r.timestamp).toBeLessThanOrEqual(BASE_TS + 4000 + 30_000);
		}
	});

	it("throws when marker not found", () => {
		expect(() => engine.search(SESSION_ID, { filters: { aroundMarker: "no-such-marker" } })).toThrow("Marker not found");
	});

	it("does not override an explicit timeRange", () => {
		// Provide a tight timeRange that excludes most events — aroundMarker should be ignored
		const results = engine.search(SESSION_ID, {
			filters: {
				aroundMarker: "marker-test",
				timeRange: { start: BASE_TS + 2000, end: BASE_TS + 2500 },
			},
			maxResults: 100,
		});
		// Only evt-api-1 (BASE_TS+2000) falls in this explicit range
		expect(results).toHaveLength(1);
		expect(results[0].event_id).toBe("evt-api-1");
	});
});
