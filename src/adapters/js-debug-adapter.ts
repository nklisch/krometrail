import { exec } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { get as httpsGet } from "node:https";
import { homedir } from "node:os";
import { join } from "node:path";
import { pipeline } from "node:stream/promises";
import { promisify } from "node:util";

const execAsync = promisify(exec);

/**
 * Pinned version of the js-debug DAP adapter to download.
 */
const JS_DEBUG_VERSION = "1.110.0";

/**
 * Path to the adapter cache directory.
 */
export function getAdapterCacheDir(): string {
	return join(homedir(), ".agent-lens", "adapters", "js-debug");
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
 * Download a URL to a local file.
 */
function downloadToFile(url: string, destPath: string): Promise<void> {
	return new Promise((resolve, reject) => {
		const file = createWriteStream(destPath);
		const req = httpsGet(url, (response) => {
			if (response.statusCode && response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
				// Follow redirect
				file.destroy();
				downloadToFile(response.headers.location, destPath).then(resolve).catch(reject);
				return;
			}
			if (response.statusCode !== 200) {
				file.destroy();
				reject(new Error(`HTTP ${response.statusCode} downloading js-debug adapter from ${url}`));
				return;
			}
			pipeline(response, file).then(resolve).catch(reject);
		});
		req.on("error", reject);
	});
}

/**
 * Download and extract the js-debug DAP adapter.
 * Fetches the VSIX from GitHub releases and extracts with the system's unzip.
 */
export async function downloadJsDebugAdapter(): Promise<void> {
	const cacheDir = getAdapterCacheDir();
	mkdirSync(cacheDir, { recursive: true });

	const vsixUrl = `https://github.com/microsoft/vscode-js-debug/releases/download/v${JS_DEBUG_VERSION}/js-debug-dap-v${JS_DEBUG_VERSION}.tar.gz`;
	const tarPath = join(cacheDir, "js-debug.tar.gz");

	try {
		await downloadToFile(vsixUrl, tarPath);
	} catch (err) {
		throw new Error(
			`Failed to download js-debug DAP adapter v${JS_DEBUG_VERSION}.\n` +
				`URL: ${vsixUrl}\n` +
				`Error: ${err instanceof Error ? err.message : String(err)}\n` +
				`To install manually, download the adapter and place dapDebugServer.js at: ${getDapServerPath()}`,
		);
	}

	// Extract the tar.gz into cacheDir
	try {
		await execAsync(`tar -xzf "${tarPath}" -C "${cacheDir}"`);
	} catch (err) {
		throw new Error(`Failed to extract js-debug adapter.\n` + `Error: ${err instanceof Error ? err.message : String(err)}\n` + `Ensure 'tar' is installed on your system.`);
	}

	// Verify the extracted file exists
	if (!existsSync(getDapServerPath())) {
		throw new Error(`js-debug adapter extracted but dapDebugServer.js not found at expected path: ${getDapServerPath()}\n` + `The archive structure may have changed. Expected: ${getDapServerPath()}`);
	}

	// Write version file
	await writeFile(getVersionFilePath(), JS_DEBUG_VERSION, "utf8");
}

/**
 * Get the path to the js-debug DAP adapter entry point.
 * Downloads the adapter if not already cached or version is stale.
 * Cache location: ~/.agent-lens/adapters/js-debug/
 */
export async function getJsDebugAdapterPath(): Promise<string> {
	if (isJsDebugAdapterCached() && (await isCachedVersionCurrent())) {
		return getDapServerPath();
	}

	await downloadJsDebugAdapter();
	return getDapServerPath();
}
