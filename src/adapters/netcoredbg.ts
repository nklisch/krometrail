import { exec } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync } from "node:fs";
import { get as httpsGet } from "node:https";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import { pipeline } from "node:stream/promises";
import { promisify } from "node:util";

const execAsync = promisify(exec);

/**
 * Pinned netcoredbg version.
 */
export const NETCOREDBG_VERSION = "3.1.2-1050";

/**
 * Returns the path to the netcoredbg cache directory.
 */
export function getNetcoredbgCachePath(): string {
	return join(homedir(), ".agent-lens", "adapters", "netcoredbg");
}

/**
 * Returns the path to the netcoredbg binary.
 */
export function getNetcoredbgBinaryPath(): string {
	const ext = platform() === "win32" ? ".exe" : "";
	return join(getNetcoredbgCachePath(), `netcoredbg${ext}`);
}

/**
 * Check if netcoredbg is already cached.
 */
export function isNetcoredbgCached(): boolean {
	return existsSync(getNetcoredbgBinaryPath());
}

/**
 * Returns the platform-specific download URL for the current platform.
 */
export function getNetcoredbgDownloadUrl(): string {
	const os = platform();
	let platformStr: string;
	let ext: string;

	if (os === "darwin") {
		platformStr = process.arch === "arm64" ? "osx-arm64" : "osx-amd64";
		ext = "tar.gz";
	} else if (os === "win32") {
		platformStr = "win64";
		ext = "zip";
	} else {
		platformStr = process.arch === "arm64" ? "linux-arm64" : "linux-amd64";
		ext = "tar.gz";
	}

	return `https://github.com/Samsung/netcoredbg/releases/download/${NETCOREDBG_VERSION}/netcoredbg-${platformStr}.${ext}`;
}

/**
 * Download a URL to a local file, following redirects.
 */
function downloadToFile(url: string, destPath: string): Promise<void> {
	return new Promise((resolve, reject) => {
		const file = createWriteStream(destPath);
		const req = httpsGet(url, (response) => {
			if (response.statusCode && response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
				file.destroy();
				downloadToFile(response.headers.location, destPath).then(resolve).catch(reject);
				return;
			}
			if (response.statusCode !== 200) {
				file.destroy();
				reject(new Error(`HTTP ${response.statusCode} downloading netcoredbg from ${url}`));
				return;
			}
			pipeline(response, file).then(resolve).catch(reject);
		});
		req.on("error", reject);
	});
}

/**
 * Download and cache the netcoredbg binary from Samsung/netcoredbg GitHub releases.
 * Returns the path to the binary.
 */
export async function downloadAndCacheNetcoredbg(): Promise<string> {
	const cacheDir = getNetcoredbgCachePath();
	mkdirSync(cacheDir, { recursive: true });

	const url = getNetcoredbgDownloadUrl();
	const isZip = url.endsWith(".zip");
	const archivePath = join(cacheDir, isZip ? "netcoredbg.zip" : "netcoredbg.tar.gz");

	try {
		await downloadToFile(url, archivePath);
	} catch (err) {
		throw new Error(
			`Failed to download netcoredbg v${NETCOREDBG_VERSION}.\n` +
				`URL: ${url}\n` +
				`Error: ${err instanceof Error ? err.message : String(err)}\n` +
				`To install manually, download the archive and extract netcoredbg to: ${cacheDir}`,
		);
	}

	// Extract
	try {
		if (isZip) {
			await execAsync(`unzip -o "${archivePath}" -d "${cacheDir}"`);
		} else {
			await execAsync(`tar xzf "${archivePath}" --strip-components=1 -C "${cacheDir}"`);
		}
	} catch (err) {
		throw new Error(`Failed to extract netcoredbg archive.\nError: ${err instanceof Error ? err.message : String(err)}\nEnsure 'tar' or 'unzip' is installed.`);
	}

	const binaryPath = getNetcoredbgBinaryPath();
	if (!existsSync(binaryPath)) {
		throw new Error(`netcoredbg extracted but binary not found at: ${binaryPath}\nThe archive structure may have changed.`);
	}

	// Make binary executable on Unix
	if (platform() !== "win32") {
		await execAsync(`chmod +x "${binaryPath}"`);
	}

	return binaryPath;
}
