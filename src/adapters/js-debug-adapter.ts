import { exec } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import type { Socket } from "node:net";
import { join } from "node:path";
import { promisify } from "node:util";

import { AdapterInstallError, getErrorMessage } from "../core/errors.js";
import { downloadError, downloadToFile, ensureAdapterCacheDir, getAdapterCacheDir as getSharedAdapterCacheDir } from "./helpers.js";

const execAsync = promisify(exec);

/**
 * Pinned version of the js-debug DAP adapter to download.
 */
const JS_DEBUG_VERSION = "1.110.0";

/**
 * Path to the adapter cache directory.
 */
export function getAdapterCacheDir(): string {
	return getSharedAdapterCacheDir("js-debug");
}

/**
 * Path to the cached version file.
 */
function getVersionFilePath(): string {
	return join(getAdapterCacheDir(), "version.txt");
}

/**
 * Path to the DAP debug server entry point.
 * The tar.gz extracts into a "js-debug/" subdirectory.
 */
function getDapServerPath(): string {
	return join(getAdapterCacheDir(), "js-debug", "src", "dapDebugServer.js");
}

/**
 * Check if the js-debug adapter is available in the cache.
 */
export function isJsDebugAdapterCached(): boolean {
	return existsSync(getDapServerPath());
}

/**
 * Check if the cached version matches the expected version.
 */
async function isCachedVersionCurrent(): Promise<boolean> {
	const versionFile = getVersionFilePath();
	if (!existsSync(versionFile)) return false;
	try {
		const cached = (await readFile(versionFile, "utf8")).trim();
		return cached === JS_DEBUG_VERSION;
	} catch {
		return false;
	}
}

/**
 * Download and extract the js-debug DAP adapter.
 * Fetches the VSIX from GitHub releases and extracts with the system's unzip.
 */
export async function downloadJsDebugAdapter(): Promise<void> {
	const cacheDir = ensureAdapterCacheDir("js-debug");

	const vsixUrl = `https://github.com/microsoft/vscode-js-debug/releases/download/v${JS_DEBUG_VERSION}/js-debug-dap-v${JS_DEBUG_VERSION}.tar.gz`;
	const tarPath = join(cacheDir, "js-debug.tar.gz");

	try {
		await downloadToFile(vsixUrl, tarPath, "js-debug adapter");
	} catch (err) {
		throw downloadError("js-debug DAP adapter", JS_DEBUG_VERSION, vsixUrl, getDapServerPath(), err, `To install manually, download the adapter and place dapDebugServer.js at: ${getDapServerPath()}`);
	}

	// Extract the tar.gz into cacheDir
	try {
		await execAsync(`tar -xzf "${tarPath}" -C "${cacheDir}"`);
	} catch (err) {
		throw new AdapterInstallError("js-debug", `Failed to extract archive.\nError: ${getErrorMessage(err)}\nEnsure 'tar' is installed on your system.`);
	}

	// Verify the extracted file exists
	if (!existsSync(getDapServerPath())) {
		throw new AdapterInstallError("js-debug", `Extracted but dapDebugServer.js not found at expected path: ${getDapServerPath()}\nThe archive structure may have changed.`);
	}

	// Write version file
	await writeFile(getVersionFilePath(), JS_DEBUG_VERSION, "utf8");
}

/**
 * Get the path to the js-debug DAP adapter entry point.
 * Downloads the adapter if not already cached or version is stale.
 * Cache location: ~/.krometrail/adapters/js-debug/
 */
export async function getJsDebugAdapterPath(): Promise<string> {
	if (isJsDebugAdapterCached() && (await isCachedVersionCurrent())) {
		return getDapServerPath();
	}

	await downloadJsDebugAdapter();
	return getDapServerPath();
}

