import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { BrowserRecorder, type BrowserRecorderConfig } from "../../src/browser/recorder/index.js";
import { findChromeBinary, isChromeAvailable } from "./chrome-check.js";

const TEST_APP_DIR = resolve(import.meta.dirname, "../fixtures/browser/test-app");

export interface BrowserTestOptions {
	/** Path to fixture directory. Default: tests/fixtures/browser/test-app */
	fixturePath?: string;
	/** Framework state config for BrowserRecorder. Default: undefined (disabled) */
	frameworkState?: boolean | string[];
	/** Additional recorder config overrides */
	recorderConfig?: Partial<BrowserRecorderConfig>;
}

export interface BrowserTestContext {
	/** Port the test-app server is running on. */
	appPort: number;
	/** URL of the test-app. */
	appUrl: string;
	/** The CDP port Chrome is listening on. */
	cdpPort: number;
	/** The BrowserRecorder instance (started, recording). */
	recorder: BrowserRecorder;
	/** Temp directory for persistence data. */
	dataDir: string;
	/** MCP client connected to an agent-lens server using the same data dir. */
	mcpClient: Client;

	/** Navigate Chrome to a URL. Waits for load. */
	navigate(path: string): Promise<void>;
	/** Evaluate JS in the page context and return the string result. */
	evaluate(expression: string): Promise<string>;
	/** Click an element by selector (via JS click()). */
	click(selector: string): Promise<void>;
	/** Fill an input field by selector. */
	fill(selector: string, value: string): Promise<void>;
	/** Submit a form by selector. */
	submitForm(formSelector: string): Promise<void>;
	/** Wait for a specified number of milliseconds. */
	wait(ms: number): Promise<void>;
	/** Place a manual marker on the recorder. */
	placeMarker(label?: string): Promise<void>;
	/** Hit a test control endpoint on the server. */
	testControl(path: string): Promise<void>;
	/** Call an MCP tool and return the text content. */
	callTool(name: string, args: Record<string, unknown>): Promise<string>;
	/** Stop recording, flush persistence, and prepare for MCP queries. */
	finishRecording(): Promise<void>;

	/** Tear down everything. Called in afterAll. */
	cleanup(): Promise<void>;
}

/**
 * Set up the full browser test environment.
 *
 * 1. Start the fixture server on a random port (default: test-app)
 * 2. Launch headless Chrome with CDP
 * 3. Create & start BrowserRecorder with persistence to a temp dir
 * 4. Create an MCP client pointing at the same temp data dir (in finishRecording)
 *
 * Returns a BrowserTestContext with driving utilities.
 */
