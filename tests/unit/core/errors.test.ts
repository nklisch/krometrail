import { describe, expect, it } from "vitest";
import {
	AdapterNotFoundError,
	AdapterPrerequisiteError,
	AgentLensError,
	CDPConnectionError,
	ChromeNotFoundError,
	DAPClientDisposedError,
	DAPConnectionError,
	DAPTimeoutError,
	LaunchError,
	SessionLimitError,
	SessionNotFoundError,
	SessionStateError,
	TabNotFoundError,
	BrowserRecorderStateError,
	getErrorMessage,
} from "../../../src/core/errors.js";

describe("getErrorMessage", () => {
	it("returns .message for Error instances", () => {
		expect(getErrorMessage(new Error("boom"))).toBe("boom");
	});

	it("returns .message for AgentLensError subclasses", () => {
		expect(getErrorMessage(new DAPTimeoutError("initialize", 5000))).toBe("DAP request 'initialize' timed out after 5000ms");
	});

	it("returns String(value) for non-Error values", () => {
		expect(getErrorMessage("string error")).toBe("string error");
		expect(getErrorMessage(42)).toBe("42");
		expect(getErrorMessage(null)).toBe("null");
		expect(getErrorMessage(undefined)).toBe("undefined");
	});
});

describe("AgentLensError", () => {
	it("sets message and code", () => {
		const err = new AgentLensError("test message", "TEST_CODE");
		expect(err.message).toBe("test message");
		expect(err.code).toBe("TEST_CODE");
		expect(err.name).toBe("AgentLensError");
	});

	it("is instanceof Error", () => {
		const err = new AgentLensError("test", "CODE");
		expect(err).toBeInstanceOf(Error);
	});
});

describe("DAPTimeoutError", () => {
	it("stores command and timeoutMs", () => {
		const err = new DAPTimeoutError("stackTrace", 10000);
		expect(err.command).toBe("stackTrace");
		expect(err.timeoutMs).toBe(10000);
		expect(err.code).toBe("DAP_TIMEOUT");
		expect(err.name).toBe("DAPTimeoutError");
		expect(err.message).toContain("stackTrace");
		expect(err.message).toContain("10000ms");
	});
});

describe("DAPClientDisposedError", () => {
	it("has correct code and name", () => {
		const err = new DAPClientDisposedError();
		expect(err.code).toBe("DAP_DISPOSED");
		expect(err.name).toBe("DAPClientDisposedError");
		expect(err.message).toContain("disposed");
	});
});

describe("DAPConnectionError", () => {
	it("stores host, port, and optional cause", () => {
		const cause = new Error("ECONNREFUSED");
		const err = new DAPConnectionError("127.0.0.1", 5678, cause);
		expect(err.host).toBe("127.0.0.1");
		expect(err.port).toBe(5678);
		expect(err.cause).toBe(cause);
		expect(err.code).toBe("DAP_CONNECTION_FAILED");
		expect(err.message).toContain("127.0.0.1:5678");
		expect(err.message).toContain("ECONNREFUSED");
	});

	it("handles missing cause", () => {
		const err = new DAPConnectionError("localhost", 9229);
		expect(err.cause).toBeUndefined();
		expect(err.message).toContain("unknown error");
	});
});

describe("SessionNotFoundError", () => {
	it("stores sessionId", () => {
		const err = new SessionNotFoundError("abc12345");
		expect(err.sessionId).toBe("abc12345");
		expect(err.code).toBe("SESSION_NOT_FOUND");
		expect(err.message).toContain("abc12345");
	});
});

describe("SessionStateError", () => {
	it("stores sessionId, currentState, expectedStates", () => {
		const err = new SessionStateError("sess1", "running", ["stopped", "terminated"]);
		expect(err.sessionId).toBe("sess1");
		expect(err.currentState).toBe("running");
		expect(err.expectedStates).toEqual(["stopped", "terminated"]);
		expect(err.code).toBe("SESSION_INVALID_STATE");
		expect(err.message).toContain("running");
		expect(err.message).toContain("stopped");
	});
});

