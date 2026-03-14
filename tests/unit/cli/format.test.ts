import { describe, expect, it } from "vitest";
import type { SessionSummary } from "../../../src/browser/investigation/query-engine.js";
import type { BrowserSessionInfo } from "../../../src/browser/types.js";
import type { DoctorResult } from "../../../src/cli/commands/doctor.js";
import { formatDoctor } from "../../../src/cli/commands/doctor.js";
import {
	formatBreakpointsList,
	formatBreakpointsSet,
	formatBrowserSession,
	formatBrowserSessions,
	formatError,
	formatEvaluate,
	formatInvestigation,
	formatLaunch,
	formatLog,
	formatOutput,
	formatSource,
	formatStackTrace,
	formatStatus,
	formatStop,
	formatThreads,
	formatVariables,
	formatViewport,
	resolveOutputMode,
} from "../../../src/cli/format.js";
import { DAPTimeoutError, SessionNotFoundError } from "../../../src/core/errors.js";
import type { BreakpointsListPayload, BreakpointsResultPayload, LaunchResultPayload, StatusResultPayload, StopResultPayload, ThreadInfoPayload } from "../../../src/daemon/protocol.js";

describe("resolveOutputMode", () => {
	it("returns json when json flag is set", () => {
		expect(resolveOutputMode({ json: true, quiet: false })).toBe("json");
	});

	it("returns quiet when quiet flag is set", () => {
		expect(resolveOutputMode({ json: false, quiet: true })).toBe("quiet");
	});

	it("prioritizes json over quiet", () => {
		expect(resolveOutputMode({ json: true, quiet: true })).toBe("json");
	});

	it("returns text when neither flag is set", () => {
		expect(resolveOutputMode({ json: false, quiet: false })).toBe("text");
	});

	it("returns text when no flags provided", () => {
		expect(resolveOutputMode({})).toBe("text");
	});
});

describe("formatLaunch", () => {
	const result: LaunchResultPayload = { sessionId: "sess-abc", status: "running" };

	it("text mode includes session id and status", () => {
		const out = formatLaunch(result, "text");
		expect(out).toContain("sess-abc");
		expect(out).toContain("running");
	});

	it("text mode includes viewport when present", () => {
		const withViewport = { ...result, viewport: "── STOPPED ──" };
		const out = formatLaunch(withViewport, "text");
		expect(out).toContain("── STOPPED ──");
	});

	it("json mode returns envelope with sessionId and status", () => {
		const out = formatLaunch(result, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.sessionId).toBe("sess-abc");
		expect(parsed.data.status).toBe("running");
	});

	it("quiet mode returns empty string when no viewport", () => {
		const out = formatLaunch(result, "quiet");
		expect(out).toBe("");
	});

	it("quiet mode returns viewport string when present", () => {
		const withViewport = { ...result, viewport: "viewport-content" };
		const out = formatLaunch(withViewport, "quiet");
		expect(out).toBe("viewport-content");
	});
});

describe("formatStop", () => {
	const result: StopResultPayload = { duration: 5000, actionCount: 10 };

	it("text mode includes session id, duration, and action count", () => {
		const out = formatStop(result, "sess-abc", "text");
		expect(out).toContain("sess-abc");
		expect(out).toContain("5.0s");
		expect(out).toContain("10");
	});

	it("json mode returns envelope with sessionId, durationMs, and actionCount", () => {
		const out = formatStop(result, "sess-abc", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.sessionId).toBe("sess-abc");
		expect(parsed.data.durationMs).toBe(5000);
		expect(parsed.data.actionCount).toBe(10);
	});

	it("quiet mode returns empty string", () => {
		const out = formatStop(result, "sess-abc", "quiet");
		expect(out).toBe("");
	});
});

describe("formatStatus", () => {
	const result: StatusResultPayload = { status: "stopped", viewport: "viewport-text" };

	it("text mode shows status and viewport", () => {
		const out = formatStatus(result, "text");
		expect(out).toContain("stopped");
		expect(out).toContain("viewport-text");
	});

	it("json mode returns envelope with status and viewport", () => {
		const out = formatStatus(result, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.status).toBe("stopped");
		expect(parsed.data.viewport).toBe("viewport-text");
	});

	it("quiet mode returns viewport", () => {
		const out = formatStatus(result, "quiet");
		expect(out).toBe("viewport-text");
	});

	it("quiet mode returns status when no viewport", () => {
		const out = formatStatus({ status: "running" }, "quiet");
		expect(out).toBe("running");
	});
});

describe("formatViewport", () => {
	const viewport = "── STOPPED at file.py:10 ──";

	it("text mode returns viewport as-is", () => {
		expect(formatViewport(viewport, "text")).toBe(viewport);
	});

	it("quiet mode returns viewport as-is", () => {
		expect(formatViewport(viewport, "quiet")).toBe(viewport);
	});

	it("json mode wraps in envelope with viewport field", () => {
		const out = formatViewport(viewport, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.viewport).toBe(viewport);
	});
});

