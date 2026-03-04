import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import type { Socket } from "node:net";
import { resolve as resolvePath } from "node:path";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP } from "./helpers.js";
import { getJsDebugAdapterPath } from "./js-debug-adapter.js";

export class NodeAdapter implements DebugAdapter {
	id = "node";
	fileExtensions = [".js", ".mjs", ".cjs"];
	displayName = "Node.js (inspector)";

	private adapterProcess: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for Node.js 18+ availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		return new Promise((resolve) => {
			const proc = spawn("node", ["--version"], { stdio: "pipe" });
			let output = "";
			proc.stdout?.on("data", (d: Buffer) => {
				output += d.toString();
			});
			proc.on("close", (code) => {
				if (code !== 0) {
					resolve({
						satisfied: false,
						missing: ["node"],
						installHint: "Install Node.js 18+ from https://nodejs.org",
					});
					return;
				}
				// Parse version: "v20.11.0" => 20
				const match = output.trim().match(/^v(\d+)/);
				const major = match ? parseInt(match[1], 10) : 0;
				if (major < 18) {
					resolve({
						satisfied: false,
						missing: ["node"],
						installHint: `Node.js ${major} is too old. Install Node.js 18+ from https://nodejs.org`,
					});
				} else {
					resolve({ satisfied: true });
				}
			});
			proc.on("error", () => {
				resolve({
					satisfied: false,
					missing: ["node"],
					installHint: "Install Node.js 18+ from https://nodejs.org",
				});
			});
		});
	}

	/**
	 * Launch a Node.js script via the js-debug DAP adapter.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const dapAdapterPath = await getJsDebugAdapterPath();
		const port = config.port ?? (await allocatePort());
		const { script, args } = parseNodeCommand(config.command);
		const cwd = config.cwd ?? process.cwd();
		const absScript = resolvePath(cwd, script);

		// Spawn the js-debug DAP adapter server, binding to 127.0.0.1 to avoid IPv6
		const adapterProc = spawn("node", [dapAdapterPath, String(port), "127.0.0.1"], {
			cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.adapterProcess = adapterProc;

		// Wait for the DAP adapter to start listening
		await new Promise<void>((resolve, reject) => {
			const stderrChunks: string[] = [];

			const timeout = setTimeout(() => {
				adapterProc.kill();
				reject(new LaunchError(`js-debug DAP adapter did not start within 10 seconds. output: ${stderrChunks.join("")}`, stderrChunks.join("")));
			}, 10_000);

			const onData = (data: Buffer) => {
				const text = data.toString();
				stderrChunks.push(text);
				// js-debug adapter prints "Debug server listening at ..." on stdout
				if (/listening/i.test(text)) {
					clearTimeout(timeout);
					resolve();
				}
			};

			adapterProc.stdout?.on("data", onData);
			adapterProc.stderr?.on("data", onData);

			adapterProc.on("error", (err) => {
				clearTimeout(timeout);
				reject(new LaunchError(`Failed to spawn js-debug adapter: ${err.message}`, stderrChunks.join("")));
			});

			adapterProc.on("close", (code) => {
				clearTimeout(timeout);
				if (code !== null && code !== 0) {
					reject(new LaunchError(`js-debug adapter exited with code ${code}. output: ${stderrChunks.join("")}`, stderrChunks.join("")));
				}
			});
		});

		// Connect TCP to the js-debug DAP adapter
		const socket = await connectTCP("127.0.0.1", port, 5, 300);
		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs: {
				type: "pwa-node",
				program: absScript,
				args,
				cwd,
				sourceMaps: true,
			},
		};
	}

	/**
	 * Attach to an already-running Node.js inspector via the js-debug adapter.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const dapAdapterPath = await getJsDebugAdapterPath();
		const dapPort = await allocatePort();

		const adapterProc = spawn("node", [dapAdapterPath, String(dapPort), "127.0.0.1"], {
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.adapterProcess = adapterProc;

		// Wait for the adapter to start listening
		await new Promise<void>((resolve, reject) => {
			const timeout = setTimeout(() => {
				adapterProc.kill();
				reject(new LaunchError("js-debug DAP adapter did not start within 10 seconds"));
			}, 10_000);
			const onData = (data: Buffer) => {
				if (/listening/i.test(data.toString())) {
					clearTimeout(timeout);
					resolve();
				}
			};
			adapterProc.stdout?.on("data", onData);
			adapterProc.stderr?.on("data", onData);
			adapterProc.on("error", (err) => {
				clearTimeout(timeout);
				reject(err);
			});
		});

		const socket = await connectTCP("127.0.0.1", dapPort, 5, 300);
		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs: {
				type: "pwa-node",
				request: "attach",
				port: config.port ?? 9229,
				host: config.host ?? "127.0.0.1",
				...(config.pid ? { processId: config.pid } : {}),
			},
		};
	}

	/**
	 * Kill the js-debug adapter process and close the socket.
	 */
	async dispose(): Promise<void> {
		if (this.socket) {
			this.socket.destroy();
			this.socket = null;
		}
		if (this.adapterProcess) {
			const proc = this.adapterProcess;
			this.adapterProcess = null;
			proc.kill("SIGTERM");
			await new Promise<void>((resolve) => {
				const timeout = setTimeout(() => {
					proc.kill("SIGKILL");
					resolve();
				}, 2_000);
				proc.once("close", () => {
					clearTimeout(timeout);
					resolve();
				});
			});
		}
	}
}

/**
 * Parse a Node.js command string, stripping "node" prefix if present.
 * E.g., "node app.js --verbose" => { script: "app.js", args: ["--verbose"] }
 * Handles "node" and "node --" prefixes.
 */
export function parseNodeCommand(command: string): { script: string; args: string[] } {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	// Strip "node" prefix
	if (parts[i] === "node") {
		i++;
	}

	// Strip "--" separator if present
	if (parts[i] === "--") {
		i++;
	}

	// Strip --inspect* flags (these are handled by the adapter)
	while (parts[i]?.startsWith("--inspect")) {
		i++;
	}

	const script = parts[i] ?? "";
	const args = parts.slice(i + 1);

	return { script, args };
}
