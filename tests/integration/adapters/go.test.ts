import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { GoAdapter } from "../../../src/adapters/go.js";
import { SKIP_NO_DLV } from "../../helpers/dlv-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/go/simple-loop.go");

describe.skipIf(SKIP_NO_DLV)("GoAdapter integration", () => {
	let adapter: GoAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new GoAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() starts Delve and returns a working DAPConnection", async () => {
		adapter = new GoAdapter();
		const connection = await adapter.launch({ command: `go run ${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
		expect(connection.launchArgs).toBeDefined();
		expect(connection.launchArgs?.mode).toBe("debug");
	}, 15_000);

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new GoAdapter();
		const connection = await adapter.launch({ command: `go run ${FIXTURE}` });

		// Write a DAP initialize request
		const req = { seq: 1, type: "request", command: "initialize", arguments: { adapterID: "test", clientID: "test", linesStartAt1: true, columnsStartAt1: true } };
		const json = JSON.stringify(req);
		connection.writer.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);

		// Read back the response
		const response = await new Promise<string>((resolve) => {
			connection.reader.once("data", (d: Buffer) => resolve(d.toString()));
		});
		expect(response).toContain("Content-Length");
	}, 15_000);

	it("dispose() kills the Delve process", async () => {
		adapter = new GoAdapter();
		const connection = await adapter.launch({ command: `go run ${FIXTURE}` });
		const pid = connection.process?.pid;
		expect(pid).toBeDefined();

		await adapter.dispose();

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
	}, 15_000);

	it("launch with bad Go file produces clear error", async () => {
		adapter = new GoAdapter();
		await expect(adapter.launch({ command: "go run /nonexistent/path/main.go" })).rejects.toThrow();
	}, 15_000);
});

describe("GoAdapter prerequisite check when dlv missing", () => {
	it("has correct adapter properties", () => {
		const adapter = new GoAdapter();
		expect(adapter.id).toBe("go");
		expect(adapter.fileExtensions).toContain(".go");
		expect(adapter.displayName).toBe("Go (Delve)");
	});
});