describe("SessionLimitError", () => {
	it("stores limit details and suggestion", () => {
		const err = new SessionLimitError("maxActionsPerSession", 201, 200, "Use conditional breakpoints.");
		expect(err.limitName).toBe("maxActionsPerSession");
		expect(err.currentValue).toBe(201);
		expect(err.maxValue).toBe(200);
		expect(err.suggestion).toBe("Use conditional breakpoints.");
		expect(err.code).toBe("SESSION_LIMIT_EXCEEDED");
		expect(err.message).toContain("201/200");
	});

	it("handles missing suggestion", () => {
		const err = new SessionLimitError("sessionTimeoutMs", 300001, 300000);
		expect(err.suggestion).toBeUndefined();
	});
});

describe("AdapterPrerequisiteError", () => {
	it("stores adapterId, missing list, and installHint", () => {
		const err = new AdapterPrerequisiteError("python", ["debugpy", "python3"], "pip install debugpy");
		expect(err.adapterId).toBe("python");
		expect(err.missing).toEqual(["debugpy", "python3"]);
		expect(err.installHint).toBe("pip install debugpy");
		expect(err.code).toBe("ADAPTER_PREREQUISITES");
		expect(err.message).toContain("debugpy");
		expect(err.message).toContain("pip install debugpy");
	});

	it("handles missing installHint", () => {
		const err = new AdapterPrerequisiteError("go", ["dlv"]);
		expect(err.installHint).toBeUndefined();
	});
});

describe("AdapterNotFoundError", () => {
	it("stores languageOrExt", () => {
		const err = new AdapterNotFoundError(".rs");
		expect(err.languageOrExt).toBe(".rs");
		expect(err.code).toBe("ADAPTER_NOT_FOUND");
		expect(err.message).toContain(".rs");
	});
});

describe("LaunchError", () => {
	it("stores stderr", () => {
		const err = new LaunchError("Process exited with code 1", "ModuleNotFoundError: No module named 'foo'");
		expect(err.stderr).toContain("ModuleNotFoundError");
		expect(err.code).toBe("LAUNCH_FAILED");
	});

	it("handles missing stderr", () => {
		const err = new LaunchError("Failed to launch");
		expect(err.stderr).toBeUndefined();
	});
});

describe("ChromeNotFoundError", () => {
	it("has correct code and message", () => {
		const err = new ChromeNotFoundError();
		expect(err.code).toBe("CHROME_NOT_FOUND");
		expect(err.message).toContain("Chrome");
	});
});

describe("CDPConnectionError", () => {
	it("stores message and cause", () => {
		const cause = new Error("WebSocket closed");
		const err = new CDPConnectionError("Failed to connect to CDP", cause);
		expect(err.cause).toBe(cause);
		expect(err.code).toBe("CDP_CONNECTION_FAILED");
	});
});

describe("TabNotFoundError", () => {
	it("stores targetId", () => {
		const err = new TabNotFoundError("target-abc");
		expect(err.targetId).toBe("target-abc");
		expect(err.code).toBe("TAB_NOT_FOUND");
		expect(err.message).toContain("target-abc");
	});
});

describe("BrowserRecorderStateError", () => {
	it("has correct code", () => {
		const err = new BrowserRecorderStateError("Already recording");
		expect(err.code).toBe("BROWSER_RECORDER_STATE");
		expect(err.message).toBe("Already recording");
	});
});

describe("error hierarchy — all errors extend AgentLensError", () => {
	const errors = [
		new DAPTimeoutError("cmd", 1000),
		new DAPClientDisposedError(),
		new DAPConnectionError("h", 1),
		new SessionNotFoundError("s"),
		new SessionStateError("s", "running", ["stopped"]),
		new SessionLimitError("l", 1, 0),
		new AdapterPrerequisiteError("a", []),
		new AdapterNotFoundError("x"),
		new LaunchError("m"),
		new ChromeNotFoundError(),
		new CDPConnectionError("m"),
		new TabNotFoundError("t"),
		new BrowserRecorderStateError("m"),
	];

	for (const err of errors) {
		it(`${err.constructor.name} extends AgentLensError`, () => {
			expect(err).toBeInstanceOf(AgentLensError);
			expect(err).toBeInstanceOf(Error);
			expect(typeof err.code).toBe("string");
			expect(err.code.length).toBeGreaterThan(0);
		});
	}
});
