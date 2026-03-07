import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { resolve } from "node:path";
import type { BrowserSessionInfo, Marker } from "../types.js";
import type { DetectionRule } from "./auto-detect.js";
import { AutoDetector, DEFAULT_DETECTION_RULES } from "./auto-detect.js";
import { CDPClient, type CDPClientOptions, fetchBrowserWsUrl } from "./cdp-client.js";
import { EventNormalizer } from "./event-normalizer.js";
import { InputTracker } from "./input-tracker.js";
import { type BufferConfig, BufferConfigSchema, RollingBuffer } from "./rolling-buffer.js";
import { TabManager } from "./tab-manager.js";

export interface BrowserRecorderConfig {
	/** CDP port Chrome is listening on. Default: 9222 */
	port: number;
	/** If true, attach to existing Chrome rather than launching. Default: false */
	attach: boolean;
	/** Optional Chrome profile name (used as user-data-dir). */
	profile?: string;
	/** Record all tabs. Default: false (only the first/active tab). */
	allTabs: boolean;
	/** URL pattern filter for tab selection (when allTabs is false). */
	tabFilter?: string;
	/** Rolling buffer config. */
	buffer?: Partial<BufferConfig>;
	/** Override detection rules. */
	detectionRules?: DetectionRule[];
}

const DOMAINS_TO_ENABLE = [
	{ domain: "Network", params: { maxPostDataSize: 65536 } },
	{ domain: "Runtime" },
	{ domain: "Page" },
	{ domain: "Performance", params: { timeDomain: "timeTicks" } },
] as const;

/**
 * Launch Chrome with remote debugging enabled.
 */
function launchChrome(port: number, profile?: string): ChildProcess {
	const chromePaths = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];

	const args = [`--remote-debugging-port=${port}`, "--no-first-run", "--no-default-browser-check"];

	if (profile) {
		args.push(`--user-data-dir=${resolve(homedir(), ".agent-lens", "chrome-profiles", profile)}`);
	}

	for (const chromePath of chromePaths) {
		try {
			return spawn(chromePath, args, { detached: true, stdio: "ignore" });
		} catch {}
	}

	throw new Error("Chrome not found. Install Chrome or use --attach to connect to an existing instance.");
}

/**
 * Wait for Chrome's CDP endpoint to become available (polls with retries).
 */
async function waitForChrome(port: number, timeoutMs = 10_000): Promise<string> {
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

	throw new Error(`Chrome CDP not available after ${timeoutMs}ms: ${lastError?.message}`);
}

/**
 * Orchestrator that ties all Browser Lens components together.
 * Manages the full lifecycle: Chrome launch → CDP connection → tab recording → event pipeline.
 */
export class BrowserRecorder {
	private cdpClient: CDPClient | null = null;
	private tabManager: TabManager | null = null;
	private normalizer: EventNormalizer;
	private inputTracker: InputTracker;
	private buffer: RollingBuffer;
	private autoDetector: AutoDetector;
	private recording = false;
	private sessionId: string;
	private startedAt = 0;
	private chromeProcess: ChildProcess | null = null;
	private config: BrowserRecorderConfig;

	constructor(config: BrowserRecorderConfig) {
		this.config = config;
		this.sessionId = crypto.randomUUID();
		this.normalizer = new EventNormalizer();
		this.inputTracker = new InputTracker();
		this.buffer = new RollingBuffer(BufferConfigSchema.parse(config.buffer ?? {}));
		this.autoDetector = new AutoDetector(config.detectionRules ?? DEFAULT_DETECTION_RULES);
	}

	/** Connect to Chrome and start recording. */
	async start(): Promise<BrowserSessionInfo> {
		if (this.recording) {
			throw new Error("Browser recorder is already running");
		}

		let wsUrl: string;
		if (this.config.attach) {
			// Connect to existing Chrome
			wsUrl = await fetchBrowserWsUrl(this.config.port);
		} else {
			// Launch Chrome
			this.chromeProcess = launchChrome(this.config.port, this.config.profile);
			wsUrl = await waitForChrome(this.config.port);
		}

		const cdpOptions: CDPClientOptions = {
			browserWsUrl: wsUrl,
			autoReconnect: true,
			maxReconnectAttempts: 10,
			reconnectDelayMs: 1000,
		};

		this.cdpClient = new CDPClient(cdpOptions);

		// Re-subscribe to tab sessions on reconnect
		this.cdpClient.on("reconnected", () => {
			this.reattachToTabs().catch(() => {});
		});

		await this.cdpClient.connect();

		this.tabManager = new TabManager(this.cdpClient);

		// Subscribe to all CDP events and route them
		this.cdpClient.on("event", (sessionId: string, method: string, params: Record<string, unknown>) => {
			this.onCDPEvent(sessionId, method, params);
		});

		// Enable Target domain and discover tabs
		await this.cdpClient.enableDomain("Target");
		await this.tabManager.discoverTabs();

		// Start recording tabs
		const tabs = this.tabManager.listTabs();
		if (tabs.length === 0) {
			throw new Error("No browser tabs found. Open a tab in Chrome and try again.");
		}

		if (this.config.allTabs) {
			for (const tab of tabs) {
				await this.startRecordingTab(tab.targetId);
			}
		} else {
			// Record first matching tab or first tab
			const filter = this.config.tabFilter;
			const target = filter ? (tabs.find((t) => t.url.includes(filter)) ?? tabs[0]) : tabs[0];
			await this.startRecordingTab(target.targetId);
		}

		this.recording = true;
		this.startedAt = Date.now();

		return this.buildSessionInfo();
	}

