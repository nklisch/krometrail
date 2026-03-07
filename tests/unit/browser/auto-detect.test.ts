import { beforeEach, describe, expect, it } from "vitest";
import { AutoDetector, DEFAULT_DETECTION_RULES } from "../../../src/browser/recorder/auto-detect.js";
import type { RecordedEvent } from "../../../src/browser/types.js";

function makeEvent(type: RecordedEvent["type"], data: Record<string, unknown> = {}): RecordedEvent {
	return {
		id: crypto.randomUUID(),
		timestamp: Date.now(),
		type,
		tabId: "tab1",
		summary: "test",
		data,
	};
}

describe("AutoDetector", () => {
	let detector: AutoDetector;

	beforeEach(() => {
		// Fresh detector with clean cooldown state
		detector = new AutoDetector(DEFAULT_DETECTION_RULES);
	});

	it("triggers medium severity marker on HTTP 4xx response", () => {
		const event = makeEvent("network_response", { status: 404, method: "GET", url: "/api/resource" });
		const markers = detector.check(event, []);

		expect(markers.some((m) => m.severity === "medium")).toBe(true);
		expect(markers.some((m) => m.label.includes("HTTP 404"))).toBe(true);
	});

	it("triggers high severity marker on HTTP 5xx response", () => {
		const event = makeEvent("network_response", { status: 500, method: "POST", url: "/api/submit" });
		const markers = detector.check(event, []);

		expect(markers.some((m) => m.severity === "high")).toBe(true);
		expect(markers.some((m) => m.label.includes("Server error"))).toBe(true);
	});

	it("triggers medium severity marker on console error", () => {
		const event = makeEvent("console", { level: "error" });
		event.summary = "[error] TypeError: Cannot read property 'x'";
		const markers = detector.check(event, []);

		expect(markers.some((m) => m.severity === "medium")).toBe(true);
		expect(markers.some((m) => m.label.includes("Console error"))).toBe(true);
	});

	it("does not trigger on console.log", () => {
		const event = makeEvent("console", { level: "log" });
		const markers = detector.check(event, []);

		expect(markers.filter((m) => m.label.includes("Console"))).toHaveLength(0);
	});

	it("triggers high severity marker on unhandled exception", () => {
		const event = makeEvent("page_error", {});
		event.summary = "Uncaught TypeError: undefined is not a function";
		const markers = detector.check(event, []);

		expect(markers.some((m) => m.severity === "high")).toBe(true);
		expect(markers.some((m) => m.label.includes("Uncaught"))).toBe(true);
	});

	it("triggers low severity marker on slow response (> 5s)", () => {
		const event = makeEvent("network_response", {
			status: 200,
			method: "GET",
			url: "/api/slow",
			durationMs: 8000,
		});
		const markers = detector.check(event, []);

		expect(markers.some((m) => m.severity === "low")).toBe(true);
		expect(markers.some((m) => m.label.includes("Slow response"))).toBe(true);
	});

	it("does not trigger for fast response", () => {
		const event = makeEvent("network_response", {
			status: 200,
			method: "GET",
			url: "/api/fast",
			durationMs: 100,
		});
		const markers = detector.check(event, []);

		expect(markers.filter((m) => m.label.includes("Slow"))).toHaveLength(0);
	});

	it("respects cooldown: does not fire the same rule twice in window", () => {
		const event1 = makeEvent("console", { level: "error" });
		event1.summary = "[error] First error";
		const markers1 = detector.check(event1, []);
		expect(markers1.some((m) => m.label.includes("Console error"))).toBe(true);

		// Immediately fire the same type of event — should be suppressed by cooldown
		const event2 = makeEvent("console", { level: "error" });
		event2.summary = "[error] Second error";
		const markers2 = detector.check(event2, []);
		expect(markers2.filter((m) => m.label.includes("Console error"))).toHaveLength(0);
	});

	it("fires multiple rules on same event when applicable", () => {
		// A 500 response that's also slow should trigger both server error AND slow response rules
		const event = makeEvent("network_response", {
			status: 500,
			method: "GET",
			url: "/api/bad",
			durationMs: 7000,
		});
		const markers = detector.check(event, []);

		// Should have at least the server error rule fire
		expect(markers.some((m) => m.severity === "high")).toBe(true);
		// Slow response also fires (different rule, different cooldown)
		expect(markers.some((m) => m.severity === "low")).toBe(true);
	});

	it("does not trigger on 2xx/3xx responses", () => {
		const event200 = makeEvent("network_response", { status: 200, method: "GET", url: "/api" });
		expect(detector.check(event200, [])).toHaveLength(0);

		const event301 = makeEvent("network_response", { status: 301, method: "GET", url: "/api" });
		expect(detector.check(event301, [])).toHaveLength(0);
	});

	it("handles navigation events without triggering rules", () => {
		const event = makeEvent("navigation", { url: "https://example.com" });
		const markers = detector.check(event, []);
		expect(markers).toHaveLength(0);
	});
});
