import type { PersistencePipeline } from "../storage/persistence.js";
import type { ScreenshotCapture } from "../storage/screenshot.js";
import type { BrowserSessionInfo, Marker, RecordedEvent } from "../types.js";
import type { AutoDetector } from "./auto-detect.js";
import type { CDPClient } from "./cdp-client.js";
import type { EventNormalizer } from "./event-normalizer.js";
import type { InputTracker } from "./input-tracker.js";
import type { RollingBuffer } from "./rolling-buffer.js";
import type { TabManager } from "./tab-manager.js";

export interface EventPipelineConfig {
	normalizer: EventNormalizer;
	buffer: RollingBuffer;
	inputTracker: InputTracker;
	autoDetector: AutoDetector;
	tabManager: TabManager;
	cdpClient: CDPClient;
	persistence?: PersistencePipeline;
	screenshotCapture?: ScreenshotCapture;
	/** Whether to capture a screenshot on navigation events. */
	captureOnNavigation: boolean;
	/** Called to get current session info for persistence calls. */
	getSessionInfo: () => BrowserSessionInfo;
	/** Called to get the primary tab's CDP session ID. */
	getPrimaryTabSessionId: () => string | null;
	/** Called to get the session recording directory. */
	getSessionDir: () => string | null;
	/** Called to place a user-initiated marker (keyboard-triggered). */
	placeMarker: (label?: string) => Promise<Marker>;
	/** Called to signal that the buffer changed and cached session info should be invalidated. */
	invalidateSessionCache: () => void;
}

/**
 * Owns the CDP event processing flow: input tracking, normalization, buffer push,
 * persistence, screenshot triggers, and auto-detection.
 */
export class EventPipeline {
	constructor(private config: EventPipelineConfig) {}

	/** Process a single CDP event dispatched from the CDPClient. */
	process(sessionId: string, method: string, params: Record<string, unknown>): void {
		const { tabManager, normalizer, inputTracker, buffer, persistence, screenshotCapture } = this.config;

		const tabId = sessionId ? (tabManager.getTabIdForSession(sessionId) ?? "") : "";
		if (sessionId && !tabId) return; // Event from untracked session, skip

		// Check for input tracker events in consoleAPICalled
		if (method === "Runtime.consoleAPICalled") {
			const args = params.args as Array<{ value?: string }> | undefined;
			if (args?.[0]?.value === "__BL__" && args[1]?.value) {
				const inputEvent = inputTracker.processInputEvent(args[1].value, tabId);
				if (inputEvent) {
					if (inputEvent.type === "marker") {
						// Keyboard-triggered marker — fire-and-forget with persistence
						void this.config.placeMarker(inputEvent.data.label as string | undefined);
					} else {
						buffer.push(inputEvent);
						this.config.invalidateSessionCache();
						this.checkAutoDetect(inputEvent);
						if (persistence) {
							persistence.onNewEvent(inputEvent, this.config.getSessionInfo());
						}
					}
				}
				return; // Don't pass __BL__ messages to normalizer
			}
		}

		// Normalize the CDP event
		const event = normalizer.normalize(method, params, tabId || "browser");
		if (!event) return;

		// Add to buffer
		buffer.push(event);
		this.config.invalidateSessionCache();

		// Persist if within an open marker window
		if (persistence) {
			persistence.onNewEvent(event, this.config.getSessionInfo());
		}

		// Capture screenshot on navigation if configured
		if (event.type === "navigation" && screenshotCapture && this.config.captureOnNavigation) {
			const sessionDir = this.config.getSessionDir();
			const tabSessionId = this.config.getPrimaryTabSessionId();
			if (sessionDir && tabSessionId) {
				void screenshotCapture.capture(this.config.cdpClient, tabSessionId, `${sessionDir}/screenshots`).catch(() => {});
			}
		}

		// Check auto-detection rules
		this.checkAutoDetect(event);
	}

	private checkAutoDetect(event: RecordedEvent): void {
		const { buffer, autoDetector, persistence } = this.config;
		const recentEvents = buffer.getEvents(event.timestamp - 5000, event.timestamp);
		const markers = autoDetector.check(event, recentEvents);
		for (const m of markers) {
			const marker = buffer.placeMarker(m.label, true, m.severity);
			this.config.invalidateSessionCache();
			if (persistence) {
				const sessionInfo = this.config.getSessionInfo();
				const tabSessionId = this.config.getPrimaryTabSessionId();
				if (tabSessionId) {
					void persistence.onMarkerPlaced(marker, buffer, sessionInfo, this.config.cdpClient, tabSessionId).catch(() => {});
				}
			}
		}
	}
}
