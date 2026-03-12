import { beforeEach, describe, expect, it } from "vitest";
import { SessionDiffer } from "../../../src/browser/investigation/diff.js";
import type { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import type { EventRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

function makeEventRow(overrides: Partial<EventRow> = {}): EventRow {
	return {
		rowid: 1,
		session_id: "s1",
		event_id: "e1",
		timestamp: Date.now(),
		type: "navigation",
		summary: "test",
		detail_offset: 0,
		detail_length: 10,
		...overrides,
	};
}

function makeFullEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: "e1",
		timestamp: Date.now(),
		type: "navigation",
		tabId: "tab1",
		summary: "test",
		data: {},
		...overrides,
	};
}

function makeQueryEngine(overrides: Partial<QueryEngine> = {}): QueryEngine {
	return {
		search: () => [],
		getFullEvent: () => null,
		getSession: () => ({
			id: "s1",
			started_at: new Date("2024-01-01T10:00:00Z").getTime(),
			ended_at: null,
			tab_url: "https://example.com",
			tab_title: "Test",
			event_count: 0,
			marker_count: 0,
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

describe("SessionDiffer", () => {
	const SESSION_ID = "session-1";
	const BASE_DATE = "2024-01-15";
	const SESSION_START = new Date(`${BASE_DATE}T10:00:00Z`).getTime();

	let queryEngine: QueryEngine;
	let differ: SessionDiffer;

	beforeEach(() => {
		queryEngine = makeQueryEngine({
			getSession: () => ({
				id: SESSION_ID,
				started_at: SESSION_START,
				ended_at: null,
				tab_url: "https://example.com",
				tab_title: "Test",
				event_count: 0,
				marker_count: 0,
				error_count: 0,
				recording_dir: "/tmp/session",
			}),
		});
		differ = new SessionDiffer(queryEngine);
	});

	describe("resolveTimestamp", () => {
		it("parses ISO timestamp", () => {
			const ts = differ.resolveTimestamp(SESSION_ID, "2024-01-15T10:30:00Z");
			expect(ts).toBe(new Date("2024-01-15T10:30:00Z").getTime());
		});

		it("parses HH:MM:SS relative to session date", () => {
			const ts = differ.resolveTimestamp(SESSION_ID, "14:31:45");
			expect(ts).toBe(new Date(`${BASE_DATE}T14:31:45`).getTime());
		});

		it("parses HH:MM relative to session date", () => {
			const ts = differ.resolveTimestamp(SESSION_ID, "14:31");
			expect(ts).toBe(new Date(`${BASE_DATE}T14:31`).getTime());
		});

		it("resolves event_id to timestamp", () => {
			const eventTs = Date.now();
			const qe = makeQueryEngine({
				getFullEvent: () => makeFullEvent({ timestamp: eventTs }),
				getSession: queryEngine.getSession,
			});
			const d = new SessionDiffer(qe);
			const ts = d.resolveTimestamp(SESSION_ID, "some-event-id");
			expect(ts).toBe(eventTs);
		});

		it("throws on unresolvable ref", () => {
			const qe = makeQueryEngine({
				getFullEvent: () => null,
				getSession: queryEngine.getSession,
			});
			const d = new SessionDiffer(qe);
			expect(() => d.resolveTimestamp(SESSION_ID, "bad-event-id")).toThrow();
		});
	});

	describe("diff", () => {
		it("returns basic time fields", () => {
			const before = new Date("2024-01-15T10:00:00Z").toISOString();
			const after = new Date("2024-01-15T10:05:00Z").toISOString();
			const result = differ.diff({ sessionId: SESSION_ID, before, after, include: [] });
			expect(result.durationMs).toBe(5 * 60 * 1000);
			expect(result.beforeTime).toBeLessThan(result.afterTime);
		});

		it("detects URL change when navigation events differ", () => {
			const beforeUrl = "https://example.com/page1";
			const afterUrl = "https://example.com/page2";
			const beforeTs = SESSION_START + 1000;
			const afterTs = SESSION_START + 5000;

			const qe = makeQueryEngine({
				getSession: queryEngine.getSession,
				search: (_, params) => {
					const { timeRange } = params.filters ?? {};
					if (!timeRange) return [];
					// Return different nav events based on time range
					if (timeRange.end <= afterTs && timeRange.end <= SESSION_START + 3000) {
						return [makeEventRow({ event_id: "nav1", type: "navigation", timestamp: beforeTs })];
					}
					return [makeEventRow({ event_id: "nav2", type: "navigation", timestamp: afterTs })];
				},
				getFullEvent: (_, eventId) => {
					if (eventId === "nav1") return makeFullEvent({ data: { url: beforeUrl } });
					if (eventId === "nav2") return makeFullEvent({ data: { url: afterUrl } });
					return null;
				},
			});

			const d = new SessionDiffer(qe);
			const result = d.diff({
				sessionId: SESSION_ID,
				before: new Date(beforeTs).toISOString(),
				after: new Date(afterTs).toISOString(),
				include: ["url"],
			});
			expect(result.urlChange).toBeDefined();
		});

		it("detects form state changes", () => {
			const beforeTs = SESSION_START;
			const afterTs = SESSION_START + 10000;

			const qe = makeQueryEngine({
				getSession: queryEngine.getSession,
				search: (_, params) => {
					const { timeRange, eventTypes } = params.filters ?? {};
					if (!eventTypes?.includes("user_input")) return [];
					if (timeRange && timeRange.start < beforeTs) {
						// beforeChanges — no prior state
						return [];
					}
					return [makeEventRow({ event_id: "change1", type: "user_input" })];
				},
				getFullEvent: (_, eventId) => {
					if (eventId === "change1") {
						return makeFullEvent({ data: { type: "change", selector: "#email", value: "user@test.com" } });
					}
					return null;
				},
			});

			const d = new SessionDiffer(qe);
			const result = d.diff({
				sessionId: SESSION_ID,
				before: new Date(beforeTs).toISOString(),
				after: new Date(afterTs).toISOString(),
				include: ["form_state"],
			});
			expect(result.formChanges).toBeDefined();
			expect(result.formChanges?.length).toBeGreaterThan(0);
			expect(result.formChanges?.[0].selector).toBe("#email");
			expect(result.formChanges?.[0].after).toBe("user@test.com");
		});

		it("returns new console messages in range", () => {
			const beforeTs = SESSION_START;
			const afterTs = SESSION_START + 5000;
			const consoleTs = SESSION_START + 2000;

			const qe = makeQueryEngine({
				getSession: queryEngine.getSession,
				search: (_, params) => {
					const { eventTypes } = params.filters ?? {};
					if (eventTypes?.includes("console")) {
						return [makeEventRow({ event_id: "console1", type: "console", timestamp: consoleTs, summary: "[error] Something went wrong" })];
					}
					return [];
				},
			});

			const d = new SessionDiffer(qe);
			const result = d.diff({
				sessionId: SESSION_ID,
				before: new Date(beforeTs).toISOString(),
				after: new Date(afterTs).toISOString(),
				include: ["console_new"],
			});
			expect(result.newConsoleMessages).toBeDefined();
			expect(result.newConsoleMessages?.length).toBe(1);
			expect(result.newConsoleMessages?.[0].level).toBe("error");
		});

		it("returns new network requests in range", () => {
			const beforeTs = SESSION_START;
			const afterTs = SESSION_START + 5000;

			const qe = makeQueryEngine({
				getSession: queryEngine.getSession,
				search: (_, params) => {
					const { eventTypes } = params.filters ?? {};
					if (eventTypes?.includes("network_request")) {
						return [
							makeEventRow({ event_id: "req1", type: "network_request", summary: "POST /api/submit" }),
							makeEventRow({ event_id: "res1", type: "network_response", summary: "422 POST /api/submit" }),
						];
					}
					return [];
				},
			});

			const d = new SessionDiffer(qe);
			const result = d.diff({
				sessionId: SESSION_ID,
				before: new Date(beforeTs).toISOString(),
				after: new Date(afterTs).toISOString(),
				include: ["network_new"],
			});
			expect(result.newNetworkRequests).toBeDefined();
			expect(result.newNetworkRequests?.length).toBeGreaterThan(0);
		});

		it("returns no changes when nothing happened", () => {
			const qe = makeQueryEngine({ getSession: queryEngine.getSession });
			const d = new SessionDiffer(qe);
			const result = d.diff({
				sessionId: SESSION_ID,
				before: new Date(SESSION_START).toISOString(),
				after: new Date(SESSION_START + 1000).toISOString(),
				include: ["form_state", "storage", "url", "console_new", "network_new"],
			});
			expect(result.formChanges).toBeUndefined();
			expect(result.storageChanges).toBeUndefined();
			expect(result.urlChange).toBeUndefined();
			expect(result.newConsoleMessages).toEqual([]);
			expect(result.newNetworkRequests).toEqual([]);
		});
	});
});
