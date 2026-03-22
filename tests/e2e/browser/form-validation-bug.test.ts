import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: form validation bug investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// --- Record a realistic user session ---

		// 1. User lands on home page
		await ctx.navigate("/");
		await ctx.wait(500);

		// 2. User navigates to login
		await ctx.click('[data-testid="nav-login"]');
		await ctx.wait(800);

		// 3. User logs in successfully
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm("#login-form");
		await ctx.wait(1500); // Wait for login + redirect to dashboard

		// 4. User navigates to settings
		await ctx.navigate("/settings");
		await ctx.wait(800);

		// 5. User fills settings form with bad email and submits
		await ctx.fill('[data-testid="name"]', "Admin User");
		await ctx.fill('[data-testid="email"]', "not-an-email");
		await ctx.fill('[data-testid="phone"]', "555-1234");
		await ctx.submitForm("#settings-form");
		await ctx.wait(1000);

		// 6. User marks the moment
		await ctx.placeMarker("form validation failed");

		// 7. Wait for events to settle then stop recording
		await ctx.wait(500);
		await ctx.finishRecording();
	}, 60_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("session_list finds the recorded session", async () => {
		const result = await ctx.callTool("session_list", {});
		expect(result).toContain("Sessions (");
		expect(result).toContain("localhost");
		// Should show at least 1 marker
		expect(result).toMatch(/\d+ markers/);
	});

	it("session_overview shows navigation timeline, markers, and errors", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
		});

		// Should show the navigation path
		expect(overview).toContain("Timeline:");
		// Should mention the marker we placed
		expect(overview).toContain("form validation failed");
		// Should show the 422 error in the error section or network summary
		expect(overview).toMatch(/422|error|failed/i);
	});

	it("session_search finds the 422 response by status code", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			status_codes: [422],
			limit: 50,
		});

		expect(searchResult).toContain("422");
		expect(searchResult).toContain("/api/settings");
	});

	it("session_search finds events by natural language query", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			query: "settings save failed",
		});

		// FTS should find console.error messages about settings
		expect(searchResult).toContain("Found");
		expect(searchResult).not.toBe("No matching events found.");
	});

	it("session_inspect reveals the full 422 response body with validation details", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		// Find the 422 event
		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			status_codes: [422],
			limit: 50,
		});
		const eventId = extractEventId(searchResult);

		const inspectResult = await ctx.callTool("session_inspect", {
			session_id: sessionId,
			event_id: eventId,
			include: ["surrounding_events", "network_body"],
		});

		// Should include the 422 status and URL details
		expect(inspectResult).toContain("422");
		expect(inspectResult).toContain("/api/settings");
		// Should show surrounding events context
		expect(inspectResult).toContain("Context");
	});

	it("session_overview focused on the marker shows concentrated evidence", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		// Get markers
		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
			include: ["markers"],
		});
		const markerId = extractMarkerId(overview);

		const focused = await ctx.callTool("session_overview", {
			session_id: sessionId,
			around_marker: markerId,
		});

		// Focused overview should still contain the marker and nearby events
		expect(focused).toContain("form validation failed");
	});

	it("session_replay_context generates reproduction steps", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const replayResult = await ctx.callTool("session_replay_context", {
			session_id: sessionId,
			format: "reproduction_steps",
		});

		// Should contain numbered steps
		expect(replayResult).toMatch(/1\.\s/);
		// Should mention navigation, form fill, and the error
		expect(replayResult).toContain("Navigate");
		expect(replayResult).toMatch(/422|error|Actual/i);
	});
});

// --- Helpers ---

function extractSessionId(listOutput: string): string {
	const match = listOutput.match(/([a-f0-9-]{36})/);
	if (!match) throw new Error(`Could not extract session ID from:\n${listOutput}`);
	return match[1];
}

function extractEventId(searchOutput: string): string {
	const match = searchOutput.match(/id:\s*([a-f0-9-]{36})/);
	if (!match) throw new Error(`Could not extract event ID from:\n${searchOutput}`);
	return match[1];
}

function extractMarkerId(overviewOutput: string): string {
	// Marker IDs are rendered as "(id: <marker-id>)" in the overview.
	// User markers have label-based IDs (e.g., "form-validation-failed-a1b2c3d4"),
	// not full UUIDs, so a generic UUID regex would match the session ID instead.
	const markerMatch = overviewOutput.match(/\(id:\s*([^)]+)\)/);
	if (!markerMatch) throw new Error(`Could not extract marker ID from:\n${overviewOutput}`);
	return markerMatch[1].trim();
}
