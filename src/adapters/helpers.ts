import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { createWriteStream, mkdirSync } from "node:fs";
import { get as httpsGet } from "node:https";
import type { Socket } from "node:net";
import { createConnection, createServer } from "node:net";
import { pipeline } from "node:stream/promises";
import { getErrorMessage, LaunchError } from "../core/errors.js";
import { getKrometrailSubdir } from "../core/paths.js";
import type { PrerequisiteResult } from "./base.js";

/**
 * Spawn a command and check whether it exits successfully.
 * Returns `{ satisfied: true }` on exit code 0, or
 * `{ satisfied: false, missing, installHint }` on spawn error or non-zero exit.
 * Use this to implement `checkPrerequisites()` in adapters.
 */
export function checkCommand(opts: {
	cmd: string;
	args: string[];
	/** Full environment to pass to the child process. Merges over process.env if provided. */
	env?: NodeJS.ProcessEnv;
	missing: string[];
	installHint: string;
}): Promise<PrerequisiteResult> {
	return new Promise((resolve) => {
		const spawnEnv = opts.env !== undefined ? { ...process.env, ...opts.env } : undefined;
		const proc = spawn(opts.cmd, opts.args, { stdio: "pipe", env: spawnEnv });
		const fail = (): void => resolve({ satisfied: false, missing: opts.missing, installHint: opts.installHint });
		proc.on("close", (code) => (code === 0 ? resolve({ satisfied: true }) : fail()));
		proc.on("error", fail);
	});
}

/**
 * Spawn a command, collect its stdout+stderr, parse a version number from the output,
 * and optionally enforce a minimum version.
 *
 * Returns `{ satisfied: true, version }` when the command runs successfully and
 * (if `minVersion` is set) the parsed version meets the requirement.
 * Returns `{ satisfied: false, missing, installHint }` on spawn error, non-zero exit,
 * or a version below `minVersion`.
 */
export function checkCommandVersioned(opts: {
	cmd: string;
	args: string[];
	env?: NodeJS.ProcessEnv;
	/** Regex whose first capture group is the version integer to extract. */
	versionRegex: RegExp;
	/** If provided, the parsed version must be >= minVersion. */
	minVersion?: number;
	missing: string[];
	installHint: string | ((version: number) => string);
}): Promise<PrerequisiteResult & { version?: number }> {
	return new Promise((resolve) => {
		const spawnEnv = opts.env !== undefined ? { ...process.env, ...opts.env } : undefined;
		const proc = spawn(opts.cmd, opts.args, { stdio: "pipe", env: spawnEnv });
		let output = "";
		proc.stdout?.on("data", (d: Buffer) => {
			output += d.toString();
		});
		proc.stderr?.on("data", (d: Buffer) => {
			output += d.toString();
		});

		const fail = (): void => resolve({ satisfied: false, missing: opts.missing, installHint: typeof opts.installHint === "string" ? opts.installHint : opts.installHint(0) });

		proc.on("close", (code) => {
			if (code !== 0) {
				fail();
				return;
			}
			const match = output.match(opts.versionRegex);
			const version = match ? parseInt(match[1], 10) : 0;
			if (opts.minVersion !== undefined && version < opts.minVersion) {
				const hint = typeof opts.installHint === "function" ? opts.installHint(version) : opts.installHint;
				resolve({ satisfied: false, missing: opts.missing, installHint: hint });
				return;
			}
			resolve({ satisfied: true, version });
		});

		proc.on("error", fail);
	});
}

/**
 * Allocate a free TCP port by binding to port 0, reading the
 * assigned port, and immediately closing the server.
 */
export function allocatePort(): Promise<number> {
	return new Promise((resolve, reject) => {
		const server = createServer();
		server.listen(0, () => {
			const addr = server.address();
			if (!addr || typeof addr === "string") {
				server.close();
				reject(new Error("Failed to allocate port"));
				return;
			}
			const port = addr.port;
			server.close(() => resolve(port));
		});
		server.on("error", reject);
	});
}

export interface SpawnAndWaitOptions {
	/** Command to spawn */
	cmd: string;
	/** Arguments */
	args: string[];
	/** Working directory */
	cwd?: string;
	/** Environment variables (full env, merged over process.env if not already) */
	env?: NodeJS.ProcessEnv;
	/** Regex pattern to match on stdout or stderr indicating readiness */
	readyPattern: RegExp;
	/** Timeout in ms for the process to become ready */
	timeoutMs?: number;
	/** Label for error messages (e.g., "debugpy", "dlv") */
	label: string;
}

export interface SpawnResult {
	process: ChildProcess;
	/** Output accumulated before readiness (stdout + stderr combined) */
	stderrBuffer: string;
}

/**
 * Spawn a debugger process and wait for a readiness pattern on stdout or stderr.
 * Rejects with LaunchError on timeout, non-zero exit, or spawn failure.
 */