/**
 * Run a js-debug "parent" DAP session, handling both `launch` and `attach` flows.
 *
 * js-debug uses a two-session model: the parent session handles the initial
 * launch/attach and emits a `startDebugging` reverse request with the child
 * session configuration. We run the parent session here and return that config.
 *
 * Flow differences:
 * - `"launch"`: trigger is `type === "response" && command === "initialize"`;
 *   sends `configurationDone` then `launch`.
 * - `"attach"`: trigger is `type === "event" && event === "initialized"`;
 *   sends `configurationDone` then `attach`.
 *
 * @param socket  An already-connected socket to js-debug's DAP server.
 * @param options.flow  Whether to use the launch or attach path.
 * @param options.args  The DAP launch/attach arguments to send.
 * @param options.timeoutMs  How long to wait for `startDebugging` (default 10 000 ms).
 * @returns  The child session configuration from `startDebugging.arguments.configuration`.
 */
export function runJsDebugParentSession(
	socket: Socket,
	options: {
		flow: "launch" | "attach";
		args: Record<string, unknown>;
		timeoutMs?: number;
	},
): Promise<Record<string, unknown>> {
	const { flow, args, timeoutMs = 10_000 } = options;

	return new Promise((resolve, reject) => {
		let buf = Buffer.alloc(0);
		let seq = 1;
		let settled = false;

		const timer = setTimeout(() => {
			if (!settled) {
				settled = true;
				socket.removeListener("data", onData);
				reject(new Error(`js-debug: startDebugging not received within ${timeoutMs}ms (flow: ${flow})`));
			}
		}, timeoutMs);

		function sendRequest(cmd: string, cmdArgs?: Record<string, unknown>): void {
			const json = JSON.stringify({ seq: seq++, type: "request", command: cmd, arguments: cmdArgs });
			socket.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);
		}

		function sendResponse(requestSeq: number, cmd: string): void {
			const json = JSON.stringify({ seq: seq++, type: "response", request_seq: requestSeq, success: true, command: cmd, body: {} });
			socket.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);
		}

		function handleMessage(msg: Record<string, unknown>): void {
			const type = msg.type as string;
			const cmd = (msg.command ?? msg.event) as string;

			const isLaunchTrigger = flow === "launch" && type === "response" && cmd === "initialize";
			const isAttachTrigger = flow === "attach" && type === "event" && cmd === "initialized";

			if (isLaunchTrigger) {
				sendRequest("configurationDone");
				sendRequest("launch", args);
			} else if (isAttachTrigger) {
				sendRequest("configurationDone");
				sendRequest("attach", args);
			} else if (type === "request") {
				if (cmd === "startDebugging" && !settled) {
					settled = true;
					clearTimeout(timer);
					socket.removeListener("data", onData);
					sendResponse(msg.seq as number, "startDebugging");
					const config = (msg.arguments as Record<string, unknown>).configuration as Record<string, unknown>;
					resolve(config);
				} else {
					sendResponse(msg.seq as number, cmd);
				}
			}
		}

		function onData(chunk: Buffer): void {
			buf = Buffer.concat([buf, chunk]);
			while (true) {
				const headerEnd = buf.indexOf("\r\n\r\n");
				if (headerEnd === -1) break;
				const header = buf.subarray(0, headerEnd).toString();
				const match = header.match(/Content-Length:\s*(\d+)/i);
				if (!match) {
					buf = buf.subarray(headerEnd + 4);
					continue;
				}
				const len = Number.parseInt(match[1], 10);
				const start = headerEnd + 4;
				if (buf.length < start + len) break;
				const bodyStr = buf.subarray(start, start + len).toString();
				buf = buf.subarray(start + len);
				try {
					handleMessage(JSON.parse(bodyStr) as Record<string, unknown>);
				} catch {
					// ignore malformed JSON
				}
			}
		}

		socket.on("data", onData);
		socket.once("error", (err) => {
			if (!settled) {
				settled = true;
				clearTimeout(timer);
				reject(err);
			}
		});

		// Start the parent session.
		sendRequest("initialize", {
			clientID: "krometrail",
			adapterID: "krometrail",
			supportsVariableType: true,
			linesStartAt1: true,
			columnsStartAt1: true,
			supportsStartDebuggingRequest: true,
		});
	});
}
