import { describe, expect, it } from "vitest";
import { getReactPatternCode, REACT_PATTERN_DEFAULTS } from "../../../src/browser/recorder/framework/patterns/react-patterns.js";
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

describe("getReactPatternCode", () => {
	it("returns a non-empty string", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(typeof code).toBe("string");
		expect(code.length).toBeGreaterThan(100);
	});

	it("uses only var declarations (no let/const)", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).not.toMatch(/\blet\s+/);
		expect(code).not.toMatch(/\bconst\s+/);
	});

	it("includes checkInfiniteRerender function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function checkInfiniteRerender");
	});

	it("includes checkStaleClosures function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function checkStaleClosures");
	});

	it("includes checkMissingCleanup function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function checkMissingCleanup");
	});

	it("includes checkExcessiveContextRerender function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function checkExcessiveContextRerender");
	});

	it("includes checkPatterns dispatcher function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function checkPatterns");
		expect(code).toContain("checkInfiniteRerender");
		expect(code).toContain("checkStaleClosures");
		expect(code).toContain("checkMissingCleanup");
		expect(code).toContain("checkExcessiveContextRerender");
	});

	it("includes updateDepsTracking function", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("function updateDepsTracking");
	});

	it("interpolates infiniteRerenderThreshold from config", () => {
		const code = getReactPatternCode({ ...defaultConfig, infiniteRerenderThreshold: 42 });
		expect(code).toContain("42");
	});

	it("interpolates staleClosureThreshold from config", () => {
		const code = getReactPatternCode({ ...defaultConfig, staleClosureThreshold: 99 });
		expect(code).toContain("99");
	});

	it("interpolates contextRerenderThreshold from config", () => {
		const code = getReactPatternCode({ ...defaultConfig, contextRerenderThreshold: 50 });
		expect(code).toContain("50");
	});

	it("references queueEvent for pattern reporting", () => {
		const code = getReactPatternCode(defaultConfig);
		expect(code).toContain("queueEvent");
	});

	it("pattern detection is bounded (consumer enumeration cap)", () => {
		const code = getReactPatternCode(defaultConfig);
		// The excessive context check caps at threshold + 5
		expect(code).toContain("_ctxThreshold + 5");
	});
});

describe("REACT_PATTERN_DEFAULTS", () => {
	it("has expected default values", () => {
		expect(REACT_PATTERN_DEFAULTS.infiniteRerenderThreshold).toBe(15);
		expect(REACT_PATTERN_DEFAULTS.infiniteRerenderWindowMs).toBe(1000);
		expect(REACT_PATTERN_DEFAULTS.staleClosureThreshold).toBe(5);
		expect(REACT_PATTERN_DEFAULTS.contextRerenderThreshold).toBe(20);
	});
});