export function spawnAndWait(options: SpawnAndWaitOptions): Promise<SpawnResult> {
	const { cmd, args, cwd, env, readyPattern, timeoutMs = 10_000, label } = options;

	return new Promise((resolve, reject) => {
		let resolved = false;
		const outputChunks: string[] = [];

		const child = spawn(cmd, args, {
			cwd,
			env: { ...process.env, ...env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		const getOutput = () => outputChunks.join("");

		const timeout = setTimeout(() => {
			child.kill();
			reject(new LaunchError(`${label} did not start within ${timeoutMs}ms. output: ${getOutput()}`, getOutput()));
		}, timeoutMs);

		const onData = (data: Buffer) => {
			const text = data.toString();
			outputChunks.push(text);
			if (!resolved && readyPattern.test(text)) {
				resolved = true;
				clearTimeout(timeout);
				resolve({ process: child, stderrBuffer: getOutput() });
			}
		};

		child.stdout?.on("data", onData);
		child.stderr?.on("data", onData);

		child.on("error", (err) => {
			clearTimeout(timeout);
			reject(new LaunchError(`Failed to spawn ${label}: ${err.message}`, getOutput()));
		});

		child.on("close", (code) => {
			clearTimeout(timeout);
			if (!resolved && code !== null && code !== 0) {
				reject(new LaunchError(`${label} exited with code ${code}. output: ${getOutput()}`, getOutput()));
			}
		});
	});
}

/**
 * Wait briefly after spawning a debugger process to catch early failures.
 * Rejects with LaunchError if the process emits an error or exits with a non-zero
 * code before `timeoutMs` elapses. Resolves if the process is still alive after the timeout.
 * Use for debuggers that don't emit a readiness signal (use `spawnAndWait` for those).
 */
export function detectEarlySpawnFailure(child: ChildProcess, label: string, stderrBuffer: string[], timeoutMs = 300): Promise<void> {
	return new Promise<void>((resolve, reject) => {
		const timer = setTimeout(resolve, timeoutMs);
		child.on("error", (err) => {
			clearTimeout(timer);
			reject(new LaunchError(`Failed to spawn ${label}: ${err.message}`, stderrBuffer.join("")));
		});
		child.on("close", (code) => {
			clearTimeout(timer);
			if (code !== null && code !== 0) {
				reject(new LaunchError(`${label} exited with code ${code}. stderr: ${stderrBuffer.join("")}`, stderrBuffer.join("")));
			} else {
				resolve();
			}
		});
	});
}

/**
 * Gracefully dispose of an adapter: close the socket, then SIGTERM the process
 * (SIGKILL after 2 s if it doesn't exit). Same teardown pattern used by all adapters.
 */
export async function gracefulDispose(socket: Socket | null, proc: ChildProcess | null): Promise<void> {
	if (socket) {
		socket.destroy();
	}
	if (proc) {
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

/**
 * TCP retry presets for adapters grouped by startup speed.
 */
export const CONNECT_FAST = { maxRetries: 5, retryDelayMs: 300 } as const;
export const CONNECT_SLOW = { maxRetries: 25, retryDelayMs: 200 } as const;
export const CONNECT_PATIENT = { maxRetries: 30, retryDelayMs: 300 } as const;

/**
 * Connect a TCP socket to host:port with retry logic.
 * Retries up to `maxRetries` times with `retryDelayMs` between attempts.
 * Returns the connected Socket.
 */
export function connectTCP(host: string, port: number, maxRetries = 3, retryDelayMs = 200): Promise<Socket> {
	return new Promise((resolve, reject) => {
		let attempts = 0;

		const tryConnect = () => {
			const sock = createConnection({ host, port });

			sock.once("connect", () => {
				resolve(sock);
			});

			sock.once("error", (err) => {
				sock.destroy();
				attempts++;
				if (attempts < maxRetries) {
					setTimeout(tryConnect, retryDelayMs);
				} else {
					reject(err);
				}
			});
		};

		tryConnect();
	});
}

/**
 * Connect a TCP socket to host:port, killing the process and throwing LaunchError on failure.
 * Convenience wrapper around `connectTCP` for the common adapter pattern.
 */
export async function connectOrKill(proc: ChildProcess, host: string, port: number, retryConfig: { maxRetries: number; retryDelayMs: number }, label: string): Promise<Socket> {
	return connectTCP(host, port, retryConfig.maxRetries, retryConfig.retryDelayMs).catch((err) => {
		proc.kill();
		throw new LaunchError(`Could not connect to ${label} on port ${port}: ${(err as Error).message}`);
	});
}

/**
 * Returns the cache directory for a named adapter under ~/.krometrail/adapters/<adapterName>.
 */
export function getAdapterCacheDir(adapterName: string): string {
	return getKrometrailSubdir("adapters", adapterName);
}

/**
 * Ensures the adapter cache directory exists and returns its path.
 */
export function ensureAdapterCacheDir(adapterName: string): string {
	const dir = getAdapterCacheDir(adapterName);
	mkdirSync(dir, { recursive: true });
	return dir;
}

/**
 * Download a URL to a local file, following redirects.
 * `label` is used in the HTTP error message (e.g. "CodeLLDB", "java-debug-adapter").
 */
export function downloadToFile(url: string, destPath: string, label = "file"): Promise<void> {
	return new Promise((resolve, reject) => {
		const file = createWriteStream(destPath);
		const req = httpsGet(url, (response) => {
			if (response.statusCode && response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
				file.destroy();
				downloadToFile(response.headers.location, destPath, label).then(resolve).catch(reject);
				return;
			}
			if (response.statusCode !== 200) {
				file.destroy();
				reject(new Error(`HTTP ${response.statusCode} downloading ${label} from ${url}`));
				return;
			}
			pipeline(response, file).then(resolve).catch(reject);
		});
		req.on("error", reject);
	});
}

/**
 * Build a standardised download-failure error message.
 * `manualHint` overrides the default "place it at: <destPath>" hint.
 */
export function downloadError(tool: string, version: string, url: string, destPath: string, err: unknown, manualHint?: string): Error {
	const errMsg = getErrorMessage(err);
	const hint = manualHint ?? `To install manually, download and place it at: ${destPath}`;
	return new Error(`Failed to download ${tool} v${version}.\nURL: ${url}\nError: ${errMsg}\n${hint}`);
}
