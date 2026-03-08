import { TabNotFoundError } from "../../core/errors.js";
import type { CDPClient } from "./cdp-client.js";

export interface TabInfo {
	targetId: string;
	sessionId: string | null; // CDP session ID once attached
	url: string;
	title: string;
	recording: boolean;
}

interface CDPTargetInfo {
	targetId: string;
	type: string;
	url: string;
	title: string;
}

/**
 * Tracks browser tabs and manages which ones are being recorded.
 * Subscribes to Target domain events for tab lifecycle management.
 */
export class TabManager {
	private tabs = new Map<string, TabInfo>();
	private sessionToTarget = new Map<string, string>(); // sessionId → targetId

	constructor(private cdpClient: CDPClient) {}

	/** Discover all page targets and subscribe to tab lifecycle events. */
	async discoverTabs(): Promise<TabInfo[]> {
		// Subscribe to target lifecycle
		this.cdpClient.on("event", (sessionId: string, method: string, params: Record<string, unknown>) => {
			if (sessionId !== "") return; // Only handle browser-level events
			if (method === "Target.targetCreated") {
				this.onTargetCreated(params.targetInfo as CDPTargetInfo);
			} else if (method === "Target.targetDestroyed") {
				this.onTargetDestroyed(params.targetId as string);
			} else if (method === "Target.targetInfoChanged") {
				this.onTargetInfoChanged(params.targetInfo as CDPTargetInfo);
			}
		});

		// Enable target discovery
		await this.cdpClient.send("Target.setDiscoverTargets", { discover: true });

		// Get existing targets
		const result = (await this.cdpClient.send("Target.getTargets")) as { targetInfos: CDPTargetInfo[] };
		for (const target of result.targetInfos) {
			if (target.type === "page") {
				this.tabs.set(target.targetId, {
					targetId: target.targetId,
					sessionId: null,
					url: target.url,
					title: target.title,
					recording: false,
				});
			}
		}

		return this.listTabs();
	}

	/** Start recording a specific tab. Returns the CDP session ID. */
	async startRecording(targetId: string): Promise<string> {
		const tab = this.tabs.get(targetId);
		if (!tab) {
			throw new TabNotFoundError(targetId);
		}

		const sessionId = await this.cdpClient.attachToTarget(targetId);
		tab.sessionId = sessionId;
		tab.recording = true;
		this.sessionToTarget.set(sessionId, targetId);

		return sessionId;
	}

	/** Stop recording a specific tab. */
	async stopRecording(targetId: string): Promise<void> {
		const tab = this.tabs.get(targetId);
		if (!tab || !tab.sessionId) return;

		this.sessionToTarget.delete(tab.sessionId);
		tab.sessionId = null;
		tab.recording = false;
	}

	/** Get tab targetId for a given CDP sessionId. */
	getTabIdForSession(sessionId: string): string | null {
		return this.sessionToTarget.get(sessionId) ?? null;
	}

	/** Get all currently known tabs. */
	listTabs(): TabInfo[] {
		return Array.from(this.tabs.values());
	}

	/** Get tabs that are currently recording. */
	listRecordingTabs(): TabInfo[] {
		return Array.from(this.tabs.values()).filter((t) => t.recording);
	}

	/** Get the first page tab (most recently focused, or first discovered). */
	getFirstPageTab(): TabInfo | null {
		for (const tab of this.tabs.values()) {
			return tab;
		}
		return null;
	}

	private onTargetCreated(target: CDPTargetInfo): void {
		if (target.type !== "page") return;
		if (!this.tabs.has(target.targetId)) {
			this.tabs.set(target.targetId, {
				targetId: target.targetId,
				sessionId: null,
				url: target.url,
				title: target.title,
				recording: false,
			});
		}
	}

	private onTargetDestroyed(targetId: string): void {
		const tab = this.tabs.get(targetId);
		if (tab?.sessionId) {
			this.sessionToTarget.delete(tab.sessionId);
		}
		this.tabs.delete(targetId);
	}

	private onTargetInfoChanged(target: CDPTargetInfo): void {
		const tab = this.tabs.get(target.targetId);
		if (tab) {
			tab.url = target.url;
			tab.title = target.title;
		}
	}
}
