import { describe, expect, it } from "vitest";
import { parseSwiftCommand, SwiftAdapter } from "../../../src/adapters/swift.js";

describe("parseSwiftCommand", () => {
	it("parses 'main.swift' as source", () => {
		const result = parseSwiftCommand("main.swift");
		expect(result.type).toBe("source");
		expect(result.path).toBe("main.swift");
		expect(result.args).toEqual([]);
	});

	it("parses 'swiftc main.swift' as source", () => {
		const result = parseSwiftCommand("swiftc main.swift");
		expect(result.type).toBe("source");
		expect(result.path).toBe("main.swift");
	});

	it("parses 'swift run' as spm", () => {
		const result = parseSwiftCommand("swift run");
		expect(result.type).toBe("spm");
		expect(result.path).toBe(".");
	});

	it("parses 'swift run MyTarget' as spm with target", () => {
		const result = parseSwiftCommand("swift run MyTarget");
		expect(result.type).toBe("spm");
		expect(result.path).toBe("MyTarget");
	});

	it("parses './mybinary --flag' as binary", () => {
		const result = parseSwiftCommand("./mybinary --flag");
		expect(result.type).toBe("binary");
		expect(result.path).toBe("./mybinary");
		expect(result.args).toEqual(["--flag"]);
	});

	it("parses '/abs/path/binary' as binary", () => {
		const result = parseSwiftCommand("/abs/path/binary");
		expect(result.type).toBe("binary");
		expect(result.path).toBe("/abs/path/binary");
	});
});

describe("SwiftAdapter", () => {
	it("has correct adapter properties", () => {
		const adapter = new SwiftAdapter();
		expect(adapter.id).toBe("swift");
		expect(adapter.fileExtensions).toEqual([".swift"]);
		expect(adapter.displayName).toBe("Swift (lldb-dap)");
	});
});
