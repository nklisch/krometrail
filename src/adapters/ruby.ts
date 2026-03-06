import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import type { Socket } from "node:net";
import { isAbsolute, resolve as resolvePath } from "node:path";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, gracefulDispose } from "./helpers.js";

export class RubyAdapter implements DebugAdapter {
	id = "ruby";
	fileExtensions = [".rb"];
	displayName = "Ruby (rdbg)";

	private process: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for Ruby 3.1+ and rdbg (debug gem) availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		// Check rdbg
		const rdbgOk = await new Promise<boolean>((resolve) => {
			const proc = spawn("rdbg", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});

		if (!rdbgOk) {
			// Check if ruby itself is present for a better hint
			const rubyOk = await new Promise<boolean>((resolve) => {
				const proc = spawn("ruby", ["--version"], { stdio: "pipe" });
				proc.on("close", (code) => resolve(code === 0));
				proc.on("error", () => resolve(false));
			});

			if (!rubyOk) {
				return {
					satisfied: false,
					missing: ["ruby", "rdbg"],
					installHint: "Install Ruby 3.1+ from https://www.ruby-lang.org, then: gem install debug",
				};
			}

			return {
				satisfied: false,
				missing: ["rdbg"],
				installHint: "gem install debug (requires Ruby 3.1+)",
			};
		}

		return { satisfied: true };
	}

	/**
	 * Launch a Ruby script via rdbg in DAP TCP server mode.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const port = config.port ?? (await allocatePort());
		const { script, args } = parseRubyCommand(config.command);
		const cwd = config.cwd ?? process.cwd();

		// Validate script path exists
		const absScript = isAbsolute(script) ? script : resolvePath(cwd, script);
		await access(absScript).catch(() => {
			throw new LaunchError(`Script not found: ${absScript}`, "");
		});

		// rdbg --open=dap listens for a DAP client on a TCP socket.
		// The `-- ruby` part tells rdbg to launch Ruby with the given script.
		const child = spawn("rdbg", ["--open=dap", `--port=${port}`, "--host=127.0.0.1", "--", "ruby", absScript, ...args], {
			cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.process = child;

		const stderrBuffer: string[] = [];
		child.stderr?.on("data", (data: Buffer) => {
			stderrBuffer.push(data.toString());
		});
		child.stdout?.on("data", (data: Buffer) => {
			stderrBuffer.push(data.toString());
		});

		// Wait briefly for early spawn failure
		const earlyError = await new Promise<Error | null>((resolve) => {
			child.on("error", (err) => resolve(new LaunchError(`Failed to spawn rdbg: ${err.message}`, stderrBuffer.join(""))));
			child.on("close", (code) => {
				if (code !== null && code !== 0) {
					resolve(new LaunchError(`rdbg exited with code ${code}. output: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
				} else {
					resolve(null);
				}
			});
			setTimeout(() => resolve(null), 500);
		});

		if (earlyError) throw earlyError;

		// Poll TCP until rdbg is ready
		const socket = await connectTCP("127.0.0.1", port, 25, 200).catch((err) => {
			child.kill();
			throw new LaunchError(`Could not connect to rdbg on port ${port}: ${err.message}. output: ${stderrBuffer.join("")}`, stderrBuffer.join(""));
		});

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: child,
			launchArgs: {
				type: "rdbg",
				cwd,
				env: config.env ?? {},
				script: absScript,
				command: "ruby",
				args,
			},
		};
	}

	/**
	 * Attach to an already-running rdbg DAP server.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const host = config.host ?? "127.0.0.1";
		const port = config.port ?? 12345;

		const socket = await connectTCP(host, port);
		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
		};
	}

	async dispose(): Promise<void> {
		await gracefulDispose(this.socket, this.process);
		this.socket = null;
		this.process = null;
	}
}

/**
 * Parse a Ruby command string, stripping "ruby" prefix if present.
 * E.g., "ruby app.rb --verbose" => { script: "app.rb", args: ["--verbose"] }
 */
export function parseRubyCommand(command: string): { script: string; args: string[] } {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	// Strip "ruby" prefix
	if (parts[i] === "ruby") {
		i++;
	}

	const script = parts[i] ?? "";
	const args = parts.slice(i + 1);

	return { script, args };
}