describe("formatEvaluate", () => {
	it("text mode shows expression = result", () => {
		const out = formatEvaluate("x + 1", "42", "text");
		expect(out).toBe("x + 1 = 42");
	});

	it("quiet mode returns just the value", () => {
		const out = formatEvaluate("x + 1", "42", "quiet");
		expect(out).toBe("42");
	});

	it("json mode returns envelope with expression and result", () => {
		const out = formatEvaluate("x + 1", "42", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.expression).toBe("x + 1");
		expect(parsed.data.result).toBe("42");
	});
});

describe("formatVariables", () => {
	const vars = "  x  = 5\n  y  = 10";

	it("text mode returns variables as-is", () => {
		expect(formatVariables(vars, "text")).toBe(vars);
	});

	it("json mode wraps in envelope with variables field", () => {
		const out = formatVariables(vars, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.variables).toBe(vars);
	});
});

describe("formatStackTrace", () => {
	const trace = "→ #0 file.py:10  func()";

	it("text mode returns trace as-is", () => {
		expect(formatStackTrace(trace, "text")).toBe(trace);
	});

	it("json mode wraps in envelope with stackTrace field", () => {
		const out = formatStackTrace(trace, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.stackTrace).toBe(trace);
	});
});

describe("formatBreakpointsSet", () => {
	const result: BreakpointsResultPayload = {
		breakpoints: [
			{ requestedLine: 10, verifiedLine: 10, verified: true },
			{ requestedLine: 20, verifiedLine: null, verified: false, message: "file not found" },
		],
	};

	it("text mode shows file and verification status", () => {
		const out = formatBreakpointsSet("app.py", result, "text");
		expect(out).toContain("app.py");
		expect(out).toContain("Line 10");
		expect(out).toContain("verified");
		expect(out).toContain("Line 20");
		expect(out).toContain("file not found");
	});

	it("json mode returns envelope with file and breakpoints", () => {
		const out = formatBreakpointsSet("app.py", result, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.file).toBe("app.py");
		expect(parsed.data.breakpoints).toHaveLength(2);
	});
});

describe("formatBreakpointsList", () => {
	const result: BreakpointsListPayload = {
		files: {
			"app.py": [{ line: 10 }, { line: 20, condition: "x > 0" }],
		},
	};

	it("text mode lists breakpoints by file", () => {
		const out = formatBreakpointsList(result, "text");
		expect(out).toContain("app.py");
		expect(out).toContain("Line 10");
		expect(out).toContain("Line 20");
		expect(out).toContain("when x > 0");
	});

	it("shows 'No breakpoints' when empty", () => {
		const out = formatBreakpointsList({ files: {} }, "text");
		expect(out).toContain("No breakpoints");
	});

	it("json mode returns envelope wrapping the result", () => {
		const out = formatBreakpointsList(result, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.files["app.py"]).toHaveLength(2);
	});
});

