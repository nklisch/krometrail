import { beforeEach, describe, expect, it } from "vitest";
import { SessionDiffer } from "../../../src/browser/investigation/diff.js";
import type { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import type { EventRow } from "../../../src/browser/storage/database.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

const SID = "s1";
const BASE_TS = new Date("2024-01-15T10:00:00Z").getTime();

function t(offsetMs: number): string {
	return new Date(BASE_TS + offsetMs).toISOString();
}

function makeEventRow(overrides: Partial<EventRow> = {}): EventRow {
	return {
		rowid: 1,
		session_id: SID,
		event_id: "e1",
		timestamp: BASE_TS,
		type: "framework_state",
		summary: "test",
		detail_offset: 0,
		detail_length: 10,
		...overrides,
	};
}

function makeFullEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: "e1",
		timestamp: BASE_TS,
		type: "framework_state",
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
			id: SID,
			started_at: BASE_TS,
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
		getOverview: () => ({ session: { id: SID, startedAt: 0, url: "", title: "" }, markers: [], timeline: [], networkSummary: null, errorSummary: null, frameworkSummary: null }),
		inspect: () => {
			throw new Error("not implemented");
		},
		...overrides,
	} as unknown as QueryEngine;
}

describe("SessionDiffer — framework_state diff", () => {
	let differ: SessionDiffer;

	beforeEach(() => {
		differ = new SessionDiffer(makeQueryEngine());
	});

	it("excludes framework_state by default", () => {
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000) });
		expect(diff.frameworkChanges).toBeUndefined();
		expect(diff.storeMutations).toBeUndefined();
		expect(diff.frameworkErrors).toBeUndefined();
	});

	it("includes framework_state when explicitly requested", () => {
		const mountEvent = makeEventRow({ event_id: "mount-1", timestamp: BASE_TS + 1000, type: "framework_state" });
		const mountFull = makeFullEvent({
			id: "mount-1",
			timestamp: BASE_TS + 1000,
			type: "framework_state",
			data: { componentName: "Dashboard", changeType: "mount" },
		});

		const qe = makeQueryEngine({
			search: (sid, params) => {
				if (params.filters?.eventTypes?.includes("framework_state")) return [mountEvent];
				if (params.filters?.eventTypes?.includes("framework_error")) return [];
				return [];
			},
			getFullEvent: (_sid, id) => (id === "mount-1" ? mountFull : null),
		});

		differ = new SessionDiffer(qe);
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });
		expect(diff.frameworkChanges).toBeDefined();
		expect(diff.frameworkChanges!.length).toBe(1);
		expect(diff.frameworkChanges![0].componentName).toBe("Dashboard");
		expect(diff.frameworkChanges![0].changeType).toBe("mounted");
	});

	it("groups mount/unmount/update correctly", () => {
		const stateRows = [
			makeEventRow({ event_id: "e-mount", timestamp: BASE_TS + 500, type: "framework_state" }),
			makeEventRow({ event_id: "e-unmount", timestamp: BASE_TS + 1000, type: "framework_state" }),
			makeEventRow({ event_id: "e-update", timestamp: BASE_TS + 1500, type: "framework_state" }),
		];
		const fullEvents: Record<string, RecordedEvent> = {
			"e-mount": makeFullEvent({ id: "e-mount", timestamp: BASE_TS + 500, data: { componentName: "Dashboard", changeType: "mount" } }),
			"e-unmount": makeFullEvent({ id: "e-unmount", timestamp: BASE_TS + 1000, data: { componentName: "LoginForm", changeType: "unmount" } }),
			"e-update": makeFullEvent({ id: "e-update", timestamp: BASE_TS + 1500, data: { componentName: "Header", changeType: "update" } }),
		};

		const qe = makeQueryEngine({
			search: (sid, params) => {
				if (params.filters?.eventTypes?.includes("framework_state")) return stateRows;
				return [];
			},
			getFullEvent: (_sid, id) => fullEvents[id] ?? null,
		});

		differ = new SessionDiffer(qe);
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });

		expect(diff.frameworkChanges).toBeDefined();
		const mounted = diff.frameworkChanges!.filter((c) => c.changeType === "mounted");
		const unmounted = diff.frameworkChanges!.filter((c) => c.changeType === "unmounted");
		const updated = diff.frameworkChanges!.filter((c) => c.changeType === "updated");
		expect(mounted.length).toBe(1);
		expect(mounted[0].componentName).toBe("Dashboard");
		expect(unmounted.length).toBe(1);
		expect(unmounted[0].componentName).toBe("LoginForm");
		expect(updated.length).toBe(1);
		expect(updated[0].componentName).toBe("Header");
	});

	it("store mutations listed separately", () => {
		const storeRow = makeEventRow({ event_id: "e-store", timestamp: BASE_TS + 1000, type: "framework_state" });
		const storeFull = makeFullEvent({
			id: "e-store",
			timestamp: BASE_TS + 1000,
			data: { componentName: "cart", changeType: "store_mutation", storeId: "cart", mutationType: "direct", actionName: "addItem" },
		});

		const qe = makeQueryEngine({
			search: (sid, params) => {
				if (params.filters?.eventTypes?.includes("framework_state")) return [storeRow];
				return [];
			},
			getFullEvent: (_sid, id) => (id === "e-store" ? storeFull : null),
		});

		differ = new SessionDiffer(qe);
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });

		expect(diff.storeMutations).toBeDefined();
		expect(diff.storeMutations![0].storeId).toBe("cart");
		expect(diff.storeMutations![0].actionName).toBe("addItem");
		// Store mutations should NOT appear in frameworkChanges
		expect(diff.frameworkChanges).toBeUndefined();
	});

	it("framework errors detected between moments", () => {
		const errorRow = makeEventRow({ event_id: "e-err", timestamp: BASE_TS + 2000, type: "framework_error" });
		const errorFull = makeFullEvent({
			id: "e-err",
			timestamp: BASE_TS + 2000,
			type: "framework_error",
			data: { pattern: "stale_closure", componentName: "SearchBar", severity: "medium", detail: "Stale value in closure" },
		});

		const qe = makeQueryEngine({
			search: (sid, params) => {
				if (params.filters?.eventTypes?.includes("framework_state")) return [];
				if (params.filters?.eventTypes?.includes("framework_error")) return [errorRow];
				return [];
			},
			getFullEvent: (_sid, id) => (id === "e-err" ? errorFull : null),
		});

		differ = new SessionDiffer(qe);
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });

		expect(diff.frameworkErrors).toBeDefined();
		expect(diff.frameworkErrors![0].pattern).toBe("stale_closure");
		expect(diff.frameworkErrors![0].componentName).toBe("SearchBar");
		expect(diff.frameworkErrors![0].severity).toBe("medium");
	});

	it("multiple updates to same component collapse to latest", () => {
		const updateRows = [makeEventRow({ event_id: "u1", timestamp: BASE_TS + 500, type: "framework_state" }), makeEventRow({ event_id: "u2", timestamp: BASE_TS + 1000, type: "framework_state" })];
		const fullEvents: Record<string, RecordedEvent> = {
			u1: makeFullEvent({
				id: "u1",
				timestamp: BASE_TS + 500,
				data: { componentName: "Counter", changeType: "update", changes: [{ key: "count", prev: 0, next: 1 }] },
			}),
			u2: makeFullEvent({
				id: "u2",
				timestamp: BASE_TS + 1000,
				data: { componentName: "Counter", changeType: "update", changes: [{ key: "count", prev: 1, next: 2 }] },
			}),
		};

		const qe = makeQueryEngine({
			search: (sid, params) => {
				if (params.filters?.eventTypes?.includes("framework_state")) return updateRows;
				return [];
			},
			getFullEvent: (_sid, id) => fullEvents[id] ?? null,
		});

		differ = new SessionDiffer(qe);
		const diff = differ.diff({ sessionId: SID, before: t(0), after: t(5000), include: ["framework_state"] });

		// Only one entry for Counter
		const counterChanges = diff.frameworkChanges!.filter((c) => c.componentName === "Counter");
		expect(counterChanges.length).toBe(1);
		// Latest changes (from u2)
		expect(counterChanges[0].changes![0].next).toBe(2);
	});
});
