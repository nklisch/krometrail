import { describe, expect, it } from "vitest";
import { HARExporter } from "../../../src/browser/export/har.js";
import type { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import type { EventRow, NetworkBodyRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

function makeEventRow(overrides: Partial<EventRow> = {}): EventRow {
	return {
		rowid: 1,
		session_id: "s1",
		event_id: "e1",
		timestamp: 1_700_000_000_000,
		type: "network_request",
		summary: "GET https://api.example.com/data",
		detail_offset: 0,
		detail_length: 10,
		...overrides,
	};
}

function makeFullEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: "e1",
		timestamp: 1_700_000_000_000,
		type: "network_request",
		tabId: "tab1",
		summary: "GET https://api.example.com/data",
		data: {
			method: "GET",
			url: "https://api.example.com/data",
			headers: [{ name: "Accept", value: "application/json" }],
		},
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
			tab_title: "My App",
			event_count: 10,
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

const SESSION_ID = "s1";

describe("HARExporter", () => {
	it("produces valid HAR 1.2 structure", () => {
		const qe = makeQueryEngine();
		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID });

		expect(har).toHaveProperty("log");
		expect(har.log.version).toBe("1.2");
		expect(har.log.creator.name).toBe("Agent Lens Browser");
		expect(Array.isArray(har.log.pages)).toBe(true);
		expect(Array.isArray(har.log.entries)).toBe(true);
	});

	it("includes session metadata in pages", () => {
		const qe = makeQueryEngine();
		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID });

		expect(har.log.pages.length).toBe(1);
		expect(har.log.pages[0].id).toBe(SESSION_ID);
		expect(har.log.pages[0].title).toBe("My App");
		expect(har.log.pages[0].startedDateTime).toBeDefined();
	});

	it("correlates request and response correctly", () => {
		const reqTs = 1_700_000_010_000;
		const resTs = 1_700_000_010_500;

		const reqRow = makeEventRow({ event_id: "req1", type: "network_request", timestamp: reqTs });
		const resRow = makeEventRow({ event_id: "res1", type: "network_response", timestamp: resTs, summary: "200 GET /data" });

		const qe = makeQueryEngine({
			search: (_, params) => {
				const { eventTypes } = params.filters ?? {};
				if (eventTypes?.includes("network_request")) return [reqRow];
				if (eventTypes?.includes("network_response")) return [resRow];
				return [];
			},
			getFullEvent: (_, id) => {
				if (id === "req1") return makeFullEvent({ id: "req1", timestamp: reqTs, data: { method: "GET", url: "https://api.example.com/data", headers: [] } });
				if (id === "res1") return makeFullEvent({ id: "res1", timestamp: resTs, type: "network_response", data: { status: 200, statusText: "OK", headers: [], contentType: "application/json" } });
				return null;
			},
		});

		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID });

		expect(har.log.entries.length).toBe(1);
		const entry = har.log.entries[0];
		expect(entry.request.method).toBe("GET");
		expect(entry.request.url).toBe("https://api.example.com/data");
		expect(entry.response.status).toBe(200);
		expect(entry.time).toBe(resTs - reqTs);
	});

	it("produces valid 'no response' entry when response is missing", () => {
		const reqRow = makeEventRow({ event_id: "req1", type: "network_request" });

		const qe = makeQueryEngine({
			search: (_, params) => {
				const { eventTypes } = params.filters ?? {};
				if (eventTypes?.includes("network_request")) return [reqRow];
				return []; // no response found
			},
			getFullEvent: (_, id) => {
				if (id === "req1") return makeFullEvent({ id: "req1", data: { method: "POST", url: "https://api.example.com/submit", headers: [] } });
				return null;
			},
		});

		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID });

		expect(har.log.entries.length).toBe(1);
		const entry = har.log.entries[0];
		expect(entry.response.status).toBe(0);
		expect(entry.response.statusText).toBe("No response");
		expect(entry.timings.wait).toBe(0);
	});

	it("includes response bodies when available", () => {
		const reqRow = makeEventRow({ event_id: "req1", type: "network_request" });
		const resRow = makeEventRow({ event_id: "res1", type: "network_response" });
		const bodyContent = '{"result": "ok"}';
		const bodyRef: NetworkBodyRow = {
			event_id: "res1",
			session_id: SESSION_ID,
			request_body_path: null,
			response_body_path: "res1_response.body",
			response_size: bodyContent.length,
			content_type: "application/json",
		};

		const qe = makeQueryEngine({
			search: (_, params) => {
				const { eventTypes } = params.filters ?? {};
				if (eventTypes?.includes("network_request")) return [reqRow];
				if (eventTypes?.includes("network_response")) return [resRow];
				return [];
			},
			getFullEvent: (_, id) => {
				if (id === "req1") return makeFullEvent({ id: "req1", data: { method: "GET", url: "/api", headers: [] } });
				if (id === "res1") return makeFullEvent({ id: "res1", type: "network_response", data: { status: 200, statusText: "OK", headers: [], contentType: "application/json" } });
				return null;
			},
			getNetworkBody: () => bodyRef,
			readNetworkBody: () => bodyContent,
		});

		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID, includeResponseBodies: true });

		expect(har.log.entries[0].response.content.text).toBe(bodyContent);
		expect(har.log.entries[0].response.content.mimeType).toBe("application/json");
	});

	it("omits response bodies when includeResponseBodies is false", () => {
		const reqRow = makeEventRow({ event_id: "req1", type: "network_request" });
		const resRow = makeEventRow({ event_id: "res1", type: "network_response" });

		const qe = makeQueryEngine({
			search: (_, params) => {
				const { eventTypes } = params.filters ?? {};
				if (eventTypes?.includes("network_request")) return [reqRow];
				if (eventTypes?.includes("network_response")) return [resRow];
				return [];
			},
			getFullEvent: (_, id) => {
				if (id === "req1") return makeFullEvent({ id: "req1", data: { method: "GET", url: "/api", headers: [] } });
				if (id === "res1") return makeFullEvent({ id: "res1", type: "network_response", data: { status: 200, statusText: "OK", headers: [] } });
				return null;
			},
			getNetworkBody: () => ({ event_id: "res1", session_id: SESSION_ID, request_body_path: null, response_body_path: "body.bin", response_size: 100, content_type: "text/plain" }),
			readNetworkBody: () => "some body content",
		});

		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID, includeResponseBodies: false });

		expect(har.log.entries[0].response.content.text).toBeUndefined();
	});

	it("includes correct HAR timings structure", () => {
		const qe = makeQueryEngine();
		const exporter = new HARExporter(qe);
		const har = exporter.export({ sessionId: SESSION_ID });

		// With empty results, entries is empty — test structure with a request
		const reqRow = makeEventRow({ event_id: "req1" });
		const qe2 = makeQueryEngine({
			search: (_, params) => {
				if (params.filters?.eventTypes?.includes("network_request")) return [reqRow];
				return [];
			},
			getFullEvent: () => makeFullEvent({ id: "req1", data: { method: "GET", url: "/api", headers: [] } }),
		});
		const har2 = new HARExporter(qe2).export({ sessionId: SESSION_ID });
		const entry = har2.log.entries[0];
		expect(entry).toHaveProperty("timings");
		expect(typeof entry.timings.send).toBe("number");
		expect(typeof entry.timings.wait).toBe("number");
		expect(typeof entry.timings.receive).toBe("number");

		// Unused to avoid lint warning
		void har;
	});
});