describe("formatDoctor", () => {
	const baseResult: DoctorResult = {
		platform: "linux x64",
		runtime: "Bun",
		runtimeVersion: "1.1.0",
		adapters: [
			{ id: "ruby", displayName: "Ruby (rdbg)", status: "available", version: "1.9.0" },
			{ id: "csharp", displayName: "C# (netcoredbg)", status: "available", version: "3.1.2" },
			{ id: "swift", displayName: "Swift (lldb-dap)", status: "missing", installHint: "xcode-select --install" },
			{ id: "kotlin", displayName: "Kotlin (java-debug-adapter)", status: "available", version: "2.0.0" },
		],
		frameworks: [],
	};

	it("text mode lists all 4 new adapters", () => {
		const out = formatDoctor(baseResult, "text");
		expect(out).toContain("Ruby (rdbg)");
		expect(out).toContain("C# (netcoredbg)");
		expect(out).toContain("Swift (lldb-dap)");
		expect(out).toContain("Kotlin (java-debug-adapter)");
	});

	it("text mode shows [OK] for available adapters with version", () => {
		const out = formatDoctor(baseResult, "text");
		expect(out).toContain("[OK]");
		expect(out).toContain("v1.9.0");
		expect(out).toContain("v3.1.2");
		expect(out).toContain("v2.0.0");
	});

	it("text mode shows [--] and install hint for missing adapters", () => {
		const out = formatDoctor(baseResult, "text");
		expect(out).toContain("[--]");
		expect(out).toContain("xcode-select --install");
	});

	it("json mode wraps result in envelope with adapter list", () => {
		const out = formatDoctor(baseResult, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		const ids = parsed.data.adapters.map((a: { id: string }) => a.id);
		expect(ids).toContain("ruby");
		expect(ids).toContain("csharp");
		expect(ids).toContain("swift");
		expect(ids).toContain("kotlin");
	});

	it("json mode preserves adapter status", () => {
		const out = formatDoctor(baseResult, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		const swift = parsed.data.adapters.find((a: { id: string }) => a.id === "swift");
		expect(swift?.status).toBe("missing");
		expect(swift?.installHint).toBe("xcode-select --install");
	});
});

describe("formatError", () => {
	it("text mode shows Error: message", () => {
		const err = new Error("Something went wrong");
		const out = formatError(err, "text");
		expect(out).toBe("Error: Something went wrong");
	});

	it("json mode returns error envelope with code and retryable", () => {
		const err = new SessionNotFoundError("sess-1");
		const out = formatError(err, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("SESSION_NOT_FOUND");
		expect(parsed.error.message).toContain("sess-1");
		expect(parsed.error.retryable).toBe(false);
	});

	it("json mode marks timeout errors as retryable", () => {
		const err = new DAPTimeoutError("continue", 5000);
		const out = formatError(err, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("DAP_TIMEOUT");
		expect(parsed.error.retryable).toBe(true);
	});

	it("json mode returns UNKNOWN_ERROR for generic errors", () => {
		const simpleErr = new Error("Simple error");
		const out = formatError(simpleErr, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBe("UNKNOWN_ERROR");
		expect(parsed.error.message).toBe("Simple error");
	});
});

describe("formatThreads", () => {
	const threads: ThreadInfoPayload[] = [
		{ id: 1, name: "main", stopped: true },
		{ id: 2, name: "worker", stopped: false },
	];

	it("text mode lists threads with stopped indicator", () => {
		const out = formatThreads(threads, "text");
		expect(out).toContain("Thread 1");
		expect(out).toContain("main");
		expect(out).toContain("stopped");
		expect(out).toContain("Thread 2");
		expect(out).toContain("running");
	});

	it("json mode wraps in envelope with threads and count", () => {
		const out = formatThreads(threads, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.count).toBe(2);
		expect(parsed.data.threads).toHaveLength(2);
	});
});

describe("formatSource", () => {
	it("text mode returns source as-is", () => {
		const out = formatSource("app.py", "def main(): pass", "text");
		expect(out).toBe("def main(): pass");
	});

	it("json mode wraps in envelope with file and source", () => {
		const out = formatSource("app.py", "def main(): pass", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.file).toBe("app.py");
		expect(parsed.data.source).toBe("def main(): pass");
	});
});

describe("formatLog", () => {
	it("text mode returns log as-is", () => {
		const out = formatLog("session started", "text");
		expect(out).toBe("session started");
	});

	it("json mode wraps in envelope with log field", () => {
		const out = formatLog("session started", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.log).toBe("session started");
	});
});

describe("formatOutput", () => {
	it("text mode returns output or fallback message", () => {
		expect(formatOutput("hello", "stdout", "text")).toBe("hello");
		expect(formatOutput("", "stdout", "text")).toBe("No output captured.");
	});

	it("json mode wraps in envelope with output and stream", () => {
		const out = formatOutput("hello", "stdout", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.output).toBe("hello");
		expect(parsed.data.stream).toBe("stdout");
	});
});

describe("formatBrowserSession", () => {
	const info: BrowserSessionInfo = {
		id: "session-1",
		startedAt: new Date("2024-01-01T12:00:00Z").getTime(),
		eventCount: 42,
		markerCount: 3,
		bufferAgeMs: 5000,
		tabs: [{ targetId: "t1", url: "https://example.com", title: "Example" }],
	};

	it("text mode includes event count and marker count", () => {
		const out = formatBrowserSession(info, "text");
		expect(out).toContain("42");
		expect(out).toContain("3");
		expect(out).toContain("https://example.com");
	});

	it("json mode wraps in envelope with session fields", () => {
		const out = formatBrowserSession(info, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.eventCount).toBe(42);
		expect(parsed.data.markerCount).toBe(3);
		expect(parsed.data.tabs).toHaveLength(1);
		expect(parsed.data.tabs[0].url).toBe("https://example.com");
	});
});

describe("formatBrowserSessions", () => {
	const sessions: SessionSummary[] = [
		{
			id: "s1",
			startedAt: new Date("2024-01-01T10:00:00Z").getTime(),
			duration: 60000,
			url: "https://app.com",
			title: "App",
			eventCount: 100,
			markerCount: 2,
			errorCount: 1,
		},
	];

	it("text mode lists session details", () => {
		const out = formatBrowserSessions(sessions, "text");
		expect(out).toContain("s1");
		expect(out).toContain("https://app.com");
	});

	it("json mode wraps sessions in envelope", () => {
		const out = formatBrowserSessions(sessions, "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.count).toBe(1);
		expect(parsed.data.sessions).toHaveLength(1);
	});

	it("text mode shows 'No recorded sessions' when empty", () => {
		const out = formatBrowserSessions([], "text");
		expect(out).toContain("No recorded sessions");
	});
});

describe("formatInvestigation", () => {
	it("text mode returns result as-is", () => {
		const out = formatInvestigation("some result", "overview", "text");
		expect(out).toBe("some result");
	});

	it("json mode wraps in envelope with result and command", () => {
		const out = formatInvestigation("some result", "overview", "json");
		const parsed = JSON.parse(out);
		expect(parsed.ok).toBe(true);
		expect(parsed.data.result).toBe("some result");
		expect(parsed.data.command).toBe("overview");
	});
});
