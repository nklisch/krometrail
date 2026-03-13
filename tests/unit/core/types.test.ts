import { describe, expect, it } from "vitest";
import { mapViewportConfig } from "../../../src/core/types.js";

describe("mapViewportConfig", () => {
	it("returns undefined when input is undefined", () => {
		expect(mapViewportConfig(undefined)).toBeUndefined();
	});

	it("maps all snake_case fields to camelCase", () => {
		const result = mapViewportConfig({
			source_context_lines: 10,
			stack_depth: 3,
			locals_max_depth: 2,
			locals_max_items: 15,
			string_truncate_length: 80,
			collection_preview_items: 4,
		});
		expect(result).toEqual({
			sourceContextLines: 10,
			stackDepth: 3,
			localsMaxDepth: 2,
			localsMaxItems: 15,
			stringTruncateLength: 80,
			collectionPreviewItems: 4,
		});
	});

	it("maps partial config — omitted fields become undefined", () => {
		const result = mapViewportConfig({ stack_depth: 2 });
		expect(result).toBeDefined();
		expect(result!.stackDepth).toBe(2);
		expect(result!.sourceContextLines).toBeUndefined();
		expect(result!.localsMaxDepth).toBeUndefined();
	});

	it("maps empty object to object with all undefined values", () => {
		const result = mapViewportConfig({});
		expect(result).toBeDefined();
		expect(result!.stackDepth).toBeUndefined();
	});

	it("preserves zero values (boundary)", () => {
		const result = mapViewportConfig({ stack_depth: 0, locals_max_items: 0 });
		expect(result!.stackDepth).toBe(0);
		expect(result!.localsMaxItems).toBe(0);
	});
});
