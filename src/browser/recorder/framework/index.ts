import { FRAMEWORKS } from "../../../core/enums.js";
import type { EventType, RecordedEvent } from "../../types.js";
import { getDetectionScript } from "./detector.js";
import { ReactObserver } from "./react-observer.js";
import { VueObserver } from "./vue-observer.js";

/** Parsed __BL__ framework event from the injection script. */
export interface FrameworkBLEvent {
	type: "framework_detect" | "framework_state" | "framework_error";
	ts: number;
	data: Record<string, unknown>;
}

/** Normalized config for the framework tracker. */
export interface FrameworkTrackerConfig {
	/** Which frameworks to observe. Empty array = disabled. */
	frameworks: string[];
}

interface FrameworkObserverFactory {
	create(): { getInjectionScript(): string };
}

const OBSERVER_REGISTRY = new Map<string, FrameworkObserverFactory>([
	["react", { create: () => new ReactObserver() }],
	["vue", { create: () => new VueObserver() }],
]);

export class FrameworkTracker {
	private config: FrameworkTrackerConfig;

	constructor(frameworkState: boolean | string[] | undefined) {
		if (!frameworkState) {
			this.config = { frameworks: [] };
		} else if (frameworkState === true) {
			this.config = { frameworks: [...FRAMEWORKS] };
		} else {
			this.config = { frameworks: frameworkState };
		}
	}

	/** Whether framework tracking is enabled. */
	isEnabled(): boolean {
		return this.config.frameworks.length > 0;
	}

	/**
	 * Returns injection scripts to install via Page.addScriptToEvaluateOnNewDocument.
	 * Detection script is always first (index 0); observer scripts follow.
	 */
	getInjectionScripts(): string[] {
		if (!this.isEnabled()) return [];

		const scripts: string[] = [getDetectionScript(this.config.frameworks)];

		for (const fw of this.config.frameworks) {
			const factory = OBSERVER_REGISTRY.get(fw);
			if (factory) {
				scripts.push(factory.create().getInjectionScript());
			}
		}

		return scripts;
	}

	/**
	 * Try to parse a __BL__ console message as a framework event.
	 * Returns a RecordedEvent if the message is a framework_* event, null otherwise.
	 *
	 * Called by EventPipeline when it receives a __BL__ message that InputTracker
	 * does not recognize.
	 */
	processFrameworkEvent(rawJson: string, tabId: string): RecordedEvent | null {
		let parsed: FrameworkBLEvent;
		try {
			parsed = JSON.parse(rawJson);
		} catch {
			return null;
		}

		if (!parsed.type?.startsWith("framework_") || !parsed.ts || !parsed.data) {
			return null;
		}

		const type = parsed.type as EventType;
		const summary = this.buildSummary(parsed);

		return {
			id: crypto.randomUUID(),
			timestamp: parsed.ts,
			type,
			tabId,
			summary,
			data: parsed.data,
		};
	}

	private buildSummary(event: FrameworkBLEvent): string {
		const d = event.data;
		const fw = (d.framework as string) ?? "unknown";

		switch (event.type) {
			case "framework_detect":
				return `[${fw}] ${fw.charAt(0).toUpperCase() + fw.slice(1)} ${d.version ?? "?"} detected` + (d.rootCount != null ? ` (${d.rootCount} root${(d.rootCount as number) !== 1 ? "s" : ""})` : "");

			case "framework_state": {
				const name = (d.componentName as string) ?? "?";
				const change = (d.changeType as string) ?? "update";
				const count = d.renderCount != null ? ` (render #${d.renderCount})` : "";
				return `[${fw}] ${name}: ${change}${count}`;
			}

			case "framework_error": {
				const pattern = (d.pattern as string) ?? "unknown";
				const comp = (d.componentName as string) ?? "?";
				const severity = (d.severity as string) ?? "medium";
				return `[${fw}:${severity}] ${pattern} in ${comp}`;
			}

			default:
				return `[${fw}] framework event`;
		}
	}
}
