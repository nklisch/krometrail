import { describe, expect, it } from "vitest";
import { parseRubyCommand, RubyAdapter } from "../../../src/adapters/ruby.js";

describe("parseRubyCommand", () => {
	it("parses 'ruby app.rb --verbose'", () => {
		const result = parseRubyCommand("ruby app.rb --verbose");
		expect(result.script).toBe("app.rb");
		expect(result.args).toEqual(["--verbose"]);
	});

	it("parses bare script 'app.rb'", () => {
		const result = parseRubyCommand("app.rb");
		expect(result.script).toBe("app.rb");
		expect(result.args).toEqual([]);
	});

	it("parses 'ruby script.rb arg1 arg2'", () => {
		const result = parseRubyCommand("ruby script.rb arg1 arg2");
		expect(result.script).toBe("script.rb");
		expect(result.args).toEqual(["arg1", "arg2"]);
	});

	it("parses path without ruby prefix", () => {
		const result = parseRubyCommand("/path/to/app.rb");
		expect(result.script).toBe("/path/to/app.rb");
		expect(result.args).toEqual([]);
	});
});

describe("RubyAdapter", () => {
	it("has correct adapter properties", () => {
		const adapter = new RubyAdapter();
		expect(adapter.id).toBe("ruby");
		expect(adapter.fileExtensions).toEqual([".rb"]);
		expect(adapter.displayName).toBe("Ruby (rdbg)");
	});
});
