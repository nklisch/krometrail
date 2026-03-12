import { describe, expect, it } from "vitest";
import { ReactObserver } from "../../../src/browser/recorder/framework/react-observer.js";

describe("ReactObserver", () => {
	describe("constructor", () => {
		it("uses default config when no args provided", () => {
			const observer = new ReactObserver();
			const script = observer.getInjectionScript();
			// Defaults: maxEventsPerSecond=10, maxSerializationDepth=3, etc.
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 10");
			expect(script).toContain("MAX_DEPTH = 3");
			expect(script).toContain("MAX_FIBERS_PER_COMMIT = 5000");
			expect(script).toContain("MAX_QUEUE_SIZE = 1000");
		});

		it("merges partial config with defaults", () => {
			const observer = new ReactObserver({ maxEventsPerSecond: 20 });
			const script = observer.getInjectionScript();
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 20");
			// Other defaults should still be there
			expect(script).toContain("MAX_DEPTH = 3");
			expect(script).toContain("MAX_QUEUE_SIZE = 1000");
		});

		it("respects all config overrides", () => {
			const observer = new ReactObserver({
				maxEventsPerSecond: 5,
				maxSerializationDepth: 2,
				staleClosureThreshold: 10,
				infiniteRerenderThreshold: 30,
				contextRerenderThreshold: 50,
				maxFibersPerCommit: 100,
				maxQueueSize: 500,
			});
			const script = observer.getInjectionScript();
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 5");
			expect(script).toContain("MAX_DEPTH = 2");
			expect(script).toContain("MAX_FIBERS_PER_COMMIT = 100");
			expect(script).toContain("MAX_QUEUE_SIZE = 500");
		});
	});

	describe("getInjectionScript", () => {
		it("returns a non-empty string", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(typeof script).toBe("string");
			expect(script.length).toBeGreaterThan(100);
		});

		it("is a self-contained IIFE", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script.trimStart()).toMatch(/^\(function\(\)/);
			expect(script.trimEnd()).toMatch(/\}\)\(\);$/);
		});

		it("uses only var declarations (no let/const)", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).not.toMatch(/\blet\s+/);
			expect(script).not.toMatch(/\bconst\s+/);
		});

		it("contains __BL__ reporting", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("__BL__");
		});

		it("contains onCommitFiberRoot patching", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("onCommitFiberRoot");
		});

		it("contains onCommitFiberUnmount patching", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("onCommitFiberUnmount");
		});

		it("interpolates config values into the script", () => {
			const observer = new ReactObserver({ maxEventsPerSecond: 42 });
			const script = observer.getInjectionScript();
			expect(script).toContain("42");
		});

		it("contains processCommit function", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function processCommit");
		});

		it("contains processUnmount function", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function processUnmount");
		});

		it("contains serialize function", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function serialize");
		});

		it("contains getComponentName function", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function getComponentName");
		});

		it("contains getComponentPath function", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function getComponentPath");
		});

		it("contains pattern detection functions", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(script).toContain("function checkPatterns");
			expect(script).toContain("function checkInfiniteRerender");
			expect(script).toContain("function checkStaleClosures");
			expect(script).toContain("function checkMissingCleanup");
			expect(script).toContain("function checkExcessiveContextRerender");
		});

		it("has no syntax errors (new Function parse check)", () => {
			const script = new ReactObserver().getInjectionScript();
			expect(() => new Function(script)).not.toThrow();
		});
	});
});
