import { resolve } from "node:path";
import type { DebugProtocol } from "@vscode/debugprotocol";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import type { DAPConnection } from "../../src/adapters/base.js";
import { PythonAdapter } from "../../src/adapters/python.js";
import { DAPClient } from "../../src/core/dap-client.js";
import { SKIP_NO_DEBUGPY } from "../helpers/debugpy-check.js";

const FIXTURE = resolve(import.meta.dirname, "../fixtures/python/simple-loop.py");

/**
 * Set up a fully initialized DAP session using debugpy.adapter protocol.
 * debugpy.adapter requires: initialize → launch (triggers initialized) → setBreakpoints → configurationDone → await launch.
 *
 * Returns a promise that resolves when the debuggee stops (registered before configurationDone
 * to avoid the race where `stopped` fires before the caller can call waitForStop).
 */
async function initSession(client: DAPClient, connection: DAPConnection, bpLine?: number): Promise<import("../../src/core/dap-client.js").StopResult> {
	// Register initialized listener BEFORE initialize so we never miss it
	const initializedEvent = new Promise<void>((r) => {
		const handler = () => {
			client.off("initialized", handler);
			r();
		};
		client.on("initialized", handler);
	});

	await client.initialize();

	// For debugpy.adapter: launch triggers the server which sends `initialized`
	const launchPromise = client.launch(connection.launchArgs as DebugProtocol.LaunchRequestArguments);

	// Wait for initialized (comes from adapter after server starts)
	await initializedEvent;

	if (bpLine !== undefined) {
		await client.setBreakpoints({ path: FIXTURE, name: "simple-loop.py" }, [{ line: bpLine }]);
	}

	// Register stop listener BEFORE configurationDone so we never miss the stopped event
	const stopPromise = client.waitForStop(15_000);

	await client.configurationDone();
	await launchPromise;

	return stopPromise;
}

describe.skipIf(SKIP_NO_DEBUGPY)("DAPClient integration", () => {
	let adapter: PythonAdapter;
	let connection: DAPConnection;
	let client: DAPClient;

	beforeAll(async () => {
		adapter = new PythonAdapter();
	});

	afterEach(async () => {
		try {
			client?.dispose();
		} catch {
			// ignore
		}
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("connects to debugpy and runs initialize handshake", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);
		const caps = await client.initialize();
		expect(caps).toBeDefined();
		expect(client.connected).toBe(true);
	});

	it("sets breakpoint, launches script, and receives stopped event", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		expect(stopResult.type).toBe("stopped");
		if (stopResult.type === "stopped") {
			expect(stopResult.event.body.reason).toBe("breakpoint");
		}
	});

	it("gets stack trace at breakpoint", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		expect(stopResult.type).toBe("stopped");

		if (stopResult.type === "stopped") {
			const threadId = stopResult.event.body.threadId ?? 1;
			const stackResponse = await client.stackTrace(threadId, 0, 5);
			expect(stackResponse.body.stackFrames.length).toBeGreaterThan(0);
			expect(stackResponse.body.stackFrames[0].line).toBe(6);
		}
	});

	it("gets scopes and variables at breakpoint", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		if (stopResult.type !== "stopped") return;

		const threadId = stopResult.event.body.threadId ?? 1;
		const stackResponse = await client.stackTrace(threadId, 0, 1);
		const frameId = stackResponse.body.stackFrames[0].id;

		const scopesResponse = await client.scopes(frameId);
		expect(scopesResponse.body.scopes.length).toBeGreaterThan(0);

		const localsScope = scopesResponse.body.scopes.find((s) => s.name.toLowerCase().includes("local"));
		if (localsScope) {
			const varsResponse = await client.variables(localsScope.variablesReference);
			expect(varsResponse.body.variables).toBeDefined();
		}
	});

	it("evaluates expression at breakpoint", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		if (stopResult.type !== "stopped") return;

		const threadId = stopResult.event.body.threadId ?? 1;
		const stackResponse = await client.stackTrace(threadId, 0, 1);
		const frameId = stackResponse.body.stackFrames[0].id;

		const evalResponse = await client.evaluate("1 + 1", frameId, "repl");
		expect(evalResponse.body.result).toBe("2");
	});

	it("steps over and receives stopped event", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		if (stopResult.type !== "stopped") return;
		const threadId = stopResult.event.body.threadId ?? 1;

		await client.next(threadId);
		const stepResult = await client.waitForStop(10_000);
		expect(stepResult.type).toBe("stopped");
	});

	it("continues to end and receives terminated or exited event", async () => {
		connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		client = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: 30_000 });
		client.attachStreams(connection.reader, connection.writer);

		const stopResult = await initSession(client, connection, 6);
		if (stopResult.type !== "stopped") return;
		const threadId = stopResult.event.body.threadId ?? 1;

		await client.continue(threadId);
		const endResult = await client.waitForStop(10_000);
		// Can be terminated or exited or stopped (at next iteration)
		expect(["stopped", "terminated", "exited"]).toContain(endResult.type);
	});
});
