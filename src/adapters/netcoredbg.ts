import { exec } from "node:child_process";
import { existsSync } from "node:fs";
import { platform } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { AdapterInstallError, getErrorMessage } from "../core/errors.js";
import { downloadError, downloadToFile, ensureAdapterCacheDir, getAdapterCacheDir } from "./helpers.js";

const execAsync = promisify(exec);

/**
 * Pinned netcoredbg version.
 */
export const NETCOREDBG_VERSION = "3.1.3-1062";

/**
 * Returns the path to the netcoredbg cache directory.
 */
export function getNetcoredbgCachePath(): string {
	return getAdapterCacheDir("netcoredbg");
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
 * Download and cache the netcoredbg binary from Samsung/netcoredbg GitHub releases.
 * Returns the path to the binary.
 */
export async function downloadAndCacheNetcoredbg(): Promise<string> {
	const cacheDir = ensureAdapterCacheDir("netcoredbg");

	const url = getNetcoredbgDownloadUrl();
	const isZip = url.endsWith(".zip");
	const archivePath = join(cacheDir, isZip ? "netcoredbg.zip" : "netcoredbg.tar.gz");

	try {
		await downloadToFile(url, archivePath, "netcoredbg");
	} catch (err) {
		throw downloadError("netcoredbg", NETCOREDBG_VERSION, url, cacheDir, err, `To install manually, download the archive and extract netcoredbg to: ${cacheDir}`);
	}

	// Extract
	try {
		if (isZip) {
			await execAsync(`unzip -o "${archivePath}" -d "${cacheDir}"`);
		} else {
			await execAsync(`tar xzf "${archivePath}" --strip-components=1 -C "${cacheDir}"`);
		}
	} catch (err) {
		throw new AdapterInstallError("csharp", `Failed to extract archive.\nError: ${getErrorMessage(err)}\nEnsure 'tar' or 'unzip' is installed.`);
	}

	const binaryPath = getNetcoredbgBinaryPath();
	if (!existsSync(binaryPath)) {
		throw new AdapterInstallError("csharp", `Extracted but binary not found at: ${binaryPath}\nThe archive structure may have changed.`);
	}

	// Make binary executable on Unix
	if (platform() !== "win32") {
		await execAsync(`chmod +x "${binaryPath}"`);
	}

	return binaryPath;
}
