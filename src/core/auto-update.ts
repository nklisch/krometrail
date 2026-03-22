import { chmodSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import pkg from "../../package.json";
import { getKrometrailDir } from "./paths.js";

// ---------------------------------------------------------------------------
// Unit 1: Install Type Detection
// ---------------------------------------------------------------------------

/** How krometrail was installed */
export type InstallType = "binary" | "npx" | "bunx" | "global-npm" | "dev";

export interface InstallInfo {
	type: InstallType;
	/** Absolute path to the krometrail binary (for binary installs) */
	binaryPath: string | undefined;
	/** Package manager command (for global-npm installs) */
	packageManager: "npm" | "bun" | undefined;
}

/**
 * Pure detection logic — accepts the process globals as parameters so it can be
 * tested without mocking globals.
 */
export function detectInstallTypeFrom(execPath: string, argv: string[], env: NodeJS.ProcessEnv): InstallInfo {
	const script = argv[1] ?? "";

	// 1. Dev mode: running the TypeScript source directly
	if (script.endsWith("src/cli/index.ts") || script.endsWith("src/mcp/index.ts")) {
		return { type: "dev", binaryPath: undefined, packageManager: undefined };
	}

	// 2. Compiled binary: execPath ends with /krometrail (or .exe) and is not
	//    inside a package manager cache or node_modules directory.
	const execName = execPath.split("/").pop() ?? "";
	const isBinaryName = execName === "krometrail" || execName === "krometrail.exe";
	const isInsideCache = execPath.includes("node_modules") || execPath.includes("/.bun/") || execPath.includes("/.npm/");
	if (isBinaryName && !isInsideCache) {
		return { type: "binary", binaryPath: execPath, packageManager: undefined };
	}

	// 3. Global bun install: must come before generic bunx check because
	//    /.bun/install/global/ also contains /.bun/
	if (script.includes("/.bun/install/global/")) {
		return { type: "global-npm", binaryPath: undefined, packageManager: "bun" };
	}

	// 4. bunx: script path contains /.bun/ (bun's cache dir)
	if (script.includes("/.bun/")) {
		return { type: "bunx", binaryPath: undefined, packageManager: undefined };
	}

	// 5. npx: npm_execpath env var is set, or script is in npm's _npx cache
	if (env.npm_execpath || script.includes("/.npm/_npx/")) {
		return { type: "npx", binaryPath: undefined, packageManager: undefined };
	}

	// 6. Global npm package
	if (script.includes("/node_modules/krometrail/")) {
		return { type: "global-npm", binaryPath: undefined, packageManager: "npm" };
	}

	// 6. Default: assume binary
	return { type: "binary", binaryPath: execPath, packageManager: undefined };
}

/**
 * Detect how krometrail was installed by inspecting process.execPath,
 * process.argv, and environment variables.
 */
export function detectInstallType(): InstallInfo {
	return detectInstallTypeFrom(process.execPath, process.argv, process.env);
}

// ---------------------------------------------------------------------------
// Unit 2: Version Check (GitHub API)
// ---------------------------------------------------------------------------

export interface VersionCheckResult {
	/** Latest version tag from GitHub (e.g. "v0.3.0") */
	latestVersion: string;
	/** Whether an update is available */
	updateAvailable: boolean;
	/** Current version from package.json */
	currentVersion: string;
}

/** Compare two semver strings; returns true if remote is strictly newer than local. */
export function isNewer(remote: string, local: string): boolean {
	const r = remote.replace(/^v/, "").split(".").map(Number);
	const l = local.replace(/^v/, "").split(".").map(Number);
	for (let i = 0; i < 3; i++) {
		if ((r[i] ?? 0) > (l[i] ?? 0)) return true;
		if ((r[i] ?? 0) < (l[i] ?? 0)) return false;
	}
	return false;
}

/**
 * Check GitHub releases API for the latest version.
 * Returns null if the check fails or times out.
 * Uses a 5-second timeout to avoid blocking.
 */
export async function checkLatestVersion(): Promise<VersionCheckResult | null> {
	const currentVersion = pkg.version;
	try {
		const response = await fetch("https://api.github.com/repos/nklisch/krometrail/releases/latest", {
			headers: { "User-Agent": `krometrail/${currentVersion}` },
			signal: AbortSignal.timeout(5000),
		});
		if (!response.ok) return null;
		const data = (await response.json()) as { tag_name?: string };
		const latestVersion = data.tag_name;
		if (!latestVersion) return null;
		return {
			latestVersion,
			updateAvailable: isNewer(latestVersion, currentVersion),
			currentVersion,
		};
	} catch {
		return null;
	}
}

// ---------------------------------------------------------------------------
// Unit 3: Throttle Gate
// ---------------------------------------------------------------------------

const THROTTLE_MS = 60 * 60 * 1000; // 1 hour

function defaultThrottleFile(): string {
	return join(getKrometrailDir(), "last-update-check");
}

/**
 * Returns true if enough time has passed since the last check.
 * Updates the timestamp file if returning true.
 *
 * Accepts an optional custom path for testing.
 */
export function shouldCheckForUpdate(throttleFilePath?: string): boolean {
	const filePath = throttleFilePath ?? defaultThrottleFile();
	try {
		const dir = dirname(filePath);
		mkdirSync(dir, { recursive: true });

		try {
			const raw = readFileSync(filePath, "utf8").trim();
			const last = Number(raw);
			if (!Number.isNaN(last) && Date.now() - last < THROTTLE_MS) {
				return false;
			}
		} catch {
			// File doesn't exist or unreadable — fall through to allow check
		}

		writeFileSync(filePath, String(Date.now()), "utf8");
		return true;
	} catch {
		// Filesystem error — fail open (allow check)
		return true;
	}
}

// ---------------------------------------------------------------------------
// Unit 4: Binary Updater
// ---------------------------------------------------------------------------

/**
 * Download the latest binary and atomically replace the current one.
 * Returns true on success, false on failure.
 */
export async function updateBinary(binaryPath: string, version: string): Promise<boolean> {
	const platform = process.platform === "darwin" ? "darwin" : "linux";
	const arch = process.arch === "arm64" ? "arm64" : "x64";
	const url = `https://github.com/nklisch/krometrail/releases/download/${version}/krometrail-${platform}-${arch}`;
	const tempPath = `${binaryPath}.update.${Date.now()}`;

	try {
		const response = await fetch(url, { signal: AbortSignal.timeout(60000) });
		if (!response.ok) return false;

		const buffer = await response.arrayBuffer();
		if (buffer.byteLength === 0) return false;

		writeFileSync(tempPath, Buffer.from(buffer));
		chmodSync(tempPath, 0o755);

		renameSync(tempPath, binaryPath);

		// On macOS, remove quarantine attribute (best-effort)
		if (process.platform === "darwin") {
			try {
				Bun.spawnSync(["xattr", "-d", "com.apple.quarantine", binaryPath], { stdio: ["ignore", "ignore", "ignore"] });
			} catch {
				// Ignore — quarantine removal is best-effort
			}
		}

		process.stderr.write(`[krometrail] Updated to ${version} (restart to use new version)\n`);
		return true;
	} catch {
		// Clean up temp file on any error
		try {
			unlinkSync(tempPath);
		} catch {
			// Ignore cleanup error
		}
		return false;
	}
}

// ---------------------------------------------------------------------------
// Unit 5: Package Manager Updater
// ---------------------------------------------------------------------------

/**
 * Update global npm/bun package in the background.
 * Fire-and-forget — never throws.
 */
export async function updateGlobalPackage(packageManager: "npm" | "bun"): Promise<void> {
	try {
		process.stderr.write(`[krometrail] Updating via ${packageManager}...\n`);
		const args = packageManager === "npm" ? ["npm", "update", "-g", "krometrail"] : ["bun", "update", "-g", "krometrail"];
		Bun.spawn(args, { stdio: ["ignore", "ignore", "ignore"] });
	} catch {
		// Best-effort — ignore all errors
	}
}

// ---------------------------------------------------------------------------
// Unit 6: Orchestrator
// ---------------------------------------------------------------------------

/**
 * Main entry point. Detects install type, checks for updates,
 * and performs the appropriate update action.
 *
 * Fire-and-forget — never throws, never blocks MCP startup.
 * Called without await from the MCP entry point.
 */
export function performAutoUpdate(): void {
	// Opt-out via env var
	const noUpdate = process.env.KROMETRAIL_NO_UPDATE;
	if (noUpdate && noUpdate !== "0" && noUpdate !== "false") return;

	const install = detectInstallType();

	// Skip dev installs
	if (install.type === "dev") return;

	// Check throttle synchronously before starting async work
	if (!shouldCheckForUpdate()) return;

	// Fire-and-forget async work
	void (async () => {
		try {
			const result = await checkLatestVersion();
			if (!result || !result.updateAvailable) return;

			const { latestVersion } = result;

			if (install.type === "binary" && install.binaryPath) {
				await updateBinary(install.binaryPath, latestVersion);
			} else if (install.type === "global-npm" && install.packageManager) {
				await updateGlobalPackage(install.packageManager);
			} else if (install.type === "npx" || install.type === "bunx") {
				// Suggest @latest tag if not already used
				const usesLatest = process.argv.some((a) => a.includes("@latest"));
				if (!usesLatest) {
					process.stderr.write(`[krometrail] Update available (${latestVersion}). Use \`${install.type} krometrail@latest --mcp\` to always get the latest version.\n`);
				}
			}
		} catch {
			// Auto-update must never crash the server
		}
	})();
}
