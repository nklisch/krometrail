import type { ChildProcess } from "node:child_process";
import { BrowserRecorderStateError } from "../../core/errors.js";
import { type PersistenceConfig, PersistencePipeline } from "../storage/persistence.js";
import { RetentionConfigSchema } from "../storage/retention.js";
import { ScreenshotCapture, type ScreenshotConfig, ScreenshotConfigSchema } from "../storage/screenshot.js";
import type { BrowserSessionInfo, Marker } from "../types.js";
import { AnnotationCoalescer } from "./annotation-coalescer.js";
import { getAnnotationInjectionScript } from "./annotation-injector.js";
import type { DetectionRule } from "./auto-detect.js";
import { AutoDetector, DEFAULT_DETECTION_RULES } from "./auto-detect.js";
import type { CDPClient } from "./cdp-client.js";
import { ChromeLauncher } from "./chrome-launcher.js";
import { EventNormalizer } from "./event-normalizer.js";
import { EventPipeline } from "./event-pipeline.js";
import { FrameworkTracker } from "./framework/index.js";
import { InputTracker } from "./input-tracker.js";
import { setupControlPanel } from "./marker-overlay.js";
import { type BufferConfig, BufferConfigSchema, RollingBuffer } from "./rolling-buffer.js";
import { type TabInfo, TabManager } from "./tab-manager.js";

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
	/** URL to open when launching Chrome (ignored in attach mode). */
	url?: string;
	/** Rolling buffer config. */
	buffer?: Partial<BufferConfig>;
	/** Override detection rules. */
	detectionRules?: DetectionRule[];
	/** Persistence config. If absent, recordings are not persisted to disk. */
	persistence?: Partial<PersistenceConfig>;
	/** Screenshot config. */
	screenshots?: Partial<ScreenshotConfig>;
	/** Framework state observation config. false/undefined = disabled. */
	frameworkState?: boolean | string[];
}

/** Returns true for chrome://, about:blank, and other internal URLs that can't be recorded. */
function isInternalChromeUrl(url: string): boolean {
	return url.startsWith("chrome://") || url.startsWith("chrome-extension://") || url === "about:blank";
}

