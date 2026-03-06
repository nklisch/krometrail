import { describe, expect, it } from "vitest";
import { CSharpAdapter, parseCSharpCommand } from "../../../src/adapters/csharp.js";

describe("parseCSharpCommand", () => {
	it("parses 'dotnet run'", () => {
		const result = parseCSharpCommand("dotnet run");
		expect(result.type).toBe("project");
		expect(result.path).toBe(".");
		expect(result.args).toEqual([]);
	});

	it("parses 'dotnet run --project MyApp'", () => {
		const result = parseCSharpCommand("dotnet run --project MyApp");
		expect(result.type).toBe("project");
		expect(result.path).toBe("MyApp");
	});

	it("parses 'dotnet MyApp.dll'", () => {
		const result = parseCSharpCommand("dotnet MyApp.dll");
		expect(result.type).toBe("dll");
		expect(result.path).toBe("MyApp.dll");
	});

	it("parses 'MyApp.cs'", () => {
		const result = parseCSharpCommand("MyApp.cs");
		expect(result.type).toBe("source");
		expect(result.path).toBe("MyApp.cs");
	});

	it("parses './MyApp' as binary", () => {
		const result = parseCSharpCommand("./MyApp");
		expect(result.type).toBe("binary");
		expect(result.path).toBe("./MyApp");
	});

	it("parses 'App.dll arg1' with args", () => {
		const result = parseCSharpCommand("App.dll arg1");
		expect(result.type).toBe("dll");
		expect(result.path).toBe("App.dll");
		expect(result.args).toEqual(["arg1"]);
	});
});

describe("CSharpAdapter", () => {
	it("has correct adapter properties", () => {
		const adapter = new CSharpAdapter();
		expect(adapter.id).toBe("csharp");
		expect(adapter.fileExtensions).toEqual([".cs"]);
		expect(adapter.displayName).toBe("C# (netcoredbg)");
	});
});
