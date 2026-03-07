import { spawn } from "node:child_process";

const CHROME_BINARIES = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];

/**
 * Check if Chrome (or Chromium) is available on this system.
 */
export async function isChromeAvailable(): Promise<boolean> {
	for (const binary of CHROME_BINARIES) {
		try {
			const available = await new Promise<boolean>((resolve) => {
				const proc = spawn(binary, ["--version"], { stdio: "pipe" });
				proc.on("close", (code) => resolve(code === 0));
				proc.on("error", () => resolve(false));
			});
			if (available) return true;
		} catch {}
	}
	return false;
}

/**
 * Find the first available Chrome binary path.
 */
export async function findChromeBinary(): Promise<string | null> {
	for (const binary of CHROME_BINARIES) {
		try {
			const available = await new Promise<boolean>((resolve) => {
				const proc = spawn(binary, ["--version"], { stdio: "pipe" });
				proc.on("close", (code) => resolve(code === 0));
				proc.on("error", () => resolve(false));
			});
			if (available) return binary;
		} catch {}
	}
	return null;
}

/**
 * Launch Chrome with CDP for testing. Returns the port and a cleanup function.
 * Uses a temp profile directory to avoid conflicting with user's Chrome.
 */
export async function launchTestChrome(): Promise<{ port: number; cleanup: () => Promise<void> }> {
	const binary = await findChromeBinary();
	if (!binary) {
		throw new Error("Chrome not found. Install Chrome or Chromium to run browser integration tests.");
	}

	const port = 9300 + Math.floor(Math.random() * 100);
	const { mkdtempSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");

	const profileDir = mkdtempSync(join(tmpdir(), "agent-lens-test-chrome-"));

	const proc = spawn(
		binary,
		[`--remote-debugging-port=${port}`, `--user-data-dir=${profileDir}`, "--no-first-run", "--no-default-browser-check", "--headless=new", "--disable-gpu", "--disable-dev-shm-usage"],
		{ stdio: "ignore" },
	);

	// Wait for Chrome to be ready
	const deadline = Date.now() + 10_000;
	while (Date.now() < deadline) {
		try {
			const resp = await fetch(`http://localhost:${port}/json/version`);
			if (resp.ok) break;
		} catch {
			await new Promise<void>((r) => setTimeout(r, 300));
		}
	}

	const cleanup = async () => {
		proc.kill("SIGTERM");
		await new Promise<void>((r) => setTimeout(r, 500));
		// Clean up profile directory
		const { rmSync } = await import("node:fs");
		try {
			rmSync(profileDir, { recursive: true, force: true });
		} catch {
			// Ignore cleanup errors
		}
	};

	return { port, cleanup };
}
