import { describe, expect, it } from "vitest";
import { BrowserRecorder } from "../../../src/browser/recorder/index.js";
import { isChromeAvailable, launchTestChrome } from "../../helpers/chrome-check.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: auto-stop on Chrome disconnect", () => {
	it("recorder stops automatically when Chrome closes", async () => {
		const { port, cleanup } = await launchTestChrome();

		const recorder = new BrowserRecorder({
			port,
			attach: true,
			allTabs: false,
			// Fast reconnect: 3 attempts × 200ms = ~600ms
			maxReconnectAttempts: 3,
			reconnectDelayMs: 200,
		});

		let autoStopCalled = false;
		recorder.onAutoStop = () => {
			autoStopCalled = true;
		};

		await recorder.start();
		expect(recorder.isRecording()).toBe(true);

		// Kill Chrome — triggers CDP disconnect
		await cleanup();

		// Wait for reconnection attempts to exhaust + auto-stop
		const deadline = Date.now() + 10_000;
		while (recorder.isRecording() && Date.now() < deadline) {
			await new Promise<void>((r) => setTimeout(r, 100));
		}

		expect(recorder.isRecording()).toBe(false);
		expect(autoStopCalled).toBe(true);
	}, 30_000);
});
