import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { CSharpAdapter } from "../../../src/adapters/csharp.js";
import { SKIP_NO_CSHARP } from "../../helpers/csharp-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/csharp/SimpleLoop.cs");

describe.skipIf(SKIP_NO_CSHARP)("CSharpAdapter integration", () => {
	let adapter: CSharpAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new CSharpAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() compiles and starts netcoredbg with working DAPConnection", async () => {
		adapter = new CSharpAdapter();
		const connection = await adapter.launch({ command: `${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
	}, 30_000);

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new CSharpAdapter();
		const connection = await adapter.launch({ command: `${FIXTURE}` });

		const req = { seq: 1, type: "request", command: "initialize", arguments: { adapterID: "test", clientID: "test", linesStartAt1: true, columnsStartAt1: true } };
		const json = JSON.stringify(req);
		connection.writer.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);

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
	}, 30_000);

	it("dispose() kills netcoredbg process", async () => {
		adapter = new CSharpAdapter();
		const connection = await adapter.launch({ command: `${FIXTURE}` });
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

	it("launch with bad .cs file produces clear error", async () => {
		adapter = new CSharpAdapter();
		await expect(adapter.launch({ command: "/nonexistent/path/App.cs" })).rejects.toThrow();
	});
});

describe("CSharpAdapter properties", () => {
	it("has correct adapter properties", () => {
		const adapter = new CSharpAdapter();
		expect(adapter.id).toBe("csharp");
		expect(adapter.fileExtensions).toEqual([".cs"]);
		expect(adapter.displayName).toBe("C# (netcoredbg)");
	});
});
