import { describe, expect, it } from "vitest";
import { VueObserver } from "../../../src/browser/recorder/framework/vue-observer.js";

describe("VueObserver", () => {
	describe("constructor", () => {
		it("uses default config when no args provided", () => {
			const observer = new VueObserver();
			const script = observer.getInjectionScript();
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 10");
			expect(script).toContain("MAX_DEPTH = 3");
			expect(script).toContain("MAX_COMPONENTS_PER_BATCH = 5000");
			expect(script).toContain("MAX_QUEUE_SIZE = 1000");
			expect(script).toContain("STORE_OBSERVATION = true");
			expect(script).toContain("STORE_DISCOVERY_INTERVAL_MS = 5000");
		});

		it("merges partial config with defaults", () => {
			const observer = new VueObserver({ maxEventsPerSecond: 20 });
			const script = observer.getInjectionScript();
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 20");
			// Other defaults should still be there
			expect(script).toContain("MAX_DEPTH = 3");
			expect(script).toContain("MAX_QUEUE_SIZE = 1000");
		});

		it("respects all config overrides", () => {
			const observer = new VueObserver({
				maxEventsPerSecond: 5,
				maxSerializationDepth: 2,
				infiniteLoopThreshold: 50,
				maxComponentsPerBatch: 100,
				maxQueueSize: 500,
				storeObservation: false,
				storeDiscoveryIntervalMs: 10000,
			});
			const script = observer.getInjectionScript();
			expect(script).toContain("MAX_EVENTS_PER_SECOND = 5");
			expect(script).toContain("MAX_DEPTH = 2");
			expect(script).toContain("MAX_COMPONENTS_PER_BATCH = 100");
			expect(script).toContain("MAX_QUEUE_SIZE = 500");
			expect(script).toContain("STORE_OBSERVATION = false");
			expect(script).toContain("STORE_DISCOVERY_INTERVAL_MS = 10000");
		});
	});

	describe("getInjectionScript", () => {
		it("returns a non-empty string", () => {
			const script = new VueObserver().getInjectionScript();
			expect(typeof script).toBe("string");
			expect(script.length).toBeGreaterThan(100);
		});

		it("is a self-contained IIFE", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script.trimStart()).toMatch(/^\(function\(\)/);
			expect(script.trimEnd()).toMatch(/\}\)\(\);$/);
		});

		it("uses only var declarations (no let/const)", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).not.toMatch(/\blet\s+/);
			expect(script).not.toMatch(/\bconst\s+/);
		});

		it("contains __BL__ reporting", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("__BL__");
		});

		it("contains component:added listener registration", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("component:added");
		});

		it("contains component:updated listener registration", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("component:updated");
		});

		it("contains component:removed listener registration", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("component:removed");
		});

		it("contains app:init handler", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("app:init");
		});

		it("contains buffer drain logic", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("_buffer");
			expect(script).toContain("buffered");
		});

		it("interpolates config values into the script", () => {
			const observer = new VueObserver({ maxEventsPerSecond: 42 });
			const script = observer.getInjectionScript();
			expect(script).toContain("42");
		});

		it("contains extractState function", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("function extractState");
		});

		it("contains diffState function", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("function diffState");
		});

		it("contains getComponentName function", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("function getComponentName");
		});

		it("contains getComponentPath function", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("function getComponentPath");
		});

		it("contains pattern detection functions", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("function checkPatterns");
			expect(script).toContain("function checkInfiniteLoop");
			expect(script).toContain("function checkLostReactivity");
			expect(script).toContain("function checkPiniaMutationOutsideAction");
		});

		it("contains store observation (Pinia detection)", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("observePiniaStore");
			expect(script).toContain("pinia");
		});

		it("contains store observation (Vuex detection)", () => {
			const script = new VueObserver().getInjectionScript();
			expect(script).toContain("observeVuexStore");
			expect(script).toContain("$store");
		});

		it("has no syntax errors (new Function parse check)", () => {
			const script = new VueObserver().getInjectionScript();
			expect(() => new Function(script)).not.toThrow();
		});
	});
});
