import { describe, expect, it } from "vitest";
import { NodeAdapter, parseNodeCommand } from "../../../src/adapters/node.js";

describe("parseNodeCommand", () => {
	it("parses 'node app.js --verbose'", () => {
		const result = parseNodeCommand("node app.js --verbose");
		expect(result.script).toBe("app.js");
		expect(result.args).toEqual(["--verbose"]);
	});

	it("parses 'app.js' (bare script)", () => {
		const result = parseNodeCommand("app.js");
		expect(result.script).toBe("app.js");
		expect(result.args).toEqual([]);
	});

	it("parses 'node app.js arg1 arg2'", () => {
		const result = parseNodeCommand("node app.js arg1 arg2");
		expect(result.script).toBe("app.js");
		expect(result.args).toEqual(["arg1", "arg2"]);
	});

	it("strips 'node --' prefix", () => {
		const result = parseNodeCommand("node -- app.js");
		expect(result.script).toBe("app.js");
		expect(result.args).toEqual([]);
	});

	it("strips --inspect flags", () => {
		const result = parseNodeCommand("node --inspect-brk=9229 app.js");
		expect(result.script).toBe("app.js");
		expect(result.args).toEqual([]);
	});

	it("parses 'node /abs/path/to/script.mjs --flag'", () => {
		const result = parseNodeCommand("node /abs/path/to/script.mjs --flag");
		expect(result.script).toBe("/abs/path/to/script.mjs");
		expect(result.args).toEqual(["--flag"]);
	});
});

describe("NodeAdapter", () => {
	it("has correct adapter properties", () => {
		const adapter = new NodeAdapter();
		expect(adapter.id).toBe("node");
		expect(adapter.fileExtensions).toEqual([".js", ".mjs", ".cjs"]);
		expect(adapter.displayName).toBe("Node.js (inspector)");
	});
});
