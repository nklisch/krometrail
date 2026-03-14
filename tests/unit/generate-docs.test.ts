import { describe, expect, test } from "vitest";
import { z } from "zod";
import { createCaptureMock, extractParams } from "../../scripts/generate-docs.js";

describe("extractParams", () => {
	test("required string param", () => {
		const result = extractParams({ name: z.string().describe("The name") });
		expect(result).toHaveLength(1);
		expect(result[0].name).toBe("name");
		expect(result[0].type).toBe("string");
		expect(result[0].required).toBe(true);
		expect(result[0].description).toBe("The name");
	});

	test("optional number param", () => {
		const result = extractParams({ count: z.number().optional().describe("How many") });
		expect(result[0].required).toBe(false);
	});

	test("enum param", () => {
		const result = extractParams({ dir: z.enum(["over", "into", "out"]).describe("Dir") });
		expect(result[0].type).toContain("over");
		expect(result[0].type).toContain("into");
	});

	test("array param", () => {
		const result = extractParams({ items: z.array(z.string()).describe("List") });
		expect(result[0].type).toContain("[]");
	});

	test("boolean param is required by default", () => {
		const result = extractParams({ flag: z.boolean().describe("A flag") });
		expect(result[0].type).toBe("boolean");
		expect(result[0].required).toBe(true);
	});

	test("default param is not required", () => {
		const result = extractParams({ count: z.number().default(5).describe("Count") });
		expect(result[0].required).toBe(false);
	});

	test("nested object param", () => {
		const result = extractParams({ config: z.object({ key: z.string() }).describe("Config") });
		expect(result[0].type).toBe("object");
	});

	test("union param shows both types", () => {
		const result = extractParams({ val: z.union([z.string(), z.number()]).describe("Value") });
		expect(result[0].type).toContain("string");
		expect(result[0].type).toContain("number");
	});

	test("multiple params", () => {
		const result = extractParams({
			a: z.string().describe("First"),
			b: z.number().optional().describe("Second"),
		});
		expect(result).toHaveLength(2);
		expect(result[0].name).toBe("a");
		expect(result[1].name).toBe("b");
	});

	test("empty params", () => {
		const result = extractParams({});
		expect(result).toHaveLength(0);
	});
});

describe("createCaptureMock", () => {
	test("captures tool registrations with description", () => {
		const { server, tools } = createCaptureMock();
		(server as { tool: (...args: unknown[]) => void }).tool("test_tool", "A test", { id: z.string() }, async () => ({}));
		expect(tools).toHaveLength(1);
		expect(tools[0].name).toBe("test_tool");
		expect(tools[0].description).toBe("A test");
	});

	test("captures tool registrations without description", () => {
		const { server, tools } = createCaptureMock();
		(server as { tool: (...args: unknown[]) => void }).tool("test_tool", { id: z.string() }, async () => ({}));
		expect(tools).toHaveLength(1);
		expect(tools[0].name).toBe("test_tool");
		expect(tools[0].description).toBe("");
	});

	test("captures multiple tools", () => {
		const { server, tools } = createCaptureMock();
		const t = server as { tool: (...args: unknown[]) => void };
		t.tool("tool_a", "Tool A", { a: z.string() }, async () => ({}));
		t.tool("tool_b", "Tool B", { b: z.number() }, async () => ({}));
		expect(tools).toHaveLength(2);
		expect(tools[0].name).toBe("tool_a");
		expect(tools[1].name).toBe("tool_b");
	});

	test("captures params schema", () => {
		const { server, tools } = createCaptureMock();
		(server as { tool: (...args: unknown[]) => void }).tool("test_tool", "Desc", { id: z.string().describe("The ID") }, async () => ({}));
		const params = extractParams(tools[0].params);
		expect(params[0].name).toBe("id");
		expect(params[0].type).toBe("string");
		expect(params[0].description).toBe("The ID");
	});
});
