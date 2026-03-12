import { describe, expect, it } from "vitest";
import { buildVueInjectionScript } from "../../../src/browser/recorder/framework/vue-injection.js";
import type { VueObserverConfig } from "../../../src/browser/recorder/framework/vue-observer.js";

const defaultConfig: Required<VueObserverConfig> = {
	maxEventsPerSecond: 10,
	maxSerializationDepth: 3,
	infiniteLoopThreshold: 30,
	maxComponentsPerBatch: 5000,
	maxQueueSize: 1000,
	storeObservation: true,
	storeDiscoveryIntervalMs: 5000,
};

describe("buildVueInjectionScript", () => {
	it("returns a non-empty string", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(typeof script).toBe("string");
		expect(script.length).toBeGreaterThan(500);
	});

	it("is a self-contained IIFE", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script.trimStart()).toMatch(/^\(function\(\)/);
		expect(script.trimEnd()).toMatch(/\}\)\(\);$/);
	});

	it("uses only var declarations (no let/const)", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).not.toMatch(/\blet\s+/);
		expect(script).not.toMatch(/\bconst\s+/);
	});

	it("contains __BL__ reporting", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("__BL__");
		expect(script).toContain("console.debug");
	});

	it("contains component lifecycle listener registration", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("component:added");
		expect(script).toContain("component:updated");
		expect(script).toContain("component:removed");
	});

	it("wraps hook.on calls for component events", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("hook.on(");
	});

	it("handles missing hook gracefully (early return)", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("if (!hook) return");
	});

	it("interpolates maxEventsPerSecond config", () => {
		const script = buildVueInjectionScript({ ...defaultConfig, maxEventsPerSecond: 25 });
		expect(script).toContain("25");
	});

	it("interpolates maxSerializationDepth config", () => {
		const script = buildVueInjectionScript({ ...defaultConfig, maxSerializationDepth: 7 });
		expect(script).toContain("7");
	});

	it("interpolates pattern thresholds from config", () => {
		const script = buildVueInjectionScript({ ...defaultConfig, infiniteLoopThreshold: 60 });
		expect(script).toContain("60");
	});

	it("interpolates maxComponentsPerBatch", () => {
		const script = buildVueInjectionScript({ ...defaultConfig, maxComponentsPerBatch: 999 });
		expect(script).toContain("999");
	});

	it("interpolates maxQueueSize", () => {
		const script = buildVueInjectionScript({ ...defaultConfig, maxQueueSize: 2000 });
		expect(script).toContain("2000");
	});

	it("generated script has no syntax errors (new Function parse check)", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(() => new Function(script)).not.toThrow();
	});

	it("contains extractState function", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function extractState");
	});

	it("contains diffState function", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function diffState");
	});

	it("contains serialize function", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function serialize");
	});

	it("contains getComponentName function", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function getComponentName");
	});

	it("contains getComponentPath function", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function getComponentPath");
	});

	it("contains pattern detection functions", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("function checkPatterns");
		expect(script).toContain("function checkInfiniteLoop");
		expect(script).toContain("function checkLostReactivity");
		expect(script).toContain("function checkPiniaMutationOutsideAction");
	});

	it("contains coalescing in queueEvent", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("queueEvent");
		expect(script).toContain("scanLimit");
	});

	it("contains MAX_QUEUE_SIZE overflow protection", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("MAX_QUEUE_SIZE");
		expect(script).toContain("observer_overflow");
	});

	it("contains Pinia store detection", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("observePiniaStore");
		expect(script).toContain("getOwnPropertySymbols");
	});

	it("contains Vuex store detection", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("observeVuexStore");
		expect(script).toContain("$store");
	});

	it("contains buffer drain for _buffer", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("_buffer");
		expect(script).toContain("buffered");
	});

	it("contains store discovery interval setup", () => {
		const script = buildVueInjectionScript(defaultConfig);
		expect(script).toContain("setInterval");
		expect(script).toContain("STORE_DISCOVERY_INTERVAL_MS");
	});
});