const DOMAINS_TO_ENABLE = [
	{ domain: "Network", params: { maxPostDataSize: 65536 } },
	{ domain: "Runtime" },
	{ domain: "Page" },
	{ domain: "Performance", params: { timeDomain: "timeTicks" } },
] as const;

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
	private frameworkTracker: FrameworkTracker;
	private annotationCoalescer: AnnotationCoalescer;
	private recording = false;
	private sessionId: string;
	private startedAt = 0;
	private chromeProcess: ChildProcess | null = null;
	private config: BrowserRecorderConfig;
	private persistence: PersistencePipeline | null = null;
	private screenshotCapture: ScreenshotCapture | null = null;
	/** Map of targetId → CDP session ID for recording tabs. */
	private tabSessions = new Map<string, string>();
	/** Map of targetId → overlay cleanup function. */
	private overlayCleanups = new Map<string, () => void>();
	private launcher: ChromeLauncher;
	private eventPipeline: EventPipeline | null = null;
	private cachedSessionInfo: BrowserSessionInfo | null = null;

	constructor(config: BrowserRecorderConfig) {
		this.config = config;
		this.sessionId = crypto.randomUUID();
		this.normalizer = new EventNormalizer();
		this.inputTracker = new InputTracker();
		this.buffer = new RollingBuffer(BufferConfigSchema.parse(config.buffer ?? {}));
		this.autoDetector = new AutoDetector(config.detectionRules ?? DEFAULT_DETECTION_RULES);
		this.frameworkTracker = new FrameworkTracker(config.frameworkState);
		this.launcher = new ChromeLauncher();

		this.annotationCoalescer = new AnnotationCoalescer((ann) => {
			const summary = ann.count === 1 ? `Annotation: ${ann.label}` : `Annotation: ${ann.label} (×${ann.count})`;
			const event: import("../types.js").RecordedEvent = {
				id: crypto.randomUUID(),
				timestamp: ann.lastTs,
				type: "annotation",
				tabId: "",
				summary,
				data: {
					label: ann.label,
					source: ann.source,
					severity: ann.severity,
					count: ann.count,
					firstTs: ann.firstTs,
					lastTs: ann.lastTs,
					metadata: ann.metadata,
				},
			};
			this.buffer.push(event);
			this.persistence?.onNewEvent(event, this.buildSessionInfo());
			this.cachedSessionInfo = null;
		});

		if (config.persistence) {
			const bufferConfig = BufferConfigSchema.parse(config.buffer ?? {});
			this.screenshotCapture = new ScreenshotCapture(ScreenshotConfigSchema.parse(config.screenshots ?? {}));
			this.persistence = new PersistencePipeline(
				{
					...config.persistence,
					markerPaddingMs: config.persistence.markerPaddingMs ?? bufferConfig.markerPaddingMs,
				},
				this.screenshotCapture,
			);

			// Run retention cleanup on startup
			const retentionConfig = RetentionConfigSchema.parse({});
			if (retentionConfig.cleanupOnStartup) {
				this.persistence.runRetentionCleanup().catch(() => {});
			}
		}
	}

	/** Connect to Chrome and start recording. */
	async start(): Promise<BrowserSessionInfo> {
		if (this.recording) {
			throw new BrowserRecorderStateError("Browser recorder is already running");
		}

		const { cdpClient, process } = await this.launcher.connect({
			port: this.config.port,
			attach: this.config.attach,
			profile: this.config.profile,
			url: this.config.url,
		});

		this.cdpClient = cdpClient;
		this.chromeProcess = process ?? null;

		try {
			// Re-subscribe to tab sessions on reconnect
			this.cdpClient.on("reconnected", () => {
				this.reattachToTabs().catch(() => {});
			});

			await this.cdpClient.connect();

			this.tabManager = new TabManager(this.cdpClient);

			// Wire up the event pipeline
			this.eventPipeline = new EventPipeline({
				normalizer: this.normalizer,
				buffer: this.buffer,
				inputTracker: this.inputTracker,
				autoDetector: this.autoDetector,
				tabManager: this.tabManager,
				cdpClient: this.cdpClient,
				persistence: this.persistence ?? undefined,
				screenshotCapture: this.screenshotCapture ?? undefined,
				frameworkTracker: this.frameworkTracker.isEnabled() ? this.frameworkTracker : undefined,
				annotationCoalescer: this.annotationCoalescer,
				captureOnNavigation: this.config.screenshots?.onNavigation !== false,
				getSessionInfo: () => this.buildSessionInfo(),
				getPrimaryTabSessionId: () => this.getPrimaryTabSessionId(),
				getSessionDir: () => this.persistence?.getSessionDir(this.sessionId) ?? null,
				placeMarker: (label?) => this.placeMarker(label),
				invalidateSessionCache: () => {
					this.cachedSessionInfo = null;
				},
			});

			// Subscribe to all CDP events and route them through the pipeline
			this.cdpClient.on("event", (sessionId: string, method: string, params: Record<string, unknown>) => {
				this.eventPipeline?.process(sessionId, method, params);
			});

			// Discover tabs with retry — on macOS (especially fresh profiles), Chrome
			// needs time after CDP is ready before page targets appear.
			// When we launched Chrome with a URL, wait for that specific tab.
			const expectedUrl = this.chromeProcess ? this.config.url : undefined;
			const tabs = await this.discoverTabsWithRetry(expectedUrl);

			if (this.config.allTabs) {
				// Skip internal Chrome pages (chrome://, about:blank) in all-tabs mode
				const recordable = tabs.filter((t) => !isInternalChromeUrl(t.url));
				for (const tab of recordable.length > 0 ? recordable : tabs) {
					await this.startRecordingTab(tab.targetId);
				}
			} else {
				// Pick the tab matching the URL we launched, tabFilter, or first content tab
				const target = this.pickTab(tabs);
				await this.startRecordingTab(target.targetId);
			}

			this.recording = true;
			this.startedAt = Date.now();

			return this.buildSessionInfo();
		} catch (err) {
			// Clean up Chrome process on failure to avoid orphans
			this.launcher.killProcess();
			this.chromeProcess = null;
			if (this.cdpClient) {
				await this.cdpClient.disconnect().catch(() => {});
				this.cdpClient = null;
			}
			throw err;
		}
	}

	/**
	 * Poll for page tabs with backoff. On macOS, Chrome's CDP endpoint becomes
	 * available before the first page target is created — especially on fresh
	 * profiles where welcome/setup pages add latency.
	 *
	 * When expectedUrl is set (we launched Chrome with a URL), keep polling until
	 * a tab matching that URL appears. Falls back to any content tab on timeout.
	 */
	private async discoverTabsWithRetry(expectedUrl?: string, timeoutMs = 10_000): Promise<TabInfo[]> {
		const deadline = Date.now() + timeoutMs;
		const pollIntervalMs = 500;

		// tabManager is guaranteed to be set — start() initializes it before calling this method
		const tm = this.tabManager as TabManager;
		while (Date.now() < deadline) {
			await tm.discoverTabs();
			const tabs = tm.listTabs();

			if (tabs.length > 0) {
				if (expectedUrl) {
					// Wait for the specific URL we told Chrome to open
					if (tabs.some((t) => t.url.includes(expectedUrl))) return tabs;
				} else {
					// No expected URL — accept any non-internal tab, or any tab in attach mode
					const hasContent = tabs.some((t) => !isInternalChromeUrl(t.url));
					if (hasContent || !this.chromeProcess) return tabs;
				}
			}
			await new Promise<void>((r) => setTimeout(r, pollIntervalMs));
		}

		// Timeout — return whatever tabs exist rather than failing
		const finalTabs = tm.listTabs();
		if (finalTabs.length > 0) return finalTabs;

		throw new BrowserRecorderStateError(
			"No browser tabs found after waiting 10s. Chrome launched but never created a page target. " + "Try closing Chrome and retrying, or use attach=true with a manually launched Chrome.",
		);
	}

	/** Select which tab to record: match requested URL > tabFilter > first content tab > first tab. */
	private pickTab(tabs: TabInfo[]): TabInfo {
		const url = this.config.url;
		const filter = this.config.tabFilter;

		// Prefer the tab matching the URL we launched Chrome with
		if (url) {
			const match = tabs.find((t) => t.url.includes(url));
			if (match) return match;
		}
		// Then try tabFilter
		if (filter) {
			const match = tabs.find((t) => t.url.includes(filter));
			if (match) return match;
		}
		// Fall back to first non-internal tab, or first tab
		return tabs.find((t) => !isInternalChromeUrl(t.url)) ?? tabs[0];
	}

	/** Place a marker at the current time. */
	async placeMarker(label?: string): Promise<Marker> {
		const marker = this.buffer.placeMarker(label, false);
		this.cachedSessionInfo = null;

		if (this.persistence && this.cdpClient) {
			const sessionInfo = this.getSessionInfo();
			const tabSessionId = this.getPrimaryTabSessionId();
			if (sessionInfo && tabSessionId) {
				await this.persistence.onMarkerPlaced(marker, this.buffer, sessionInfo, this.cdpClient, tabSessionId);
			}
		}

		return marker;
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
		this.eventPipeline = null;
		this.cachedSessionInfo = null;

		for (const cleanup of this.overlayCleanups.values()) cleanup();
		this.overlayCleanups.clear();

		if (this.screenshotCapture) {
			this.screenshotCapture.stopPeriodic();
		}

		this.annotationCoalescer.flushAll();
		this.annotationCoalescer.dispose();

		if (this.persistence) {
			this.persistence.endSession(this.sessionId);
		}

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
		this.tabSessions.set(targetId, sessionId);

		// Enable CDP domains for this tab session
		for (const { domain, params } of DOMAINS_TO_ENABLE) {
			await this.cdpClient.sendToTarget(sessionId, `${domain}.enable`, params as Record<string, unknown>).catch(() => {});
		}

		// Inject annotation API script (window.__krometrail.mark)
		await this.cdpClient
			.sendToTarget(sessionId, "Page.addScriptToEvaluateOnNewDocument", {
				source: getAnnotationInjectionScript(),
			})
			.catch(() => {});

		// Inject framework detection scripts (before input tracker to ensure hooks install first)
		for (const script of this.frameworkTracker.getInjectionScripts()) {
			await this.cdpClient.sendToTarget(sessionId, "Page.addScriptToEvaluateOnNewDocument", { source: script }).catch(() => {});
		}

		// Inject input tracker script
		await this.cdpClient
			.sendToTarget(sessionId, "Page.addScriptToEvaluateOnNewDocument", {
				source: this.inputTracker.getInjectionScript(),
			})
			.catch(() => {});

		// Inject control panel (mark + snap buttons, keyboard shortcuts)
		const snapshotCallback =
			this.screenshotCapture && this.persistence
				? async () => {
						if (!this.cdpClient) return;
						const sessDir = this.persistence?.getOrCreateSessionDir(this.buildSessionInfo());
						if (sessDir) await this.screenshotCapture?.capture(this.cdpClient, sessionId, `${sessDir}/screenshots`).catch(() => {});
					}
				: null;
		const overlayCleanup = await setupControlPanel(this.cdpClient, sessionId, (label) => this.placeMarker(label), snapshotCallback, this.config.screenshots?.intervalMs ?? 0);
		this.overlayCleanups.set(targetId, overlayCleanup);

		// Start periodic screenshot capture if configured
		if (this.screenshotCapture && this.persistence) {
			const sessionDir = this.persistence.getSessionDir(this.sessionId);
			if (sessionDir) {
				this.screenshotCapture.startPeriodic(this.cdpClient, sessionId, `${sessionDir}/screenshots`);
			}
		}
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

	/** Get the CDP client (for step executor). Returns null if not recording. */
	getCDPClient(): CDPClient | null {
		return this.cdpClient;
	}

	/** Get the primary tab session ID (for step executor). */
	getPrimaryTabSession(): string | null {
		return this.getPrimaryTabSessionId();
	}

	/** Get the screenshot capture instance. */
	getScreenshotCapture(): ScreenshotCapture | null {
		return this.screenshotCapture;
	}

	/** Get the session screenshot directory (returns null if session dir not yet created). */
	getScreenshotDir(): string | null {
		if (!this.persistence) return null;
		const sessDir = this.persistence.getSessionDir(this.sessionId);
		return sessDir ? `${sessDir}/screenshots` : null;
	}

	/** Get or create the session screenshot directory (eagerly initializes persistence). */
	getOrCreateScreenshotDir(): string | null {
		if (!this.persistence) return null;
		const sessDir = this.persistence.getOrCreateSessionDir(this.buildSessionInfo());
		return `${sessDir}/screenshots`;
	}

	private getPrimaryTabSessionId(): string | null {
		const [first] = this.tabSessions.values();
		return first ?? null;
	}

	private buildSessionInfo(): BrowserSessionInfo {
		if (this.cachedSessionInfo) return this.cachedSessionInfo;

		const stats = this.buffer.getStats();
		const tabs = this.tabManager?.listRecordingTabs() ?? [];
		const bufferAgeMs = stats.oldestTimestamp > 0 ? Date.now() - stats.oldestTimestamp : 0;

		this.cachedSessionInfo = {
			id: this.sessionId,
			startedAt: this.startedAt,
			tabs: tabs.map((t) => ({ targetId: t.targetId, url: t.url, title: t.title })),
			eventCount: stats.eventCount,
			markerCount: stats.markerCount,
			bufferAgeMs,
		};
		return this.cachedSessionInfo;
	}
}
