import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import type { Socket } from "node:net";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP } from "./helpers.js";

export class PythonAdapter implements DebugAdapter {
	id = "python";
	fileExtensions = [".py"];
	displayName = "Python (debugpy)";

	private process: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for python3 and debugpy availability.
	 * Spawns `python3 -m debugpy --version` and parses output.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		return new Promise((resolve) => {
			const proc = spawn("python3", ["-m", "debugpy", "--version"], { stdio: "pipe" });
			let _output = "";
			proc.stdout?.on("data", (d: Buffer) => {
				_output += d.toString();
			});
			proc.stderr?.on("data", (d: Buffer) => {
				_output += d.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) {
					resolve({ satisfied: true });
				} else {
					resolve({
						satisfied: false,
						missing: ["debugpy"],
						installHint: "pip install debugpy",
					});
				}
			});
			proc.on("error", () => {
				resolve({
					satisfied: false,
					missing: ["python3", "debugpy"],
					installHint: "Install python3 and pip install debugpy",
				});
			});
		});
	}

	/**
	 * Launch a Python script under debugpy.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const port = config.port ?? (await allocatePort());
		const { script, args } = parseCommand(config.command);

		const debugpyArgs = ["python3", "-m", "debugpy", "--listen", `0.0.0.0:${port}`, "--wait-for-client"];

		// Handle -m module case
		if (script === "-m") {
			debugpyArgs.push("-m", ...args);
		} else {
			debugpyArgs.push(script, ...args);
		}

		const [cmd, ...spawnArgs] = debugpyArgs;
		const stderrBuffer: string[] = [];

		const child = spawn(cmd, spawnArgs, {
			cwd: config.cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.process = child;

		// Wait for debugpy to start listening
		await new Promise<void>((resolve, reject) => {
			const timeout = setTimeout(() => {
				child.kill();
				reject(new LaunchError(`debugpy did not start within 10 seconds. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
			}, 10_000);

			child.stderr?.on("data", (data: Buffer) => {
				const text = data.toString();
				stderrBuffer.push(text);
				if (text.toLowerCase().includes("waiting") || text.toLowerCase().includes("listening")) {
					clearTimeout(timeout);
					resolve();
				}
			});

			child.on("error", (err) => {
				clearTimeout(timeout);
				reject(new LaunchError(`Failed to spawn debugpy: ${err.message}`, stderrBuffer.join("")));
			});

			child.on("close", (code) => {
				clearTimeout(timeout);
				if (code !== null && code !== 0) {
					reject(new LaunchError(`debugpy exited with code ${code}. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
				}
			});
		});

		// Connect TCP socket to debugpy
		const socket = await connectTCP("127.0.0.1", port);

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: child,
		};
	}

	/**
	 * Attach to an already-running debugpy instance.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const host = config.host ?? "127.0.0.1";
		const port = config.port ?? 5678;

		const socket = await connectTCP(host, port);

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
		};
	}

	/**
	 * Kill the child process and close the socket.
	 */
	async dispose(): Promise<void> {
		if (this.socket) {
			this.socket.destroy();
			this.socket = null;
		}
		if (this.process) {
			const proc = this.process;
			this.process = null;
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
 * Parse the script path and arguments from a command string.
 * E.g., "python app.py --verbose" => { script: "app.py", args: ["--verbose"] }
 * Strips leading "python3", "python", or "python3 -m debugpy ..." prefixes.
 */
export function parseCommand(command: string): { script: string; args: string[] } {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	// Strip python/python3 prefix
	if (parts[i] === "python3" || parts[i] === "python") {
		i++;
	}

	// Handle -m module case
	if (parts[i] === "-m") {
		// e.g. "python -m pytest tests/" => script: "-m", args: ["pytest", "tests/"]
		return { script: "-m", args: parts.slice(i + 1) };
	}

	// Bare script or remaining path
	const script = parts[i] ?? "";
	const args = parts.slice(i + 1);

	return { script, args };
}
