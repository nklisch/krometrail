import { describe, expect, it } from "vitest";
import { GoAdapter, parseGoCommand } from "../../../src/adapters/go.js";

describe("parseGoCommand", () => {
	it("parses 'go run main.go'", () => {
		const result = parseGoCommand("go run main.go");
		expect(result.mode).toBe("debug");
		expect(result.program).toBe("main.go");
		expect(result.args).toEqual([]);
	});

	it("parses 'go run main.go --flag'", () => {
		const result = parseGoCommand("go run main.go --flag");
		expect(result.mode).toBe("debug");
		expect(result.program).toBe("main.go");
		expect(result.args).toEqual(["--flag"]);
	});

	it("parses 'go test ./...'", () => {
		const result = parseGoCommand("go test ./...");
		expect(result.mode).toBe("test");
		expect(result.program).toBe("./...");
		expect(result.args).toEqual([]);
	});

	it("parses 'go test ./pkg/... -v'", () => {
		const result = parseGoCommand("go test ./pkg/... -v");
		expect(result.mode).toBe("test");
		expect(result.program).toBe("./pkg/...");
		expect(result.args).toEqual(["-v"]);
	});

	it("parses './mybinary --flag' as exec mode", () => {
		const result = parseGoCommand("./mybinary --flag");
		expect(result.mode).toBe("exec");
		expect(result.program).toBe("./mybinary");
		expect(result.args).toEqual(["--flag"]);
	});

	it("parses '/abs/path/to/binary' as exec mode", () => {
		const result = parseGoCommand("/abs/path/to/binary");
		expect(result.mode).toBe("exec");
		expect(result.program).toBe("/abs/path/to/binary");
		expect(result.args).toEqual([]);
	});

	it("parses 'go run -gcflags=-N main.go' preserving build flags", () => {
		const result = parseGoCommand("go run -gcflags=-N main.go");
		expect(result.mode).toBe("debug");
		expect(result.program).toBe("main.go");
		expect(result.buildFlags).toEqual(["-gcflags=-N"]);
	});
});

describe("GoAdapter", () => {
	it("has correct adapter properties", () => {
		const adapter = new GoAdapter();
		expect(adapter.id).toBe("go");
		expect(adapter.fileExtensions).toEqual([".go"]);
		expect(adapter.displayName).toBe("Go (Delve)");
	});
});
