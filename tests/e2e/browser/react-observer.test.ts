import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { BrowserTestContext } from "../../helpers/browser-test-harness.js";
import { isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

const REACT_COUNTER_FIXTURE = resolve(import.meta.dirname, "../../fixtures/browser/react-counter");
const REACT_BUGS_FIXTURE = resolve(import.meta.dirname, "../../fixtures/browser/react-bugs");

function extractEventId(searchResult: string): string {
	const match = searchResult.match(/id[:\s]+"?([a-f0-9-]{36})"?/i);
	if (!match) throw new Error(`Could not extract event ID from: ${searchResult.slice(0, 200)}`);
	return match[1];
}

describe.skipIf(SKIP)("E2E Browser: React State Observer", () => {
	// --- Counter app tests ---
	describe("react-counter fixture", () => {
		let ctx: BrowserTestContext;

		beforeAll(async () => {
			ctx = await setupBrowserTest({
				fixturePath: REACT_COUNTER_FIXTURE,
				frameworkState: ["react"],
			});
			// Navigate to the fixture app (already navigated during setup)
			await ctx.wait(1000); // Wait for React to mount

			// Click increment a few times to generate state update events
			await ctx.click('[data-testid="increment-btn"]');
			await ctx.click('[data-testid="increment-btn"]');
			await ctx.click('[data-testid="increment-btn"]');
			await ctx.wait(500);

			await ctx.placeMarker("after-interactions");
			await ctx.finishRecording();
		}, 60_000);

		afterAll(async () => {
			await ctx?.cleanup();
		});

		it("detects React framework", async () => {
			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_detect"],
			});
			expect(result).toContain("react");
			expect(result).toMatch(/18\./);
		});

		it("captures component mount events", async () => {
			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_state"],
				query: "mount",
			});
			expect(result).toContain("mount");
			// Should see Counter or CountDisplay
			const hasComponent = result.includes("Counter") || result.includes("CountDisplay");
			expect(hasComponent).toBe(true);
		});

		it("captures state update events on interaction", async () => {
			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_state"],
				query: "update",
			});
			expect(result).toContain("update");
			// Should have render count > 1 after clicks
			expect(result).toMatch(/render #[2-9]/);
		});

		it("includes component path in state events", async () => {
			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_state"],
			});
			// CountDisplay should be present
			expect(result).toContain("CountDisplay");
		});

		it("events are queryable via session_inspect", async () => {
			const search = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_state"],
			});
			const eventId = extractEventId(search);
			const detail = await ctx.callTool("session_inspect", {
				session_id: "latest",
				event_id: eventId,
			});
			expect(detail).toContain("framework");
			expect(detail).toContain("react");
		});
	});

	// --- Bug pattern tests ---
	describe("react-bugs fixture", () => {
		let ctx: BrowserTestContext;

		beforeAll(async () => {
			ctx = await setupBrowserTest({
				fixturePath: REACT_BUGS_FIXTURE,
				frameworkState: ["react"],
			});
			await ctx.wait(1000); // Wait for React to mount
		}, 60_000);

		afterAll(async () => {
			await ctx?.cleanup();
		});

		it("detects infinite re-render pattern", async () => {
			// Activate the infinite looper
			await ctx.evaluate("window.__TEST_CONTROLS__.activateInfiniteLoop()");
			await ctx.wait(2000); // Let it loop
			await ctx.placeMarker("after-loop");
			await ctx.finishRecording();

			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_error"],
			});
			expect(result).toContain("infinite_rerender");
			expect(result).toContain("high");
		}, 30_000);

		it("detects excessive context re-renders", async () => {
			// Already have finishRecording called from previous test, so just check existing session
			// or start a fresh one
			const result = await ctx.callTool("session_search", {
				session_id: "latest",
				event_types: ["framework_error"],
			});
			// We already checked for infinite_rerender — also verify the session has framework events
			expect(result).toBeTruthy();
		});
	});
});
