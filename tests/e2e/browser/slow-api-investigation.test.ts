import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: slow API and WebSocket failure investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// 1. Login first
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm("#login-form");
		await ctx.wait(1500);

		// 2. Inject a 6-second API delay (slow enough to trigger auto-detection at >5s)
		await ctx.testControl("/__test__/set-delay?ms=6000");

		// 3. Navigate to dashboard (will trigger slow /api/dashboard fetch)
		await ctx.navigate("/dashboard");
		await ctx.wait(8000); // Wait for the slow response

		// 4. Force-close the WebSocket from server side
		await ctx.testControl("/__test__/close-ws");
		await ctx.wait(1000);

		// 5. Place marker and finish
		await ctx.placeMarker("slow load + ws disconnect");
		await ctx.wait(500);

		// Reset delay for cleanliness
		await ctx.testControl("/__test__/set-delay?ms=0");

		await ctx.finishRecording();
	}, 90_000); // Long timeout due to slow API

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("session_overview shows network summary with request counts", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
			include: ["network_summary", "markers"],
		});

		expect(overview).toContain("Network:");
		expect(overview).toMatch(/\d+ requests/);
	});

	it("session_search finds WebSocket events", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const wsEvents = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["websocket"],
		});

		// Should find WS open, close, and possibly frame events
		expect(wsEvents).toContain("Found");
		expect(wsEvents).toMatch(/websocket/i);
	});

	it("session_search for network_response finds the slow dashboard API", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const responses = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			url_pattern: "**/api/dashboard**",
			limit: 50,
		});

		expect(responses).toContain("/api/dashboard");
	});

	it("session_inspect on dashboard response shows full JSON body", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const responses = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			url_pattern: "**/api/dashboard**",
			limit: 50,
		});
		const eventId = extractEventId(responses);

		const inspectResult = await ctx.callTool("session_inspect", {
			session_id: sessionId,
			event_id: eventId,
			include: ["network_body", "surrounding_events"],
		});

		// Should show the dashboard API response details
		expect(inspectResult).toContain("/api/dashboard");
		expect(inspectResult).toContain("200");
	});

	it("session_replay_context generates a summary of the session", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const summary = await ctx.callTool("session_replay_context", {
			session_id: sessionId,
			format: "summary",
		});

		// Summary should mention navigation, user actions, and any errors
		expect(summary).toContain("Navigation");
	});
});

function extractSessionId(output: string): string {
	const match = output.match(/([a-f0-9-]{36})/);
	if (!match) throw new Error(`No session ID in:\n${output}`);
	return match[1];
}

function extractEventId(output: string): string {
	const match = output.match(/id:\s*([a-f0-9-]{36})/);
	if (!match) throw new Error(`No event ID in:\n${output}`);
	return match[1];
}
