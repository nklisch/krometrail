import { existsSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: screenshot capture", () => {
	let ctx: BrowserTestContext;
	let screenshotDir: string;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// Navigate to land on a page (recording started, but no session dir yet)
		await ctx.navigate("/");
		await ctx.wait(300);

		// Place a marker — this initialises the session dir and captures a marker screenshot
		await ctx.placeMarker("screenshot-test");
		await ctx.wait(300);

		// Navigate again — this triggers a navigation screenshot (session dir now exists)
		await ctx.navigate("/login");
		await ctx.wait(500);

		await ctx.finishRecording();

		// Locate the recording directory written by persistence
		const recordingsDir = resolve(ctx.dataDir, "recordings");
		const dirs = readdirSync(recordingsDir);
		expect(dirs.length).toBeGreaterThan(0);
		screenshotDir = resolve(recordingsDir, dirs[0], "screenshots");
	}, 30_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("creates a screenshots directory", () => {
		expect(existsSync(screenshotDir)).toBe(true);
	});

	it("saves JPEG files (not PNG)", () => {
		const files = readdirSync(screenshotDir);
		expect(files.length).toBeGreaterThan(0);
		expect(files.every((f) => f.endsWith(".jpg"))).toBe(true);
	});

	it("captures at least two screenshots (marker + navigation)", () => {
		const files = readdirSync(screenshotDir);
		expect(files.length).toBeGreaterThanOrEqual(2);
	});

	it("screenshot filenames are unix timestamps", () => {
		const files = readdirSync(screenshotDir);
		for (const f of files) {
			const ts = Number.parseInt(f.replace(".jpg", ""), 10);
			expect(Number.isFinite(ts)).toBe(true);
			expect(ts).toBeGreaterThan(1_000_000_000_000); // > year 2001
		}
	});
});
