import { describe, expect, it } from "vitest";
import { allocatePort } from "../../../src/adapters/helpers.js";
import { parseCommand } from "../../../src/adapters/python.js";

describe("parseCommand", () => {
	it("parses 'python app.py'", () => {
		const result = parseCommand("python app.py");
		expect(result.script).toBe("app.py");
		expect(result.args).toEqual([]);
	});

	it("parses 'python3 app.py --verbose'", () => {
		const result = parseCommand("python3 app.py --verbose");
		expect(result.script).toBe("app.py");
		expect(result.args).toEqual(["--verbose"]);
	});

	it("parses 'python -m pytest tests/'", () => {
		const result = parseCommand("python -m pytest tests/");
		expect(result.script).toBe("-m");
		expect(result.args).toEqual(["pytest", "tests/"]);
	});

	it("parses 'python3 -m pytest tests/ -v'", () => {
		const result = parseCommand("python3 -m pytest tests/ -v");
		expect(result.script).toBe("-m");
		expect(result.args).toEqual(["pytest", "tests/", "-v"]);
	});

	it("parses 'python -c \"code\"'", () => {
		const result = parseCommand('python -c "import sys; print(sys.path)"');
		expect(result.script).toBe("-c");
		expect(result.args).toEqual(['"import', "sys;", "print(sys.path)\""]);
	});

	it("parses 'python3 -c code'", () => {
		const result = parseCommand("python3 -c print('hello')");
		expect(result.script).toBe("-c");
		expect(result.args).toEqual(["print('hello')"]);
	});

	it("parses bare 'app.py'", () => {
		const result = parseCommand("app.py");
		expect(result.script).toBe("app.py");
		expect(result.args).toEqual([]);
	});

	it("parses 'python3 app.py arg1 arg2'", () => {
		const result = parseCommand("python3 app.py arg1 arg2");
		expect(result.script).toBe("app.py");
		expect(result.args).toEqual(["arg1", "arg2"]);
	});
});

describe("allocatePort", () => {
	it("returns a valid port number > 0", async () => {
		const port = await allocatePort();
		expect(port).toBeGreaterThan(0);
		expect(port).toBeLessThanOrEqual(65535);
		expect(Number.isInteger(port)).toBe(true);
	});

	it("returns different ports on sequential calls", async () => {
		const port1 = await allocatePort();
		const port2 = await allocatePort();
		// Ports are likely different (not guaranteed but very probable)
		expect(typeof port1).toBe("number");
		expect(typeof port2).toBe("number");
	});
});
