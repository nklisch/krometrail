import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import type { Socket } from "node:net";
import { isAbsolute, resolve as resolvePath } from "node:path";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, gracefulDispose } from "./helpers.js";

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
	 * Launch a Python script via debugpy.adapter (DAP server mode).
	 * The adapter accepts `launch` requests and starts the debuggee as a child process.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const port = config.port ?? (await allocatePort());
		const { script, args } = parseCommand(config.command);
		const cwd = config.cwd ?? process.cwd();

		// Validate script path exists before spawning (skip for -m module and -c code modes)
		if (script !== "-m" && script !== "-c") {
			const absScript = isAbsolute(script) ? script : resolvePath(cwd, script);
			await access(absScript).catch(() => {
				throw new LaunchError(`Script not found: ${absScript}`, "");
			});
		}

		// Start debugpy in DAP adapter server mode — this mode accepts `launch` requests
		// and starts the debuggee as a subprocess (unlike --listen which requires `attach`).
		const child = spawn("python3", ["-m", "debugpy.adapter", "--host", "127.0.0.1", "--port", String(port)], {
			cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.process = child;

		const stderrBuffer: string[] = [];
		child.stderr?.on("data", (data: Buffer) => {
			stderrBuffer.push(data.toString());
		});

		// Handle early spawn failure
		const earlyError = await new Promise<Error | null>((resolve) => {
			child.on("error", (err) => resolve(new LaunchError(`Failed to spawn debugpy: ${err.message}`, stderrBuffer.join(""))));
			child.on("close", (code) => {
				if (code !== null && code !== 0) {
					resolve(new LaunchError(`debugpy exited with code ${code}. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
				} else {
					resolve(null);
				}
			});
			setTimeout(() => resolve(null), 300);
		});

		if (earlyError) throw earlyError;

		// Poll TCP until the adapter server is ready
		const socket = await connectTCP("127.0.0.1", port, 25, 200).catch((err) => {
			child.kill();
			throw new LaunchError(`Could not connect to debugpy on port ${port}: ${err.message}. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join(""));
		});

		this.socket = socket;

		// Build DAP launch arguments for the debuggee
		const absScript = script !== "-m" ? (isAbsolute(script) ? script : resolvePath(cwd, script)) : undefined;
		const launchArgs: Record<string, unknown> = {
			type: "python",
			cwd,
			env: config.env ?? {},
			// _dapFlow signals session-manager to use the debugpy.adapter protocol:
			// send `launch` first (without awaiting), wait for `initialized`, then setBreakpoints/configurationDone.
			_dapFlow: "launch-first",
		};

		if (script === "-m") {
			launchArgs.module = args[0];
			launchArgs.args = args.slice(1);
		} else if (script === "-c") {
			launchArgs.code = args.join(" ");
		} else {
			launchArgs.program = absScript;
			launchArgs.args = args;
		}

		return {
			reader: socket,
			writer: socket,
			process: child,
			launchArgs,
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
			launchArgs: {
				// debugpy --listen mode: does not send `initialized` until `attach` is received.
				// Use launch-first flow so session-manager sends `attach` before awaiting `initialized`.
				_dapFlow: "launch-first",
				// debugpy requires non-empty attach arguments (empty {} is falsy in Python and
				// causes AttachRequest.__init__() to fail). Pass connect info as the args.
				connect: { host, port },
			},
		};
	}

	/**
	 * Kill the child process and close the socket.
	 */
	async dispose(): Promise<void> {
		await gracefulDispose(this.socket, this.process);
		this.socket = null;
		this.process = null;
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

	// Handle -c inline code case
	if (parts[i] === "-c") {
		// e.g. "python -c 'import sys; print(sys.path)'" => script: "-c", args: ["import sys; ..."]
		return { script: "-c", args: parts.slice(i + 1) };
	}

	// Bare script or remaining path
	const script = parts[i] ?? "";
	const args = parts.slice(i + 1);

	return { script, args };
}
