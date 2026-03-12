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

const SID = "sess-fw-test";
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

const frameworkEvents: RecordedEvent[] = [
	makeEvent("fw-detect", "framework_detect", "[react] React 18.2.0 detected (1 root)", BASE_TS, {
		framework: "react",
		version: "18.2.0",
		rootCount: 1,
		componentCount: 12,
	}),
	makeEvent("fw-state-1", "framework_state", "[react] UserProfile: mount (render #1)", BASE_TS + 1000, {
		framework: "react",
		componentName: "UserProfile",
		changeType: "mount",
		renderCount: 1,
	}),
	makeEvent("fw-state-2", "framework_state", "[react] SearchBar: update (render #5)", BASE_TS + 2000, {
		framework: "react",
		componentName: "SearchBar",
		changeType: "update",
		renderCount: 5,
		changes: [{ key: "state[0]", prev: "hello", next: "hello w" }],
	}),
	makeEvent("fw-error-1", "framework_error", "[react:high] infinite_rerender in CartContext", BASE_TS + 3000, {
		framework: "react",
		pattern: "infinite_rerender",
		componentName: "CartContext",
		severity: "high",
		detail: "CartContext rendered 23 times in 1000ms",
		evidence: { rendersInWindow: 23, windowMs: 1000 },
	}),
	makeEvent("fw-vue-detect", "framework_detect", "[vue] Vue 3.4.21 detected (1 root)", BASE_TS + 500, {
		framework: "vue",
		version: "3.4.21",
		rootCount: 1,
		componentCount: 8,
	}),
];

beforeEach(() => {
	tmpDir = resolve(tmpdir(), "agent-lens-fw-test-" + crypto.randomUUID());
	recordingDir = resolve(tmpDir, "recordings", SID);
	mkdirSync(resolve(recordingDir, "network"), { recursive: true });

	db = new BrowserDatabase(resolve(tmpDir, "index.db"));
	engine = new QueryEngine(db, tmpDir);

	db.createSession({ id: SID, startedAt: BASE_TS, tabUrl: "https://example.com", tabTitle: "Test", recordingDir });

	const writer = new EventWriter(resolve(recordingDir, "events.jsonl"));
	for (const e of frameworkEvents) {
		const { offset, length } = writer.write(e);
		db.insertEvent({ sessionId: SID, eventId: e.id, timestamp: e.timestamp, type: e.type, summary: e.summary, detailOffset: offset, detailLength: length });
	}
	db.updateSessionCounts(SID);
	writer.close();
});

afterEach(() => {
	db.close();
});

describe("framework filters", () => {
	it("framework filter returns only matching framework events", () => {
		const results = engine.search(SID, { filters: { framework: "react" }, maxResults: 100 });
		expect(results.every((r) => r.summary.startsWith("[react]") || r.summary.startsWith("[react:"))).toBe(true);
		expect(results.length).toBe(4); // detect + state-1 + state-2 + error (not vue)
	});

	it("framework filter auto-narrows event types to framework_*", () => {
		const results = engine.search(SID, { filters: { framework: "react" }, maxResults: 100 });
		expect(results.every((r) => r.type.startsWith("framework_"))).toBe(true);
	});

	it("vue framework filter returns only vue events", () => {
		const results = engine.search(SID, { filters: { framework: "vue" }, maxResults: 100 });
		expect(results.length).toBe(1);
		expect(results[0].event_id).toBe("fw-vue-detect");
	});

	it("component filter matches component name in summary", () => {
		const results = engine.search(SID, { filters: { component: "UserProfile" }, maxResults: 100 });
		expect(results.length).toBe(1);
		expect(results[0].event_id).toBe("fw-state-1");
	});

	it("component filter only matches framework_ events", () => {
		const results = engine.search(SID, { filters: { component: "UserProfile" }, maxResults: 100 });
		for (const r of results) {
			expect(r.type.startsWith("framework_")).toBe(true);
		}
	});

	it("pattern filter matches framework_error events only", () => {
		const results = engine.search(SID, { filters: { pattern: "infinite_rerender" }, maxResults: 100 });
		expect(results.length).toBe(1);
		expect(results[0].type).toBe("framework_error");
		expect(results[0].event_id).toBe("fw-error-1");
	});

	it("pattern filter returns empty when no match", () => {
		const results = engine.search(SID, { filters: { pattern: "stale_closure" }, maxResults: 100 });
		expect(results.length).toBe(0);
	});

	it("filters combine with timeRange", () => {
		const results = engine.search(SID, {
			filters: { framework: "react", timeRange: { start: BASE_TS, end: BASE_TS + 1500 } },
			maxResults: 100,
		});
		// Only fw-detect (BASE_TS) and fw-state-1 (BASE_TS+1000) are in range; fw-vue-detect (BASE_TS+500) is filtered by framework
		expect(results.length).toBe(2);
		const ids = results.map((r) => r.event_id);
		expect(ids).toContain("fw-detect");
		expect(ids).toContain("fw-state-1");
	});

	it("returns all framework events when no framework filter applied", () => {
		const results = engine.search(SID, { filters: { eventTypes: ["framework_detect"] }, maxResults: 100 });
		expect(results.length).toBe(2); // react + vue
	});
});

describe("getOverview framework summary", () => {
	it("returns frameworkSummary when framework events exist", () => {
		const overview = engine.getOverview(SID);
		expect(overview.frameworkSummary).not.toBeNull();
	});

	it("frameworkSummary contains detected frameworks", () => {
		const overview = engine.getOverview(SID);
		const fs = overview.frameworkSummary!;
		expect(fs.frameworks.length).toBe(2);
		const names = fs.frameworks.map((f) => f.name);
		expect(names).toContain("react");
		expect(names).toContain("vue");
	});

	it("frameworkSummary counts state events", () => {
		const overview = engine.getOverview(SID);
		const fs = overview.frameworkSummary!;
		expect(fs.stateEventCount).toBe(2);
	});

	it("frameworkSummary counts error events by severity", () => {
		const overview = engine.getOverview(SID);
		const fs = overview.frameworkSummary!;
		expect(fs.errors.high).toBe(1);
		expect(fs.errors.medium).toBe(0);
		expect(fs.errors.low).toBe(0);
	});

	it("frameworkSummary lists top components", () => {
		const overview = engine.getOverview(SID);
		const fs = overview.frameworkSummary!;
		expect(fs.topComponents.length).toBe(2);
		const names = fs.topComponents.map((c) => c.name);
		expect(names).toContain("UserProfile");
		expect(names).toContain("SearchBar");
	});

	it("does not compute frameworkSummary when include excludes framework", () => {
		const overview = engine.getOverview(SID, { include: ["timeline"] });
		expect(overview.frameworkSummary).toBeNull();
	});

	it("computes frameworkSummary when include contains framework", () => {
		const overview = engine.getOverview(SID, { include: ["framework"] });
		expect(overview.frameworkSummary).not.toBeNull();
	});
});
