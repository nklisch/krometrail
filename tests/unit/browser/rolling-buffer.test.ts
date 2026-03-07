import { beforeEach, describe, expect, it, vi } from "vitest";
import { BufferConfigSchema, RollingBuffer } from "../../../src/browser/recorder/rolling-buffer.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

function makeEvent(timestampOffset = 0): RecordedEvent {
	return {
		id: crypto.randomUUID(),
		timestamp: Date.now() + timestampOffset,
		type: "console",
		tabId: "tab1",
		summary: "test event",
		data: {},
	};
}

describe("RollingBuffer", () => {
	let buffer: RollingBuffer;

	beforeEach(() => {
		buffer = new RollingBuffer(BufferConfigSchema.parse({}));
	});

	it("stores and retrieves events by time range", () => {
		const now = Date.now();
		const e1 = { ...makeEvent(), timestamp: now - 5000 };
		const e2 = { ...makeEvent(), timestamp: now - 2000 };
		const e3 = { ...makeEvent(), timestamp: now };

		buffer.push(e1);
		buffer.push(e2);
		buffer.push(e3);

		const events = buffer.getEvents(now - 3000, now);
		expect(events).toHaveLength(2);
		expect(events.map((e) => e.id)).toContain(e2.id);
		expect(events.map((e) => e.id)).toContain(e3.id);
		expect(events.map((e) => e.id)).not.toContain(e1.id);
	});

	it("returns correct stats", () => {
		buffer.push(makeEvent(-1000));
		buffer.push(makeEvent(0));
		buffer.placeMarker("test");

		const stats = buffer.getStats();
		expect(stats.eventCount).toBe(2);
		expect(stats.markerCount).toBe(1);
		expect(stats.oldestTimestamp).toBeGreaterThan(0);
		expect(stats.newestTimestamp).toBeGreaterThanOrEqual(stats.oldestTimestamp);
	});

	it("evicts events older than maxAge", () => {
		vi.useFakeTimers();
		const t0 = Date.now();
		const shortBuffer = new RollingBuffer(BufferConfigSchema.parse({ maxAgeMs: 1000, markerPaddingMs: 100, maxEvents: 100_000 }));

		const recentEvent = { ...makeEvent(), timestamp: t0 };
		shortBuffer.push(recentEvent);

		// Advance time past maxAge
		vi.setSystemTime(t0 + 2000);

		// The old event is now stale — push a new one to trigger eviction
		shortBuffer.push({ ...makeEvent(), timestamp: t0 + 2000 });

		// recentEvent is now 2s old (> maxAgeMs of 1000ms) → evicted
		const allEvents = shortBuffer.getEvents(0, t0 + 3000);
		expect(allEvents.map((e) => e.id)).not.toContain(recentEvent.id);

		vi.useRealTimers();
	});

	it("protects events near markers from eviction", () => {
		vi.useFakeTimers();
		const t0 = Date.now();
		const shortBuffer = new RollingBuffer(BufferConfigSchema.parse({ maxAgeMs: 1000, markerPaddingMs: 5000, maxEvents: 100_000 }));

		// Push an event and place a marker at the same time
		const event = { ...makeEvent(), timestamp: t0 };
		shortBuffer.push(event);
		shortBuffer.placeMarker("test");

		// Advance time past maxAge
		vi.setSystemTime(t0 + 2000);

		// Push a new event to trigger eviction
		shortBuffer.push({ ...makeEvent(), timestamp: t0 + 2000 });

		// event is 2s old (> maxAgeMs=1000), but within ±5s of marker → protected
		const allEvents = shortBuffer.getEvents(0, t0 + 3000);
		expect(allEvents.map((e) => e.id)).toContain(event.id);

		vi.useRealTimers();
	});

	it("enforces maxEvents limit by dropping oldest first", () => {
		const smallBuffer = new RollingBuffer(BufferConfigSchema.parse({ maxAgeMs: 30 * 60 * 1000, markerPaddingMs: 120_000, maxEvents: 3 }));

		const events = [makeEvent(-4000), makeEvent(-3000), makeEvent(-2000), makeEvent(-1000)];
		for (const e of events) {
			smallBuffer.push(e);
		}

		const stats = smallBuffer.getStats();
		expect(stats.eventCount).toBe(3);

		// Oldest event should be dropped
		const allEvents = smallBuffer.getEvents(0, Date.now() + 1000);
		expect(allEvents.map((e) => e.id)).not.toContain(events[0].id);
		expect(allEvents.map((e) => e.id)).toContain(events[3].id);
	});

	it("places markers with correct fields", () => {
		const marker = buffer.placeMarker("my label", false, "high");

		expect(marker.id).toBeTruthy();
		expect(marker.label).toBe("my label");
		expect(marker.autoDetected).toBe(false);
		expect(marker.severity).toBe("high");
		expect(marker.timestamp).toBeGreaterThan(0);
	});

	it("retrieves events around a marker", () => {
		const now = Date.now();
		const shortBuffer = new RollingBuffer(BufferConfigSchema.parse({ maxAgeMs: 30 * 60 * 1000, markerPaddingMs: 5000, maxEvents: 100_000 }));

		const before = { ...makeEvent(), timestamp: now - 3000 };
		const after = { ...makeEvent(), timestamp: now + 3000 };
		const outside = { ...makeEvent(), timestamp: now - 10000 };

		shortBuffer.push(before);
		shortBuffer.push(outside);
		shortBuffer.push(after);

		const marker = shortBuffer.placeMarker();
		const around = shortBuffer.getEventsAroundMarker(marker.id);

		expect(around.map((e) => e.id)).toContain(before.id);
		expect(around.map((e) => e.id)).toContain(after.id);
		expect(around.map((e) => e.id)).not.toContain(outside.id);
	});

	it("returns empty array for unknown markerId", () => {
		expect(buffer.getEventsAroundMarker("nonexistent")).toHaveLength(0);
	});

	it("supports multiple markers, each protecting nearby events", () => {
		vi.useFakeTimers();
		const t0 = Date.now();
		const shortBuffer = new RollingBuffer(BufferConfigSchema.parse({ maxAgeMs: 1000, markerPaddingMs: 5000, maxEvents: 100_000 }));

		// Push two events and place markers near each
		const e1 = { ...makeEvent(), timestamp: t0 };
		const e2 = { ...makeEvent(), timestamp: t0 + 100 };
		shortBuffer.push(e1);
		shortBuffer.push(e2);
		shortBuffer.placeMarker("m1");
		shortBuffer.placeMarker("m2");

		// Advance time past maxAge for both events
		vi.setSystemTime(t0 + 3000);
		shortBuffer.push({ ...makeEvent(), timestamp: t0 + 3000 });

		// Both events should be protected by their respective markers
		const allEvents = shortBuffer.getEvents(0, t0 + 4000);
		expect(allEvents.map((e) => e.id)).toContain(e1.id);
		expect(allEvents.map((e) => e.id)).toContain(e2.id);

		vi.useRealTimers();
	});
});
