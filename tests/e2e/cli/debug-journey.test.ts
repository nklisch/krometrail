import { resolve } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { extractCliSessionId, runCli, runCliJson } from "../../helpers/cli-runner.js";
import { SKIP_NO_DEBUGPY } from "../../helpers/debugpy-check.js";

const SIMPLE_LOOP = resolve(import.meta.dirname, "../../fixtures/python/simple-loop.py");

describe.skipIf(SKIP_NO_DEBUGPY)("E2E CLI: debug journey — JSON envelope", () => {
	// Track session IDs for cleanup
	const activeSessions: string[] = [];

	afterAll(async () => {
		// Best-effort cleanup of any leaked sessions
		for (const sid of activeSessions) {
			try {
				await runCli(["debug", "stop", "--session", sid]);
			} catch {
				/* ignore */
			}
		}
	});

	it("launch → continue → eval → stop (JSON envelope end-to-end)", async () => {
		// 1. Launch with breakpoint
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`, "--json"]);
		expect(launch.ok).toBe(true);
		if (!launch.ok) throw new Error("launch failed");
		expect(launch.data.sessionId).toBeTypeOf("string");
		expect(launch.data.sessionId).toBeTruthy();
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		// 2. Continue to breakpoint
		const cont = await runCliJson(["debug", "continue", "--session", sid, "--json"], { timeoutMs: 15_000 });
		expect(cont.ok).toBe(true);
		if (!cont.ok) throw new Error("continue failed");
		expect(cont.data.viewport).toContain("STOPPED");

		// 3. Evaluate expression
		const evalResult = await runCliJson(["debug", "eval", "total", "--session", sid, "--json"]);
		expect(evalResult.ok).toBe(true);
		if (!evalResult.ok) throw new Error("eval failed");
		expect(evalResult.data.expression).toBe("total");
		expect(evalResult.data.result).toBeDefined();

		// 4. Get variables
		const vars = await runCliJson(["debug", "vars", "--session", sid, "--json"]);
		expect(vars.ok).toBe(true);

		// 5. Get stack trace
		const stack = await runCliJson(["debug", "stack", "--session", sid, "--json"]);
		expect(stack.ok).toBe(true);
		if (!stack.ok) throw new Error("stack failed");
		expect(stack.data.stackTrace).toContain("simple-loop.py");

		// 6. Stop
		const stop = await runCliJson(["debug", "stop", "--session", sid, "--json"]);
		expect(stop.ok).toBe(true);
		if (!stop.ok) throw new Error("stop failed");
		expect(stop.data.actionCount).toBeTypeOf("number");
		expect(stop.data.durationMs).toBeTypeOf("number");
		activeSessions.pop();
	});

	it("step over/into/out with --json envelope", async () => {
		// Launch with stop-on-entry
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--stop-on-entry", "--json"]);
		expect(launch.ok).toBe(true);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		// Step over
		const stepOver = await runCliJson(["debug", "step", "over", "--session", sid, "--json"]);
		expect(stepOver.ok).toBe(true);
		if (!stepOver.ok) throw new Error("step failed");
		expect(stepOver.data.viewport).toContain("STOPPED");

		// Step into
		const stepInto = await runCliJson(["debug", "step", "into", "--session", sid, "--json"]);
		expect(stepInto.ok).toBe(true);

		// Stop
		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("breakpoints set/list with --json envelope", async () => {
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--stop-on-entry", "--json"]);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		// Set breakpoint
		const setBp = await runCliJson(["debug", "break", `${SIMPLE_LOOP}:6`, "--session", sid, "--json"]);
		expect(setBp.ok).toBe(true);
		if (!setBp.ok) throw new Error("break set failed");
		expect(setBp.data.file).toContain("simple-loop.py");
		expect(setBp.data.breakpoints).toHaveLength(1);
		expect(setBp.data.breakpoints[0].verified).toBe(true);

		// List breakpoints
		const listBp = await runCliJson(["debug", "breakpoints", "--session", sid, "--json"]);
		expect(listBp.ok).toBe(true);

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("watch/unwatch with --json envelope", async () => {
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`, "--json"]);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		// Continue to breakpoint first
		await runCli(["debug", "continue", "--session", sid], { timeoutMs: 15_000 });

		// Add watch
		const watch = await runCliJson(["debug", "watch", "total", "--session", sid, "--json"]);
		expect(watch.ok).toBe(true);
		if (!watch.ok) throw new Error("watch failed");
		expect(watch.data.watchExpressions).toContain("total");
		expect(watch.data.count).toBe(1);

		// Remove watch
		const unwatch = await runCliJson(["debug", "unwatch", "total", "--session", sid, "--json"]);
		expect(unwatch.ok).toBe(true);
		if (!unwatch.ok) throw new Error("unwatch failed");
		expect(unwatch.data.count).toBe(0);

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("MCP parity flags: --cwd and --env on launch", async () => {
		const launch = await runCliJson([
			"debug",
			"launch",
			`python3 ${SIMPLE_LOOP}`,
			"--cwd",
			resolve(import.meta.dirname, "../../fixtures/python"),
			"--env",
			"MY_VAR=hello",
			"--stop-on-entry",
			"--json",
		]);
		expect(launch.ok).toBe(true);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("MCP parity flags: viewport config on launch", async () => {
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`, "--source-lines", "20", "--stack-depth", "3", "--token-budget", "4000", "--json"]);
		expect(launch.ok).toBe(true);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("text mode output (no --json) for agent skill path", async () => {
		const launch = await runCli(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`]);
		expect(launch.exitCode).toBe(0);
		expect(launch.stdout).toContain("Session started:");
		const sid = extractCliSessionId(launch.stdout);

		const cont = await runCli(["debug", "continue", "--session", sid], { timeoutMs: 15_000 });
		expect(cont.exitCode).toBe(0);
		expect(cont.stdout).toContain("STOPPED");
		expect(cont.stdout).toContain("simple-loop.py");

		const evalResult = await runCli(["debug", "eval", "total", "--session", sid]);
		expect(evalResult.exitCode).toBe(0);
		expect(evalResult.stdout).toContain("total =");

		await runCli(["debug", "stop", "--session", sid]);
	});

	it("source view with --json envelope", async () => {
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`, "--json"]);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		await runCli(["debug", "continue", "--session", sid], { timeoutMs: 15_000 });

		const source = await runCliJson(["debug", "source", `${SIMPLE_LOOP}:1-10`, "--session", sid, "--json"]);
		expect(source.ok).toBe(true);
		if (!source.ok) throw new Error("source failed");
		expect(source.data.file).toContain("simple-loop.py");
		expect(source.data.source).toContain("def sum_range");

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});

	it("log and output commands with --json envelope", async () => {
		const launch = await runCliJson(["debug", "launch", `python3 ${SIMPLE_LOOP}`, "--break", `${SIMPLE_LOOP}:6`, "--json"]);
		if (!launch.ok) throw new Error("launch failed");
		const sid = launch.data.sessionId;
		activeSessions.push(sid);

		await runCli(["debug", "continue", "--session", sid], { timeoutMs: 15_000 });

		// Log
		const log = await runCliJson(["debug", "log", "--session", sid, "--json"]);
		expect(log.ok).toBe(true);
		if (!log.ok) throw new Error("log failed");
		expect(log.data.log).toBeTypeOf("string");

		// Output
		const output = await runCliJson(["debug", "output", "--session", sid, "--json"]);
		expect(output.ok).toBe(true);
		if (!output.ok) throw new Error("output failed");
		expect(output.data.stream).toBeTypeOf("string");

		await runCli(["debug", "stop", "--session", sid]);
		activeSessions.pop();
	});
});

describe.skipIf(SKIP_NO_DEBUGPY)("E2E CLI: error handling and exit codes", () => {
	it("debug stop with nonexistent session returns exit code 3 (NOT_FOUND)", async () => {
		const result = await runCli(["debug", "stop", "--session", "nonexistent", "--json"]);
		expect(result.exitCode).toBe(3);
		const parsed = JSON.parse(result.stderr.trim());
		expect(parsed.ok).toBe(false);
		expect(parsed.error.code).toBeDefined();
		expect(parsed.error.retryable).toBe(false);
	});

	it("debug eval without active session returns error envelope", async () => {
		// No daemon running or no sessions — should error
		const result = await runCli(["debug", "eval", "x", "--json"]);
		expect(result.exitCode).toBeGreaterThan(0);
	});

	it("error envelope has code, message, retryable fields", async () => {
		const result = await runCli(["debug", "status", "--session", "nonexistent", "--json"]);
		expect(result.exitCode).toBeGreaterThan(0);
		const parsed = JSON.parse(result.stderr.trim());
		expect(parsed).toHaveProperty("ok", false);
		expect(parsed.error).toHaveProperty("code");
		expect(parsed.error).toHaveProperty("message");
		expect(parsed.error).toHaveProperty("retryable");
		expect(parsed.error.code).toBeTypeOf("string");
		expect(parsed.error.message).toBeTypeOf("string");
		expect(parsed.error.retryable).toBeTypeOf("boolean");
	});
});
