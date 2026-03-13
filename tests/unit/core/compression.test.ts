import { describe, expect, it } from "vitest";
import { compressionNote, computeEffectiveConfig, estimateTokens, resolveCompressionTier, shouldUseDiffMode } from "../../../src/core/compression.js";
import type { ViewportConfig } from "../../../src/core/types.js";
import { DEFAULT_COMPRESSION_TIERS } from "../../../src/core/types.js";

const baseConfig: ViewportConfig = {
	sourceContextLines: 15,
	stackDepth: 5,
	localsMaxDepth: 1,
	localsMaxItems: 20,
	stringTruncateLength: 120,
	collectionPreviewItems: 5,
};

describe("resolveCompressionTier", () => {
	it("returns tier 0 for actions 1-20", () => {
		expect(resolveCompressionTier(1)).toBe(DEFAULT_COMPRESSION_TIERS[0]);
		expect(resolveCompressionTier(20)).toBe(DEFAULT_COMPRESSION_TIERS[0]);
	});

	it("returns tier 1 for actions 21-50", () => {
		expect(resolveCompressionTier(21)).toBe(DEFAULT_COMPRESSION_TIERS[1]);
		expect(resolveCompressionTier(50)).toBe(DEFAULT_COMPRESSION_TIERS[1]);
	});

	it("returns tier 2 for actions 51-99", () => {
		expect(resolveCompressionTier(51)).toBe(DEFAULT_COMPRESSION_TIERS[2]);
		expect(resolveCompressionTier(99)).toBe(DEFAULT_COMPRESSION_TIERS[2]);
	});

	it("returns tier 3 for actions 100+", () => {
		expect(resolveCompressionTier(100)).toBe(DEFAULT_COMPRESSION_TIERS[3]);
		expect(resolveCompressionTier(200)).toBe(DEFAULT_COMPRESSION_TIERS[3]);
	});
});

describe("computeEffectiveConfig", () => {
	it("merges tier overrides into base config", () => {
		const tier = DEFAULT_COMPRESSION_TIERS[1]; // stackDepth: 3, stringTruncateLength: 80
		const effective = computeEffectiveConfig(baseConfig, tier);
		expect(effective.stackDepth).toBe(3);
		expect(effective.stringTruncateLength).toBe(80);
		expect(effective.localsMaxItems).toBe(baseConfig.localsMaxItems); // unchanged
	});

	it("does not override user-explicit fields", () => {
		const tier = DEFAULT_COMPRESSION_TIERS[1]; // wants stackDepth: 3
		const explicitFields = new Set(["stackDepth"]);
		const effective = computeEffectiveConfig(baseConfig, tier, explicitFields);
		expect(effective.stackDepth).toBe(baseConfig.stackDepth); // user set this
		expect(effective.stringTruncateLength).toBe(80); // tier override applies
	});
});

describe("shouldUseDiffMode", () => {
	it("returns false for tier 0", () => {
		expect(shouldUseDiffMode(DEFAULT_COMPRESSION_TIERS[0])).toBe(false);
	});

	it("returns true for tier 2+", () => {
		expect(shouldUseDiffMode(DEFAULT_COMPRESSION_TIERS[2])).toBe(true);
		expect(shouldUseDiffMode(DEFAULT_COMPRESSION_TIERS[3])).toBe(true);
	});

	it("returns false for tier 1 (moderate compression, no auto-diff)", () => {
		expect(shouldUseDiffMode(DEFAULT_COMPRESSION_TIERS[1])).toBe(false);
	});

	it("returns true when session diff mode is explicitly enabled", () => {
		expect(shouldUseDiffMode(DEFAULT_COMPRESSION_TIERS[0], true)).toBe(true);
	});
});

describe("compressionNote", () => {
	it("returns undefined for tier 0", () => {
		expect(compressionNote(10, 200, DEFAULT_COMPRESSION_TIERS[0])).toBeUndefined();
	});

	it("returns descriptive string for tier 1+", () => {
		const note = compressionNote(25, 200, DEFAULT_COMPRESSION_TIERS[1]);
		expect(note).toBeDefined();
		expect(note).toContain("compressed");
	});

	it("includes action count and max actions", () => {
		const note = compressionNote(35, 200, DEFAULT_COMPRESSION_TIERS[1]);
		expect(note).toContain("35");
		expect(note).toContain("200");
	});
});

describe("estimateTokens", () => {
	it("returns chars / 4 rounded up", () => {
		expect(estimateTokens("hello")).toBe(2); // 5/4 = 1.25 → ceil = 2
		expect(estimateTokens("abcdefgh")).toBe(2); // 8/4 = 2
		expect(estimateTokens("abcdefghi")).toBe(3); // 9/4 = 2.25 → ceil = 3
	});

	it("returns 0 for empty string", () => {
		expect(estimateTokens("")).toBe(0);
	});
});
