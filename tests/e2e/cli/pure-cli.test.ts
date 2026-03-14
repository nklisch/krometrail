import { describe, expect, it } from "vitest";
import { runCli, runCliJson } from "../../helpers/cli-runner.js";

describe("E2E CLI: doctor", () => {
	it("doctor --json returns envelope with platform and adapters", async () => {
		const result = await runCliJson(["doctor", "--json"]);
		expect(result.ok).toBe(true);
		if (!result.ok) throw new Error("expected ok");
		expect(result.data.platform).toBeTypeOf("string");
		expect(result.data.runtime).toBeTypeOf("string");
		expect(Array.isArray(result.data.adapters)).toBe(true);
		expect(result.data.adapters.length).toBeGreaterThan(0);
		// Each adapter has id, displayName, status
		for (const adapter of result.data.adapters) {
			expect(adapter).toHaveProperty("id");
			expect(adapter).toHaveProperty("displayName");
			expect(["available", "missing"]).toContain(adapter.status);
		}
	});

	it("doctor --json includes fixCommand for missing adapters", async () => {
		const result = await runCliJson(["doctor", "--json"]);
		if (!result.ok) throw new Error("expected ok");
		const missing = result.data.adapters.filter((a: { status: string }) => a.status === "missing");
		// At least some missing adapters should have fixCommand
		// (not all will — some don't have known install commands)
		if (missing.length > 0) {
			const withFix = missing.filter((a: { fixCommand?: string }) => a.fixCommand);
			// We don't assert > 0 since it depends on system, but validate shape
			for (const a of withFix) {
				expect(a.fixCommand).toBeTypeOf("string");
			}
		}
	});

	it("doctor text mode outputs human-readable table", async () => {
		const result = await runCli(["doctor"]);
		expect(result.exitCode).toBe(0);
		expect(result.stdout).toContain("Krometrail");
		expect(result.stdout).toContain("Platform:");
		expect(result.stdout).toContain("Adapters:");
	});

	it("doctor exit code is 0 when at least one adapter available", async () => {
		const result = await runCli(["doctor"]);
		// On any dev machine, at least Python or Node adapter should be available
		expect(result.exitCode).toBe(0);
	});
});

describe("E2E CLI: commands", () => {
	it("commands returns JSON envelope by default", async () => {
		const result = await runCliJson(["commands"]);
		expect(result.ok).toBe(true);
		if (!result.ok) throw new Error("expected ok");
		expect(result.data.version).toBe("0.1.0");
		expect(Array.isArray(result.data.groups)).toBe(true);
	});

	it("commands lists debug, browser, and top-level groups", async () => {
		const result = await runCliJson(["commands"]);
		if (!result.ok) throw new Error("expected ok");
		const groupNames = result.data.groups.map((g: { name: string }) => g.name);
		expect(groupNames).toContain("debug");
		expect(groupNames).toContain("browser");
		expect(groupNames).toContain("top-level");
	});

	it("commands --group debug only returns debug commands", async () => {
		const result = await runCliJson(["commands", "--group", "debug"]);
		if (!result.ok) throw new Error("expected ok");
		expect(result.data.groups).toHaveLength(1);
		expect(result.data.groups[0].name).toBe("debug");
		const commandNames = result.data.groups[0].commands.map((c: { name: string }) => c.name);
		expect(commandNames).toContain("launch");
		expect(commandNames).toContain("step");
		expect(commandNames).toContain("eval");
		expect(commandNames).toContain("stop");
	});

	it("debug commands include new MCP parity args", async () => {
		const result = await runCliJson(["commands", "--group", "debug"]);
		if (!result.ok) throw new Error("expected ok");
		const launch = result.data.groups[0].commands.find((c: { name: string }) => c.name === "launch");
		expect(launch).toBeTruthy();
		const argNames = launch.args.map((a: { name: string }) => a.name);
		expect(argNames).toContain("cwd");
		expect(argNames).toContain("env");
		expect(argNames).toContain("source-lines");
		expect(argNames).toContain("stack-depth");
		expect(argNames).toContain("token-budget");
		expect(argNames).toContain("diff-mode");
	});

	it("browser commands include new MCP parity args", async () => {
		const result = await runCliJson(["commands", "--group", "browser"]);
		if (!result.ok) throw new Error("expected ok");
		const search = result.data.groups[0].commands.find((c: { name: string }) => c.name === "search");
		expect(search).toBeTruthy();
		const argNames = search.args.map((a: { name: string }) => a.name);
		expect(argNames).toContain("around-marker");
		expect(argNames).toContain("url-pattern");
		expect(argNames).toContain("console-levels");
		expect(argNames).toContain("framework");
		expect(argNames).toContain("component");
		expect(argNames).toContain("pattern");
	});

	it("commands include arg metadata (type, required, alias, description)", async () => {
		const result = await runCliJson(["commands", "--group", "debug"]);
		if (!result.ok) throw new Error("expected ok");
		const step = result.data.groups[0].commands.find((c: { name: string }) => c.name === "step");
		expect(step).toBeTruthy();
		const directionArg = step.args.find((a: { name: string }) => a.name === "direction");
		expect(directionArg).toBeTruthy();
		expect(directionArg.type).toBe("positional");
		expect(directionArg.required).toBe(true);
		expect(directionArg.description).toBeTypeOf("string");
	});
});

describe("E2E CLI: namespace structure", () => {
	it("top-level krometrail --help shows help", async () => {
		const result = await runCli(["--help"]);
		// citty prints help/usage with --help
		expect(result.stdout + result.stderr).toMatch(/debug|browser|doctor|commands/i);
	});

	it("krometrail debug with no subcommand shows debug help", async () => {
		const result = await runCli(["debug"]);
		expect(result.stdout + result.stderr).toMatch(/launch|step|eval|stop/i);
	});

	it("krometrail browser with no subcommand shows browser help", async () => {
		const result = await runCli(["browser"]);
		expect(result.stdout + result.stderr).toMatch(/start|mark|status|stop|sessions/i);
	});
});