export async function setupBrowserTest(options?: BrowserTestOptions): Promise<BrowserTestContext> {
	const fixtureDir = options?.fixturePath ?? TEST_APP_DIR;

	// Auto-install fixture dependencies if needed
	const pkgJson = join(fixtureDir, "package.json");
	const nodeModules = join(fixtureDir, "node_modules");
	if (existsSync(pkgJson) && !existsSync(nodeModules)) {
		const installResult = Bun.spawnSync(["bun", "install"], { cwd: fixtureDir, stdout: "pipe", stderr: "pipe" });
		if (installResult.exitCode !== 0) {
			throw new Error(`bun install failed in ${fixtureDir}: ${installResult.stderr}`);
		}
	}

	// 1. Start fixture server
	const { port: appPort, process: serverProc } = await startFixtureServer(fixtureDir);
	const appUrl = `http://localhost:${appPort}`;

	// 2. Launch headless Chrome
	const { port: cdpPort, chromeCleanup } = await launchChrome();

	// 3. Create temp data dir for persistence
	const dataDir = mkdtempSync(join(tmpdir(), "agent-lens-browser-e2e-"));
	mkdirSync(join(dataDir, "recordings"), { recursive: true });

	// 4. Navigate Chrome to the app first so there's a tab to record
	await cdpNavigate(cdpPort, `${appUrl}/`);
	await wait(500);

	// 5. Create and start BrowserRecorder
	const recorderConfig: BrowserRecorderConfig = {
		port: cdpPort,
		attach: true,
		allTabs: false,
		persistence: { dataDir },
		screenshots: { onNavigation: true, onMarker: true, intervalMs: 0 },
		...(options?.frameworkState !== undefined ? { frameworkState: options.frameworkState } : {}),
		...options?.recorderConfig,
	};
	const recorder = new BrowserRecorder(recorderConfig);
	await recorder.start();

	// --- Build the context ---
	const ctx: BrowserTestContext = {
		appPort,
		appUrl,
		cdpPort,
		recorder,
		dataDir,
		mcpClient: null as unknown as Client, // Initialized in finishRecording

		async navigate(path: string) {
			const url = path.startsWith("http") ? path : `${appUrl}${path}`;
			await cdpSendToPrimaryTab(cdpPort, "Page.navigate", { url });
			await wait(800);
		},

		async evaluate(expression: string): Promise<string> {
			const result = await cdpSendToPrimaryTab(cdpPort, "Runtime.evaluate", {
				expression,
				returnByValue: true,
			});
			return (result as { result?: { value?: unknown } })?.result?.value != null ? String((result as { result: { value: unknown } }).result.value) : "";
		},

		async click(selector: string) {
			await ctx.evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
			await wait(300);
		},

		async fill(selector: string, value: string) {
			await ctx.evaluate(`
				(() => {
					const el = document.querySelector(${JSON.stringify(selector)});
					el.value = '';
					el.focus();
					el.value = ${JSON.stringify(value)};
					el.dispatchEvent(new Event('input', { bubbles: true }));
					el.dispatchEvent(new Event('change', { bubbles: true }));
				})()
			`);
			await wait(100);
		},

		async submitForm(formSelector: string) {
			// requestSubmit() fires the submit event properly (unlike dispatchEvent or button.click())
			await ctx.evaluate(`document.querySelector(${JSON.stringify(formSelector)}).requestSubmit()`);
			await wait(500);
		},

		async wait(ms: number) {
			await new Promise<void>((r) => setTimeout(r, ms));
		},

		async placeMarker(label?: string) {
			await recorder.placeMarker(label);
		},

		async testControl(path: string) {
			await fetch(`${appUrl}${path}`);
		},

		async callTool(name: string, args: Record<string, unknown>): Promise<string> {
			if (!ctx.mcpClient) throw new Error("Must call finishRecording() before calling MCP tools");
			const result = await ctx.mcpClient.callTool({ name, arguments: args });
			const content = result.content as Array<{ type: string; text?: string }>;
			if (result.isError) {
				const text = content
					.filter((c) => c.type === "text")
					.map((c) => c.text ?? "")
					.join("\n");
				throw new Error(`Tool '${name}' returned error: ${text}`);
			}
			return content
				.filter((c) => c.type === "text")
				.map((c) => c.text ?? "")
				.join("\n");
		},

		async finishRecording() {
			await recorder.stop();
			// Start MCP server pointing at the same data dir
			const transport = new StdioClientTransport({
				command: "bun",
				args: ["run", resolve(import.meta.dirname, "../../src/mcp/index.ts")],
				env: {
					...process.env,
					AGENT_LENS_BROWSER_DATA_DIR: dataDir,
				},
			});
			const { Client } = await import("@modelcontextprotocol/sdk/client/index.js");
			ctx.mcpClient = new Client({ name: "browser-e2e-test", version: "1.0.0" }, { capabilities: {} });
			await ctx.mcpClient.connect(transport);
		},

		async cleanup() {
			try {
				await recorder.stop().catch(() => {});
			} catch {}
			try {
				if (ctx.mcpClient) await ctx.mcpClient.close().catch(() => {});
			} catch {}
			await chromeCleanup();
			try {
				serverProc.kill("SIGTERM");
			} catch {}
			await wait(500);
			try {
				rmSync(dataDir, { recursive: true, force: true });
			} catch {}
		},
	};

	return ctx;
}

