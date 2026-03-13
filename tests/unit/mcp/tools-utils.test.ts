import { describe, expect, it } from "vitest";
import { errorResponse, textResponse, toolHandler } from "../../../src/mcp/tools/utils.js";

describe("textResponse", () => {
	it("wraps string in MCP content array", () => {
		const result = textResponse("hello");
		expect(result).toEqual({
			content: [{ type: "text", text: "hello" }],
		});
	});

	it("does not set isError", () => {
		const result = textResponse("ok");
		expect(result).not.toHaveProperty("isError");
	});

	it("handles empty string", () => {
		const result = textResponse("");
		expect(result.content[0].text).toBe("");
	});
});

describe("errorResponse", () => {
	it("extracts message from Error and sets isError", () => {
		const result = errorResponse(new Error("something broke"));
		expect(result).toEqual({
			content: [{ type: "text", text: "something broke" }],
			isError: true,
		});
	});

	it("handles string errors", () => {
		const result = errorResponse("raw string error");
		expect(result.content[0].text).toBe("raw string error");
		expect(result.isError).toBe(true);
	});

	it("handles null/undefined errors", () => {
		expect(errorResponse(null).content[0].text).toBe("null");
		expect(errorResponse(undefined).content[0].text).toBe("undefined");
	});

	it("handles numeric errors", () => {
		expect(errorResponse(42).content[0].text).toBe("42");
		expect(errorResponse(42).isError).toBe(true);
	});
});

describe("toolHandler", () => {
	it("returns textResponse on success", async () => {
		const handler = toolHandler(async () => "result text");
		const result = await handler({});
		expect(result).toEqual({
			content: [{ type: "text", text: "result text" }],
		});
	});

	it("returns errorResponse when fn throws Error", async () => {
		const handler = toolHandler(async () => {
			throw new Error("handler failed");
		});
		const result = await handler({});
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toBe("handler failed");
	});

	it("returns errorResponse when fn throws non-Error", async () => {
		const handler = toolHandler(async () => {
			throw "string throw";
		});
		const result = await handler({});
		expect(result.isError).toBe(true);
		expect(result.content[0].text).toBe("string throw");
	});

	it("passes params through to the wrapped function", async () => {
		const handler = toolHandler(async (params: { name: string }) => `Hello ${params.name}`);
		const result = await handler({ name: "World" });
		expect(result.content[0].text).toBe("Hello World");
	});
});