	/** Place a marker at the current time. */
	placeMarker(label?: string): Marker {
		return this.buffer.placeMarker(label, false);
	}

	/** Get current session info, or null if not recording. */
	getSessionInfo(): BrowserSessionInfo | null {
		if (!this.recording) return null;
		return this.buildSessionInfo();
	}

	/** Whether the recorder is currently active. */
	isRecording(): boolean {
		return this.recording;
	}

	/** Stop recording and disconnect. */
	async stop(closeBrowser = false): Promise<void> {
		this.recording = false;

		if (this.tabManager) {
			for (const tab of this.tabManager.listRecordingTabs()) {
				await this.tabManager.stopRecording(tab.targetId).catch(() => {});
			}
		}

		if (this.cdpClient) {
			await this.cdpClient.disconnect().catch(() => {});
			this.cdpClient = null;
		}

		if (closeBrowser && this.chromeProcess) {
			this.chromeProcess.kill();
			this.chromeProcess = null;
		}
	}

	private async startRecordingTab(targetId: string): Promise<void> {
		if (!this.cdpClient || !this.tabManager) return;

		const sessionId = await this.tabManager.startRecording(targetId);

		// Enable CDP domains for this tab session
		for (const { domain, params } of DOMAINS_TO_ENABLE) {
			await this.cdpClient.sendToTarget(sessionId, `${domain}.enable`, params as Record<string, unknown>).catch(() => {});
		}

		// Inject input tracker script
		await this.cdpClient
			.sendToTarget(sessionId, "Page.addScriptToEvaluateOnNewDocument", {
				source: this.inputTracker.getInjectionScript(),
			})
			.catch(() => {});
	}

	private async reattachToTabs(): Promise<void> {
		if (!this.tabManager) return;
		// Re-enable Target domain after reconnect
		await this.cdpClient?.enableDomain("Target").catch(() => {});
		await this.tabManager.discoverTabs().catch(() => {});

		const tabs = this.tabManager.listTabs();
		for (const tab of tabs) {
			if (!tab.recording) {
				await this.startRecordingTab(tab.targetId).catch(() => {});
			}
		}
	}

	private onCDPEvent(sessionId: string, method: string, params: Record<string, unknown>): void {
		if (!this.tabManager) return;

		const tabId = sessionId ? (this.tabManager.getTabIdForSession(sessionId) ?? "") : "";
		if (sessionId && !tabId) return; // Event from untracked session, skip

		// Check for input tracker events in consoleAPICalled
		if (method === "Runtime.consoleAPICalled") {
			const args = params.args as Array<{ value?: string }> | undefined;
			if (args?.[0]?.value === "__BL__" && args[1]?.value) {
				const inputEvent = this.inputTracker.processInputEvent(args[1].value, tabId);
				if (inputEvent) {
					if (inputEvent.type === "marker") {
						// Keyboard-triggered marker
						this.buffer.placeMarker(inputEvent.data.label as string | undefined, false);
					} else {
						this.buffer.push(inputEvent);
						this.checkAutoDetect(inputEvent);
					}
				}
				return; // Don't pass __BL__ messages to normalizer
			}
		}

		// Normalize the CDP event
		const event = this.normalizer.normalize(method, params, tabId || "browser");
		if (!event) return;

		// Add to buffer
		this.buffer.push(event);

		// Check auto-detection rules
		this.checkAutoDetect(event);
	}

	private checkAutoDetect(event: import("../types.js").RecordedEvent): void {
		const recentEvents = this.buffer.getEvents(event.timestamp - 5000, event.timestamp);
		const markers = this.autoDetector.check(event, recentEvents);
		for (const m of markers) {
			this.buffer.placeMarker(m.label, true, m.severity);
		}
	}

	private buildSessionInfo(): BrowserSessionInfo {
		const stats = this.buffer.getStats();
		const tabs = this.tabManager?.listRecordingTabs() ?? [];
		const bufferAgeMs = stats.oldestTimestamp > 0 ? Date.now() - stats.oldestTimestamp : 0;

		return {
			id: this.sessionId,
			startedAt: this.startedAt,
			tabs: tabs.map((t) => ({ targetId: t.targetId, url: t.url, title: t.title })),
			eventCount: stats.eventCount,
			markerCount: stats.markerCount,
			bufferAgeMs,
		};
	}
}
