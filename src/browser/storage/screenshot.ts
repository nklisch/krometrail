import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { z } from "zod";
import type { CDPClient } from "../recorder/cdp-client.js";

export const ScreenshotConfigSchema = z.object({
	/** Periodic screenshot interval in ms. 0 to disable. Default: 0 (disabled). */
	intervalMs: z.number().default(0),
	/** Capture on navigation. Default: true. */
	onNavigation: z.boolean().default(true),
	/** Capture on marker placement. Default: true. */
	onMarker: z.boolean().default(true),
	/** JPEG quality (0–100). Default: 60. */
	quality: z.number().min(0).max(100).default(60),
});

export type ScreenshotConfig = z.infer<typeof ScreenshotConfigSchema>;

export class ScreenshotCapture {
	private intervalTimer: ReturnType<typeof setInterval> | null = null;

	constructor(private config: ScreenshotConfig) {}

	/**
	 * Capture a screenshot and save to the session directory.
	 * Returns the file path of the saved screenshot.
	 */
	async capture(cdpClient: CDPClient, tabSessionId: string, screenshotDir: string, timestamp?: number): Promise<string> {
		const ts = timestamp ?? Date.now();
		const result = (await cdpClient.sendToTarget(tabSessionId, "Page.captureScreenshot", {
			format: "jpeg",
			quality: this.config.quality,
		})) as { data: string };

		const filePath = resolve(screenshotDir, `${ts}.jpg`);
		writeFileSync(filePath, Buffer.from(result.data, "base64"));
		return filePath;
	}

	/**
	 * Start periodic screenshot capture.
	 */
	startPeriodic(cdpClient: CDPClient, tabSessionId: string, screenshotDir: string): void {
		if (this.config.intervalMs <= 0) return;
		this.intervalTimer = setInterval(async () => {
			try {
				await this.capture(cdpClient, tabSessionId, screenshotDir);
			} catch {
				// Tab may have closed — stop periodic capture
				this.stopPeriodic();
			}
		}, this.config.intervalMs);
	}

	stopPeriodic(): void {
		if (this.intervalTimer) {
			clearInterval(this.intervalTimer);
			this.intervalTimer = null;
		}
	}
}
