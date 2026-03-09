import type { ChildProcess } from "node:child_process";
import { exec, spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { extname, join, resolve as resolvePath } from "node:path";
import { promisify } from "node:util";
import { getErrorMessage, LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { gracefulDispose } from "./helpers.js";

const execAsync = promisify(exec);

/**
 * Find the lldb-dap binary, checking PATH first, then platform-specific locations.
 * On macOS, tries `xcrun -f lldb-dap` as fallback.
 * On Linux, checks common Swift toolchain paths under /usr/libexec/swift/.
 * Returns the found path or null.
 */
export async function findLldbDap(): Promise<string | null> {
	// Check PATH first
	const onPath = await new Promise<boolean>((resolve) => {
		const proc = spawn("lldb-dap", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (onPath) return "lldb-dap";

	// macOS fallback: xcrun -f lldb-dap
	if (process.platform === "darwin") {
		try {
			const { stdout } = await execAsync("xcrun -f lldb-dap");
			const path = stdout.trim();
			if (path) return path;
		} catch {
			// not found via xcrun
		}
	}

	// Linux fallback: check common Swift toolchain paths
	if (process.platform === "linux") {
		try {
			const { stdout } = await execAsync("find /usr/libexec/swift -name lldb-dap -type f 2>/dev/null | head -1");
			const path = stdout.trim();
			if (path) return path;
		} catch {
			// not found
		}
	}

	return null;
}

export class SwiftAdapter implements DebugAdapter {
	id = "swift";
	fileExtensions = [".swift"];
	displayName = "Swift (lldb-dap)";

	private debuggerProcess: ChildProcess | null = null;

	/**
	 * Check for swiftc and lldb-dap availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		const swiftOk = await new Promise<boolean>((resolve) => {
			const proc = spawn("swiftc", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});

		if (!swiftOk) {
			return {
				satisfied: false,
				missing: ["swiftc"],
				installHint: "macOS: xcode-select --install. Linux: install from https://swift.org/download",
			};
		}

		const lldbDap = await findLldbDap();
		if (!lldbDap) {
			return {
				satisfied: false,
				missing: ["lldb-dap"],
				installHint: "Install Xcode (macOS) or Swift toolchain (Linux) from https://swift.org",
			};
		}

		return { satisfied: true };
	}

	/**
	 * Launch a Swift program via lldb-dap (stdin/stdout transport).
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const cwd = config.cwd ?? process.cwd();
		const parsed = parseSwiftCommand(config.command);
		let binaryPath: string;

		if (parsed.type === "source") {
			// Compile the source file with debug info
			const src = resolvePath(cwd, parsed.path);
			const outName = `agent-lens-swift-${Date.now()}`;
			const outPath = join(tmpdir(), outName);

			try {
				await execAsync(`swiftc -g -Onone "${src}" -o "${outPath}"`, {
					cwd,
					env: { ...process.env, ...config.env },
				});
			} catch (err) {
				throw new LaunchError(`Swift compilation failed: ${getErrorMessage(err)}`);
			}

			binaryPath = outPath;
		} else if (parsed.type === "spm") {
			// Build SPM project
			try {
				await execAsync("swift build", {
					cwd,
					env: { ...process.env, ...config.env },
				});
			} catch (err) {
				throw new LaunchError(`swift build failed: ${getErrorMessage(err)}`);
			}

			// Locate binary — use the target name or derive from directory name
			const targetName = parsed.path !== "." ? parsed.path : null;
			if (targetName) {
				binaryPath = resolvePath(cwd, ".build", "debug", targetName);
			} else {
				// Default: look for a binary with the same name as the directory
				const { basename } = await import("node:path");
				const dirName = basename(cwd);
				binaryPath = resolvePath(cwd, ".build", "debug", dirName);
			}
		} else {
			// Pre-built binary
			binaryPath = resolvePath(cwd, parsed.path);
		}

		const lldbDapPath = await findLldbDap();
		if (!lldbDapPath) {
			throw new LaunchError("lldb-dap not found. Install Xcode or Swift toolchain.");
		}

		// Spawn lldb-dap with stdin/stdout transport (same as C/C++ adapter)
		const child = spawn(lldbDapPath, [], {
			cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.debuggerProcess = child;

		const stderrBuffer: string[] = [];
		child.stderr?.on("data", (data: Buffer) => {
			stderrBuffer.push(data.toString());
		});

		// Wait briefly for early spawn failure
		const earlyError = await new Promise<Error | null>((resolve) => {
			child.on("error", (err) => resolve(new LaunchError(`Failed to spawn lldb-dap: ${err.message}`, stderrBuffer.join(""))));
			child.on("close", (code) => {
				if (code !== null && code !== 0) {
					resolve(new LaunchError(`lldb-dap exited with code ${code}. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
				} else {
					resolve(null);
				}
			});
			setTimeout(() => resolve(null), 500);
		});

		if (earlyError) throw earlyError;
		if (!child.stdout || !child.stdin) throw new LaunchError("lldb-dap stdio not available");

		return {
			reader: child.stdout,
			writer: child.stdin,
			process: child,
			launchArgs: {
				_dapFlow: "launch-first",
				program: binaryPath,
				cwd,
				env: config.env ?? {},
			},
		};
	}

	/**
	 * Attach to a running process via lldb-dap.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const lldbDapPath = await findLldbDap();
		if (!lldbDapPath) {
			throw new LaunchError("lldb-dap not found. Install Xcode or Swift toolchain.");
		}

		const child = spawn(lldbDapPath, [], {
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.debuggerProcess = child;

		const stderrBuffer: string[] = [];
		child.stderr?.on("data", (data: Buffer) => {
			stderrBuffer.push(data.toString());
		});

		const earlyError = await new Promise<Error | null>((resolve) => {
			child.on("error", (err) => resolve(new LaunchError(`Failed to spawn lldb-dap: ${err.message}`)));
			child.on("close", (code) => {
				if (code !== null && code !== 0) {
					resolve(new LaunchError(`lldb-dap exited with code ${code}. stderr: ${stderrBuffer.join("")}`));
				} else {
					resolve(null);
				}
			});
			setTimeout(() => resolve(null), 500);
		});

		if (earlyError) throw earlyError;
		if (!child.stdout || !child.stdin) throw new LaunchError("lldb-dap stdio not available");

		return {
			reader: child.stdout,
			writer: child.stdin,
			process: child,
			launchArgs: {
				request: "attach",
				pid: config.pid,
			},
		};
	}

	async dispose(): Promise<void> {
		await gracefulDispose(null, this.debuggerProcess);
		this.debuggerProcess = null;
	}
}

/**
 * Parse a Swift command string.
 * Handles: "swift run", "swiftc main.swift", "main.swift", "./binary"
 */
export function parseSwiftCommand(command: string): {
	type: "source" | "spm" | "binary";
	path: string;
	args: string[];
} {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	const first = parts[i] ?? "";

	// "swift run ..."
	if (first === "swift") {
		i++;
		if (parts[i] === "run") {
			i++;
			// optional target name
			const target = parts[i] && !(parts[i] as string).startsWith("-") ? (parts[i] as string) : ".";
			const afterTarget = parts[i] && !(parts[i] as string).startsWith("-") ? i + 1 : i;
			return { type: "spm", path: target, args: parts.slice(afterTarget) };
		}
		// bare "swift" — treat remainder as source
	}

	// "swiftc main.swift ..."
	if (first === "swiftc") {
		i++;
	}

	const path = parts[i] ?? "";
	const args = parts.slice(i + 1);
	const ext = extname(path).toLowerCase();

	if (ext === ".swift") {
		return { type: "source", path, args };
	}

	// Pre-built binary
	return { type: "binary", path, args };
}
