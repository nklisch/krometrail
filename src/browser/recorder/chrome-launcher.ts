import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { CDPConnectionError, ChromeEarlyExitError, ChromeNotFoundError } from "../../core/errors.js";
import { getKrometrailSubdir } from "../../core/paths.js";
import { CDPClient, type CDPClientOptions, fetchBrowserWsUrl } from "./cdp-client.js";

/** Binary search order for Chrome/Chromium. */
const CHROME_BINARIES = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];

export interface ChromeLaunchOptions {
	/** CDP port Chrome is listening on. */
	port: number;
	/** If true, attach to existing Chrome rather than launching. */
	attach: boolean;
	/** Optional Chrome profile name (used as user-data-dir). */
	profile?: string;
	/** URL to open when launching Chrome (ignored in attach mode). */
	url?: string;
}

/**
 * Find the first available Chrome binary by spawning each candidate with --version.
 * Returns the binary path, or null if none found.
 */
export async function findChromeBinary(): Promise<string | null> {
	for (const binary of CHROME_BINARIES) {
		const ok = await new Promise<boolean>((resolve) => {
			const proc = spawn(binary, ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});
		if (ok) return binary;
	}
	return null;
}

/**
 * Manages Chrome process lifecycle and CDP connection setup.
 * Abstracts over launch vs. attach modes and CDP URL resolution.
 */
export class ChromeLauncher {
	private chromeProcess: ChildProcess | null = null;

	/**
	 * Resolve Chrome's CDP WebSocket URL, optionally launching Chrome first.
	 * Returns a CDPClient instance (not yet connected) and the Chrome process if launched.
	 */
	async connect(options: ChromeLaunchOptions): Promise<{ cdpClient: CDPClient; process?: ChildProcess }> {
		let wsUrl: string;

		if (options.attach) {
			wsUrl = await fetchBrowserWsUrl(options.port);
		} else {
			// Always use a profile to ensure an isolated user-data-dir.
			// Without one, macOS Chrome detects the existing instance, delegates
			// the URL to it, and the spawned process exits immediately — causing
			// a CDP timeout.
			const profile = options.profile ?? "default";
			this.chromeProcess = await this.launchChrome(options.port, profile, options.url);
			wsUrl = await this.waitForChrome(options.port, this.chromeProcess);
		}

		const cdpOptions: CDPClientOptions = {
			browserWsUrl: wsUrl,
			autoReconnect: true,
			maxReconnectAttempts: 10,
			reconnectDelayMs: 1000,
		};

		const cdpClient = new CDPClient(cdpOptions);
		return { cdpClient, process: this.chromeProcess ?? undefined };
	}

	/** Kill the Chrome process if we launched it. Used for cleanup on start failure. */
	killProcess(): void {
		if (this.chromeProcess) {
			try {
				this.chromeProcess.kill();
			} catch {
				// Process may already be dead
			}
			this.chromeProcess = null;
		}
	}

	private async launchChrome(port: number, profile?: string, url?: string): Promise<ChildProcess> {
		const chromePath = await findChromeBinary();
		if (!chromePath) throw new ChromeNotFoundError();

		const args = [
			`--remote-debugging-port=${port}`,
			"--no-first-run",
			"--no-default-browser-check",
			// Suppress Chrome welcome/what's-new pages that open extra tabs on fresh profiles
			"--disable-features=ChromeWhatsNewUI",
			// Prevent default apps (Gmail, YouTube shortcuts) from creating extra tabs
			"--disable-default-apps",
			// Prevent session restore from opening old tabs alongside the requested URL
			"--disable-session-crashed-bubble",
		];

		const profileDir = profile ? resolve(getKrometrailSubdir("chrome-profiles", profile)) : null;
		if (profileDir) {
			// Mark previous session as clean so Chrome doesn't restore old tabs.
			// When we kill Chrome with SIGTERM, it writes exit_type:"Crashed" into
			// the profile Preferences. On next launch Chrome restores that session,
			// opening a duplicate tab alongside the URL we pass on the command line.
			this.clearCrashedState(profileDir);
			args.push(`--user-data-dir=${profileDir}`);
		}

		// Always pass a URL — if none provided, use about:blank to prevent Chrome
		// from opening its default new-tab page alongside nothing.
		args.push(url ?? "about:blank");

		return spawn(chromePath, args, { detached: true, stdio: "ignore" });
	}

	/**
	 * Patch the Chrome profile Preferences to clear crashed-session state.
	 * Without this, a SIGTERM-killed Chrome leaves exit_type:"Crashed" and the
	 * next launch restores old tabs — producing the "two tabs on start" bug.
	 */
	private clearCrashedState(profileDir: string): void {
		const defaultDir = join(profileDir, "Default");
		const prefsPath = join(defaultDir, "Preferences");
		try {
			if (!existsSync(prefsPath)) return;
			const raw = readFileSync(prefsPath, "utf-8");
			const prefs = JSON.parse(raw);
			let changed = false;
			if (prefs.profile?.exit_type && prefs.profile.exit_type !== "Normal") {
				prefs.profile.exit_type = "Normal";
				changed = true;
			}
			if (prefs.profile?.exited_cleanly === false) {
				prefs.profile.exited_cleanly = true;
				changed = true;
			}
			if (changed) {
				writeFileSync(prefsPath, JSON.stringify(prefs), "utf-8");
			}
		} catch {
			// Non-fatal — profile may not exist yet on first run
		}
	}

	private async waitForChrome(port: number, chromeProcess: ChildProcess, timeoutMs = 10_000): Promise<string> {
		const deadline = Date.now() + timeoutMs;
		let lastError: Error | undefined;

		// Track early exit — on macOS, Chrome exits immediately when an
		// existing instance with the same user-data-dir absorbs the launch.
		let earlyExit: { code: number | null; signal: string | null } | null = null;
		const exitHandler = (code: number | null, signal: string | null) => {
			earlyExit = { code, signal };
		};
		chromeProcess.on("exit", exitHandler);

		try {
			while (Date.now() < deadline) {
				if (earlyExit) {
					throw new ChromeEarlyExitError(earlyExit.code, earlyExit.signal);
				}
				try {
					return await fetchBrowserWsUrl(port);
				} catch (err) {
					lastError = err as Error;
					await new Promise<void>((r) => setTimeout(r, 500));
				}
			}

			throw new CDPConnectionError(`Chrome CDP not available after ${timeoutMs}ms: ${lastError?.message}`, lastError);
		} finally {
			chromeProcess.removeListener("exit", exitHandler);
		}
	}
}