// --- Internal helpers ---

async function startFixtureServer(fixtureDir: string): Promise<{ port: number; process: ChildProcess }> {
	const proc = spawn("bun", ["run", join(fixtureDir, "server.ts"), "0"], { stdio: ["ignore", "pipe", "pipe"] });
	const port = await new Promise<number>((resolve, reject) => {
		let output = "";
		proc.stdout!.on("data", (chunk: Buffer) => {
			output += chunk.toString();
			const match = output.match(/READY:(\d+)/);
			if (match) resolve(Number.parseInt(match[1], 10));
		});
		proc.on("error", reject);
		setTimeout(() => reject(new Error("Test server startup timeout")), 15_000);
	});
	return { port, process: proc };
}

async function launchChrome(): Promise<{ port: number; chromeCleanup: () => Promise<void> }> {
	const binary = await findChromeBinary();
	if (!binary) throw new Error("Chrome not found — install Chrome or Chromium to run browser e2e tests");

	const port = 9400 + Math.floor(Math.random() * 100);
	const profileDir = mkdtempSync(join(tmpdir(), "agent-lens-e2e-chrome-"));

	const proc = spawn(
		binary,
		[`--remote-debugging-port=${port}`, `--user-data-dir=${profileDir}`, "--no-first-run", "--no-default-browser-check", "--headless=new", "--disable-gpu", "--disable-dev-shm-usage"],
		{ stdio: "ignore" },
	);

	// Wait for Chrome to be ready
	const deadline = Date.now() + 15_000;
	while (Date.now() < deadline) {
		try {
			const resp = await fetch(`http://localhost:${port}/json/version`);
			if (resp.ok) break;
		} catch {
			await new Promise<void>((r) => setTimeout(r, 300));
		}
	}

	const chromeCleanup = async () => {
		proc.kill("SIGTERM");
		await new Promise<void>((r) => setTimeout(r, 500));
		try {
			rmSync(profileDir, { recursive: true, force: true });
		} catch {}
	};

	return { port, chromeCleanup };
}

/** Navigate the first page tab to a URL via CDP WebSocket. */
async function cdpNavigate(cdpPort: number, url: string): Promise<void> {
	await cdpSendToPrimaryTab(cdpPort, "Page.navigate", { url });
	await new Promise<void>((r) => setTimeout(r, 1000));
}

/** Send a CDP command to the primary page tab via a temporary WebSocket. */
async function cdpSendToPrimaryTab(cdpPort: number, method: string, params: Record<string, unknown>): Promise<unknown> {
	const resp = await fetch(`http://localhost:${cdpPort}/json/list`);
	const targets = (await resp.json()) as Array<{ id: string; webSocketDebuggerUrl: string; type: string }>;
	const page = targets.find((t) => t.type === "page");
	if (!page) throw new Error("No page target found in Chrome");

	// Open a temporary WebSocket to send the command
	return new Promise((resolve, reject) => {
		const ws = new WebSocket(page.webSocketDebuggerUrl);
		const id = Math.floor(Math.random() * 100_000);
		ws.onopen = () => {
			ws.send(JSON.stringify({ id, method, params }));
		};
		ws.onmessage = (e) => {
			const msg = JSON.parse(String(e.data));
			if (msg.id === id) {
				ws.close();
				if (msg.error) reject(new Error(msg.error.message));
				else resolve(msg.result);
			}
		};
		ws.onerror = () => reject(new Error(`WebSocket error sending ${method}`));
		setTimeout(() => {
			ws.close();
			reject(new Error(`CDP command timeout: ${method}`));
		}, 10_000);
	});
}

function wait(ms: number): Promise<void> {
	return new Promise((r) => setTimeout(r, ms));
}

export { isChromeAvailable } from "./chrome-check.js";
