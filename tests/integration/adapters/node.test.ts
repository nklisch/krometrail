import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { NodeAdapter } from "../../../src/adapters/node.js";
import { SKIP_NO_NODE_DEBUG } from "../../helpers/node-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/node/simple-loop.js");

describe.skipIf(SKIP_NO_NODE_DEBUG)("NodeAdapter integration", () => {
	let adapter: NodeAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new NodeAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() spawns Node and returns a working DAPConnection", async () => {
		adapter = new NodeAdapter();
		const connection = await adapter.launch({ command: `node ${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
		expect(connection.launchArgs).toBeDefined();
		expect(connection.launchArgs?.type).toBe("pwa-node");
	});

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new NodeAdapter();
		const connection = await adapter.launch({ command: `node ${FIXTURE}` });

		// Write a DAP initialize request
		const req = { seq: 1, type: "request", command: "initialize", arguments: { adapterID: "test", clientID: "test", linesStartAt1: true, columnsStartAt1: true } };
		const json = JSON.stringify(req);
		connection.writer.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);

		// Read back the response
		const response = await new Promise<string>((resolve) => {
			connection.reader.once("data", (d: Buffer) => resolve(d.toString()));
		});
		expect(response).toContain("Content-Length");
	});

	it("dispose() kills the child processes", async () => {
		adapter = new NodeAdapter();
		const connection = await adapter.launch({ command: `node ${FIXTURE}` });
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
	});

	it("launch returns DAPConnection (script errors surface on DAP launch request, not adapter spawn)", async () => {
		// For Node.js, the js-debug DAP adapter launches independently of the script.
		// The script path is validated when the DAP launch request is sent, not during adapter.launch().
		adapter = new NodeAdapter();
		const connection = await adapter.launch({ command: "node /nonexistent/path/script.js" });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		// launchArgs carries the program path for the DAP launch request
		expect(connection.launchArgs?.program).toContain("/nonexistent/path/script.js");
	});
});

describe("NodeAdapter prerequisite check when Node missing", () => {
	it("has correct adapter properties", () => {
		const adapter = new NodeAdapter();
		expect(adapter.id).toBe("node");
		expect(adapter.fileExtensions).toContain(".js");
		expect(adapter.fileExtensions).toContain(".mjs");
		expect(adapter.displayName).toBe("Node.js (inspector)");
	});
});
