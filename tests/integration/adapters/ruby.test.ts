import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { RubyAdapter } from "../../../src/adapters/ruby.js";
import { SKIP_NO_RDBG } from "../../helpers/ruby-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/ruby/simple-loop.rb");

describe.skipIf(SKIP_NO_RDBG)("RubyAdapter integration", () => {
	let adapter: RubyAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new RubyAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() spawns rdbg and returns a working DAPConnection", async () => {
		adapter = new RubyAdapter();
		const connection = await adapter.launch({ command: `ruby ${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
	});

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new RubyAdapter();
		const connection = await adapter.launch({ command: `ruby ${FIXTURE}` });

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
	});

	it("dispose() kills the child process", async () => {
		adapter = new RubyAdapter();
		const connection = await adapter.launch({ command: `ruby ${FIXTURE}` });
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

	it("launch with bad script path produces clear error", async () => {
		adapter = new RubyAdapter();
		await expect(adapter.launch({ command: "ruby /nonexistent/path/script.rb" })).rejects.toThrow();
	});
});

describe("RubyAdapter properties", () => {
	it("has correct adapter properties", () => {
		const adapter = new RubyAdapter();
		expect(adapter.id).toBe("ruby");
		expect(adapter.fileExtensions).toEqual([".rb"]);
		expect(adapter.displayName).toBe("Ruby (rdbg)");
	});
});
