import { describe, expect, it } from "vitest";
import { VUE_PATTERN_DEFAULTS, getVuePatternCode } from "../../../src/browser/recorder/framework/patterns/vue-patterns.js";
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

describe("getVuePatternCode", () => {
	it("returns a non-empty string", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(typeof code).toBe("string");
		expect(code.length).toBeGreaterThan(100);
	});

	it("uses only var declarations (no let/const)", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(code).not.toMatch(/\blet\s+/);
		expect(code).not.toMatch(/\bconst\s+/);
	});

	it("contains checkPatterns dispatcher", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(code).toContain("function checkPatterns");
		expect(code).toContain("checkInfiniteLoop");
		expect(code).toContain("checkLostReactivity");
	});

	it("contains checkInfiniteLoop with configurable threshold", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(code).toContain("function checkInfiniteLoop");
		expect(code).toContain("watcher_infinite_loop");
		expect(code).toContain("30"); // default infiniteLoopThreshold
	});

	it("interpolates infiniteLoopThreshold from config", () => {
		const code = getVuePatternCode({ ...defaultConfig, infiniteLoopThreshold: 50 });
		expect(code).toContain("var _threshold = 50");
	});

	it("contains checkLostReactivity", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(code).toContain("function checkLostReactivity");
		expect(code).toContain("lost_reactivity");
		expect(code).toContain("__v_isReactive");
		expect(code).toContain("__v_isRef");
		expect(code).toContain("__v_isReadonly");
	});

	it("contains checkPiniaMutationOutsideAction", () => {
		const code = getVuePatternCode(defaultConfig);
		expect(code).toContain("function checkPiniaMutationOutsideAction");
		expect(code).toContain("pinia_mutation_outside_action");
	});

	it("has no syntax errors when embedded in a function body", () => {
		const code = getVuePatternCode(defaultConfig);
		// Wrap in a dummy function to validate syntax
		expect(() => new Function(`function queueEvent(){} ${code}`)).not.toThrow();
	});

	it("dispatcher wraps calls in try/catch", () => {
		const code = getVuePatternCode(defaultConfig);
		// checkPatterns should use try/catch for isolation
		expect(code).toMatch(/try\s*\{[^}]*checkInfiniteLoop/);
		expect(code).toMatch(/try\s*\{[^}]*checkLostReactivity/);
	});
});

describe("VUE_PATTERN_DEFAULTS", () => {
	it("has expected default values", () => {
		expect(VUE_PATTERN_DEFAULTS.infiniteLoopThreshold).toBe(30);
		expect(VUE_PATTERN_DEFAULTS.infiniteLoopWindowMs).toBe(2000);
	});
});
