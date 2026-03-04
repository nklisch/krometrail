import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import type { Socket } from "node:net";
import { createConnection, createServer } from "node:net";
import { LaunchError } from "../core/errors.js";

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
