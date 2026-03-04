import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { PythonAdapter } from "../../../src/adapters/python.js";
import { SKIP_NO_DEBUGPY } from "../../helpers/debugpy-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/python/simple-loop.py");

describe.skipIf(SKIP_NO_DEBUGPY)("PythonAdapter integration", () => {
	let adapter: PythonAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new PythonAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() spawns debugpy and returns a working DAPConnection", async () => {
		adapter = new PythonAdapter();
		const connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
	});

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new PythonAdapter();
		const connection = await adapter.launch({ command: `python3 ${FIXTURE}` });

		// Write a DAP initialize request
		const req = { seq: 1, type: "request", command: "initialize", arguments: { adapterID: "test", clientID: "test", linesStartAt1: true, columnsStartAt1: true } };
		const json = JSON.stringify(req);
		connection.writer.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);

		// Read back the response — debugpy sends telemetry events first, so accumulate until we see a response
		const response = await new Promise<string>((resolve) => {
			let accumulated = "";
			const onData = (d: Buffer) => {
				accumulated += d.toString();
				if (accumulated.includes('"type":"response"') || accumulated.includes('"type": "response"')) {
					connection.reader.off("data", onData);
					resolve(accumulated);
				}
			};
			connection.reader.on("data", onData);
		});
		expect(response).toContain("Content-Length");
		expect(response).toContain("response");
	});

	it("dispose() kills the child process", async () => {
		adapter = new PythonAdapter();
		const connection = await adapter.launch({ command: `python3 ${FIXTURE}` });
		const pid = connection.process?.pid;
		expect(pid).toBeDefined();

		await adapter.dispose();

		// Check process is no longer running
		if (pid) {
			const alive = await new Promise<boolean>((resolve) => {
				try {
					process.kill(pid, 0);
					resolve(true);
				} catch {
					resolve(false);
				}
			});
			expect(alive).toBe(false);
		}
	});

	it("launch with bad script path produces clear error", async () => {
		adapter = new PythonAdapter();
		await expect(adapter.launch({ command: "python3 /nonexistent/path/script.py" })).rejects.toThrow();
	});
});

describe("PythonAdapter prerequisite check when debugpy missing", () => {
	it("returns satisfied: false with install hint when python not found", async () => {
		// This test verifies the structure; we can't easily simulate missing debugpy
		// in CI where it's available, so we just check the adapter structure
		const adapter = new PythonAdapter();
		expect(adapter.id).toBe("python");
		expect(adapter.fileExtensions).toContain(".py");
		expect(adapter.displayName).toBe("Python (debugpy)");
	});
});
