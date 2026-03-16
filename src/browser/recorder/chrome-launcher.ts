import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { resolve } from "node:path";
import { CDPConnectionError, ChromeNotFoundError } from "../../core/errors.js";
import { getKrometrailSubdir } from "../../core/paths.js";
import { CDPClient, type CDPClientOptions, fetchBrowserWsUrl } from "./cdp-client.js";

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
			this.chromeProcess = this.launchChrome(options.port, options.profile, options.url);
			wsUrl = await this.waitForChrome(options.port);
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

	private launchChrome(port: number, profile?: string, url?: string): ChildProcess {
		const chromePaths = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];

		const args = [`--remote-debugging-port=${port}`, "--no-first-run", "--no-default-browser-check"];

		if (profile) {
			args.push(`--user-data-dir=${resolve(getKrometrailSubdir("chrome-profiles", profile))}`);
		}

		if (url) {
			args.push(url);
		}

		for (const chromePath of chromePaths) {
			try {
				return spawn(chromePath, args, { detached: true, stdio: "ignore" });
			} catch {}
		}

		throw new ChromeNotFoundError();
	}

	private async waitForChrome(port: number, timeoutMs = 10_000): Promise<string> {
		const deadline = Date.now() + timeoutMs;
		let lastError: Error | undefined;

		while (Date.now() < deadline) {
			try {
				return await fetchBrowserWsUrl(port);
			} catch (err) {
				lastError = err as Error;
				await new Promise<void>((r) => setTimeout(r, 500));
			}
		}

		throw new CDPConnectionError(`Chrome CDP not available after ${timeoutMs}ms: ${lastError?.message}`, lastError);
	}
}
