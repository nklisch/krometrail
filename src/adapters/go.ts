import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import type { Socket } from "node:net";
import { homedir } from "node:os";
import { isAbsolute, join, resolve as resolvePath } from "node:path";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, spawnAndWait } from "./helpers.js";

/**
 * Build an augmented PATH that includes common Go binary install locations
 * ($GOPATH/bin, ~/go/bin) so dlv is found even when not in the shell PATH.
 */
function goEnv(extra?: Record<string, string>): NodeJS.ProcessEnv {
	const goBin = process.env.GOPATH ? join(process.env.GOPATH, "bin") : join(homedir(), "go", "bin");
	const currentPath = process.env.PATH ?? "";
	const augmentedPath = currentPath.includes(goBin) ? currentPath : `${goBin}:${currentPath}`;
	return { ...process.env, PATH: augmentedPath, ...extra };
}

export class GoAdapter implements DebugAdapter {
	id = "go";
	fileExtensions = [".go"];
	displayName = "Go (Delve)";

	private dlvProcess: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for Delve (dlv) availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		return new Promise((resolve) => {
			const proc = spawn("dlv", ["version"], { stdio: "pipe", env: goEnv() });
			proc.on("close", (code) => {
				if (code === 0) {
					resolve({ satisfied: true });
				} else {
					resolve({
						satisfied: false,
						missing: ["dlv"],
						installHint: "go install github.com/go-delve/delve/cmd/dlv@latest",
					});
				}
			});
			proc.on("error", () => {
				resolve({
					satisfied: false,
					missing: ["dlv"],
					installHint: "go install github.com/go-delve/delve/cmd/dlv@latest",
				});
			});
		});
	}

	/**
	 * Launch a Go program via Delve's DAP server.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const port = config.port ?? (await allocatePort());
		const cwd = config.cwd ?? process.cwd();
		const parsed = parseGoCommand(config.command);
		const absProgram = isAbsolute(parsed.program) ? parsed.program : resolvePath(cwd, parsed.program);

		// Validate the program path exists for exec mode (binary) and absolute file paths
		if (parsed.mode === "exec" || (parsed.mode === "debug" && isAbsolute(parsed.program))) {
			await access(absProgram).catch(() => {
				throw new LaunchError(`Program not found: ${absProgram}`, "");
			});
		}

		// Spawn Delve as a DAP server
		const { process: dlvProc } = await spawnAndWait({
			cmd: "dlv",
			args: ["dap", "--listen", `127.0.0.1:${port}`],
			cwd,
			env: goEnv(config.env),
			readyPattern: /DAP server listening at/i,
			timeoutMs: 15_000,
			label: "dlv",
		});

		this.dlvProcess = dlvProc;

		// Connect TCP with retries — Delve can take a moment to accept connections
		const socket = await connectTCP("127.0.0.1", port, 5, 300);
		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: dlvProc,
			launchArgs: {
				mode: parsed.mode,
				program: absProgram,
				args: parsed.args,
				cwd,
				buildFlags: parsed.buildFlags,
			},
		};
	}

	/**
	 * Attach to an already-running Go process via Delve.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const port = config.port ?? (await allocatePort());

		const { process: dlvProc } = await spawnAndWait({
			cmd: "dlv",
			args: ["dap", "--listen", `127.0.0.1:${port}`],
			env: goEnv(config.env),
			readyPattern: /DAP server listening at/i,
			timeoutMs: 10_000,
			label: "dlv",
		});

		this.dlvProcess = dlvProc;

		const socket = await connectTCP("127.0.0.1", port, 5, 300);
		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: dlvProc,
			launchArgs: {
				mode: "local",
				processId: config.pid,
			},
		};
	}

	/**
	 * Kill the Delve process and close the socket.
	 */
	async dispose(): Promise<void> {
		if (this.socket) {
			this.socket.destroy();
			this.socket = null;
		}
		if (this.dlvProcess) {
			const proc = this.dlvProcess;
			this.dlvProcess = null;
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
 * Parse a Go command string.
 * E.g., "go run main.go" => { mode: "debug", program: "main.go", args: [] }
 *       "./mybinary --flag" => { mode: "exec", program: "./mybinary", args: ["--flag"] }
 *       "go test ./..." => { mode: "test", program: "./...", args: [] }
 */
export function parseGoCommand(command: string): {
	mode: "debug" | "exec" | "test";
	program: string;
	buildFlags?: string[];
	args: string[];
} {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	// "go run ..." or "go test ..."
	if (parts[i] === "go") {
		i++;
		if (parts[i] === "run") {
			i++;
			// Collect build flags (start with -)
			const buildFlags: string[] = [];
			while (parts[i]?.startsWith("-")) {
				buildFlags.push(parts[i]);
				i++;
			}
			const program = parts[i] ?? "";
			const args = parts.slice(i + 1);
			return { mode: "debug", program, buildFlags: buildFlags.length > 0 ? buildFlags : undefined, args };
		}
		if (parts[i] === "test") {
			i++;
			const program = parts[i] ?? "./...";
			const args = parts.slice(i + 1);
			return { mode: "test", program, args };
		}
	}

	// Bare binary: "./mybinary --flag" or "/abs/path/to/binary"
	const program = parts[i] ?? "";
	const args = parts.slice(i + 1);
	return { mode: "exec", program, args };
}
