import { describe, expect, it, vi } from "vitest";
import type { QueryEngine } from "../../../src/browser/investigation/query-engine.js";
import { resolveTimestamp } from "../../../src/browser/investigation/resolve-timestamp.js";

// Session starts at 2024-03-07T21:50:22.000Z
const SESSION_START = new Date("2024-03-07T21:50:22.000Z").getTime();

function makeQueryEngine(sessionStartedAt: number, fullEvent?: { timestamp: number } | null): QueryEngine {
	return {
		getSession: vi.fn().mockReturnValue({ started_at: sessionStartedAt }),
		getFullEvent: vi.fn().mockReturnValue(fullEvent ?? null),
	} as unknown as QueryEngine;
}

describe("resolveTimestamp", () => {
	it("parses pure numeric string as epoch ms", () => {
		const qe = makeQueryEngine(0);
		const epochMs = 1704110400000;
		expect(resolveTimestamp(qe, "session-1", String(epochMs))).toBe(epochMs);
	});

	it("parses ISO timestamp to epoch ms", () => {
		const qe = makeQueryEngine(0);
		const iso = "2024-01-01T12:00:00.000Z";
		expect(resolveTimestamp(qe, "session-1", iso)).toBe(new Date(iso).getTime());
	});

	it("parses YYYY-MM-DD ISO date prefix", () => {
		const qe = makeQueryEngine(0);
		const iso = "2024-06-15T00:00:00Z";
		expect(resolveTimestamp(qe, "session-1", iso)).toBe(new Date(iso).getTime());
	});

	it("resolves wall-clock HH:mm:ss.SSS relative to session start date", () => {
		// Session starts 2024-03-07T21:50:22.000Z → "21:50:39.742" = 2024-03-07T21:50:39.742Z
		const qe = makeQueryEngine(SESSION_START);
		const expected = new Date("2024-03-07T21:50:39.742Z").getTime();
		expect(resolveTimestamp(qe, "session-1", "21:50:39.742")).toBe(expected);
	});

	it("resolves wall-clock HH:mm:ss without milliseconds", () => {
		const qe = makeQueryEngine(SESSION_START);
		const expected = new Date("2024-03-07T21:50:39.000Z").getTime();
		expect(resolveTimestamp(qe, "session-1", "21:50:39")).toBe(expected);
	});

	it("resolves wall-clock with short ms component (.7 → 700ms)", () => {
		const qe = makeQueryEngine(SESSION_START);
		const expected = new Date("2024-03-07T21:50:39.700Z").getTime();
		expect(resolveTimestamp(qe, "session-1", "21:50:39.7")).toBe(expected);
	});

	it("handles day rollover: session starts near midnight, wall-clock is next day", () => {
		// Session starts at 23:50:00 UTC on 2024-03-07
		const nearMidnight = new Date("2024-03-07T23:50:00.000Z").getTime();
		const qe = makeQueryEngine(nearMidnight);
		// "00:05:12" is before session start in same day → resolves to next day
		const expected = new Date("2024-03-08T00:05:12.000Z").getTime();
		expect(resolveTimestamp(qe, "session-1", "00:05:12")).toBe(expected);
	});

	it("resolves event_id via queryEngine lookup", () => {
		const eventTimestamp = 1704110400123;
		const qe = makeQueryEngine(0, { timestamp: eventTimestamp });
		const eventId = "a1b2c3d4-0000-0000-0000-000000000001";

		expect(resolveTimestamp(qe, "session-1", eventId)).toBe(eventTimestamp);
		expect(qe.getFullEvent).toHaveBeenCalledWith("session-1", eventId);
	});

	it("throws on unresolvable reference", () => {
		const qe = makeQueryEngine(0, null);
		expect(() => resolveTimestamp(qe, "session-1", "not-a-known-event-id")).toThrow('Cannot resolve "not-a-known-event-id"');
	});

	it("error message mentions wall-clock format", () => {
		const qe = makeQueryEngine(SESSION_START, null);
		expect(() => resolveTimestamp(qe, "session-1", "garbage-xyz")).toThrow("wall-clock");
	});
});
