import { describe, expect, it } from "vitest";
import { buildReactInjectionScript } from "../../../src/browser/recorder/framework/react-injection.js";
import type { ReactObserverConfig } from "../../../src/browser/recorder/framework/react-observer.js";

const defaultConfig: Required<ReactObserverConfig> = {
	maxEventsPerSecond: 10,
	maxSerializationDepth: 3,
	staleClosureThreshold: 5,
	infiniteRerenderThreshold: 15,
	contextRerenderThreshold: 20,
	maxFibersPerCommit: 5000,
	maxQueueSize: 1000,
};

describe("buildReactInjectionScript", () => {
	it("returns a non-empty string", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(typeof script).toBe("string");
		expect(script.length).toBeGreaterThan(500);
	});

	it("is a self-contained IIFE", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script.trimStart()).toMatch(/^\(function\(\)/);
		expect(script.trimEnd()).toMatch(/\}\)\(\);$/);
	});

	it("uses only var declarations (no let/const)", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).not.toMatch(/\blet\s+/);
		expect(script).not.toMatch(/\bconst\s+/);
	});

	it("contains __BL__ reporting", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("__BL__");
		expect(script).toContain("console.debug");
	});

	it("contains onCommitFiberRoot patching", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("onCommitFiberRoot");
		expect(script).toContain("processCommit");
	});

	it("contains onCommitFiberUnmount patching", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("onCommitFiberUnmount");
		expect(script).toContain("processUnmount");
	});

	it("wraps existing hook callbacks (chaining)", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("origOnCommit");
		expect(script).toContain("origOnUnmount");
	});

	it("handles missing hook gracefully (early return)", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("if (!hook) return");
	});

	it("interpolates maxEventsPerSecond config", () => {
		const script = buildReactInjectionScript({ ...defaultConfig, maxEventsPerSecond: 25 });
		expect(script).toContain("25");
	});

	it("interpolates maxSerializationDepth config", () => {
		const script = buildReactInjectionScript({ ...defaultConfig, maxSerializationDepth: 7 });
		expect(script).toContain("7");
	});

	it("interpolates pattern thresholds from config", () => {
		const script = buildReactInjectionScript({ ...defaultConfig, infiniteRerenderThreshold: 30 });
		expect(script).toContain("30");
	});

	it("interpolates maxFibersPerCommit", () => {
		const script = buildReactInjectionScript({ ...defaultConfig, maxFibersPerCommit: 999 });
		expect(script).toContain("999");
	});

	it("interpolates maxQueueSize", () => {
		const script = buildReactInjectionScript({ ...defaultConfig, maxQueueSize: 2000 });
		expect(script).toContain("2000");
	});

	it("generated script has no syntax errors (new Function parse check)", () => {
		const script = buildReactInjectionScript(defaultConfig);
		// new Function will throw if there are syntax errors
		expect(() => new Function(script)).not.toThrow();
	});

	it("contains processCommit function", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function processCommit");
	});

	it("contains processUnmount function", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function processUnmount");
	});

	it("contains serialize function", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function serialize");
	});

	it("contains getComponentName function", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function getComponentName");
	});

	it("contains getComponentPath function", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function getComponentPath");
	});

	it("contains pattern detection functions", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("function checkPatterns");
		expect(script).toContain("function checkInfiniteRerender");
		expect(script).toContain("function checkStaleClosures");
		expect(script).toContain("function checkMissingCleanup");
		expect(script).toContain("function checkExcessiveContextRerender");
	});

	it("contains coalescing in queueEvent", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("queueEvent");
		// Coalescing should scan tail of queue
		expect(script).toContain("scanLimit");
	});

	it("contains MAX_QUEUE_SIZE overflow protection", () => {
		const script = buildReactInjectionScript(defaultConfig);
		expect(script).toContain("MAX_QUEUE_SIZE");
		expect(script).toContain("observer_overflow");
	});
});
