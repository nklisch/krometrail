import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { KotlinAdapter } from "../../../src/adapters/kotlin.js";
import { SKIP_NO_KOTLIN } from "../../helpers/kotlin-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/kotlin/SimpleLoop.kt");

describe.skipIf(SKIP_NO_KOTLIN)("KotlinAdapter integration", () => {
	let adapter: KotlinAdapter;

	afterEach(async () => {
		try {
			await adapter?.dispose();
		} catch {
			// ignore
		}
	});

	it("checkPrerequisites() returns satisfied: true", async () => {
		adapter = new KotlinAdapter();
		const result = await adapter.checkPrerequisites();
		expect(result.satisfied).toBe(true);
	});

	it("launch() compiles .kt and returns working DAPConnection", async () => {
		adapter = new KotlinAdapter();
		const connection = await adapter.launch({ command: `${FIXTURE}` });
		expect(connection.reader).toBeDefined();
		expect(connection.writer).toBeDefined();
		expect(connection.process).toBeDefined();
		expect(connection.process?.pid).toBeGreaterThan(0);
	}, 60_000);

	it("DAPConnection can send/receive DAP messages", async () => {
		adapter = new KotlinAdapter();
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
	}, 60_000);

	it("dispose() kills java-debug-adapter process", async () => {
		adapter = new KotlinAdapter();
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
	}, 60_000);

	it("launch with bad .kt file produces clear error", async () => {
		adapter = new KotlinAdapter();
		await expect(adapter.launch({ command: "/nonexistent/path/Main.kt" })).rejects.toThrow();
	});
});

describe("KotlinAdapter properties", () => {
	it("has correct adapter properties", () => {
		const adapter = new KotlinAdapter();
		expect(adapter.id).toBe("kotlin");
		expect(adapter.fileExtensions).toEqual([".kt"]);
		expect(adapter.displayName).toBe("Kotlin (java-debug-adapter)");
	});
});
