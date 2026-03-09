import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: agent investigation workflow", () => {
	let ctx: BrowserTestContext;

	// Shared state across sequential test steps (mimicking agent's progressive discovery)
	let sessionId: string;
	let errorEventId: string;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// Simulate a user session with a realistic bug:
		// User tries to update settings, server rejects with 422,
		// user retries and gets a different validation error.

		// Login
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm("#login-form");
		await ctx.wait(1500);

		// Go to settings
		await ctx.navigate("/settings");
		await ctx.wait(500);

		// First attempt: bad email
		await ctx.fill('[data-testid="name"]', "Admin");
		await ctx.fill('[data-testid="email"]', "bad-email");
		await ctx.fill('[data-testid="phone"]', "1234567890");
		await ctx.submitForm("#settings-form");
		await ctx.wait(1000);

		// Inject server-side failure for the next submit
		await ctx.testControl("/__test__/fail-next-submit");

		// Second attempt: fix email but server rejects phone format
		await ctx.fill('[data-testid="email"]', "admin@example.com");
		await ctx.submitForm("#settings-form");
		await ctx.wait(1000);

		// User marks the bug
		await ctx.placeMarker("settings form keeps failing");
		await ctx.wait(500);

		await ctx.finishRecording();
	}, 60_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	// --- Step 1: Agent finds the session ---
	it("Step 1: find sessions with errors", async () => {
		const result = await ctx.callTool("session_list", {});
		expect(result).toContain("Sessions (");

		sessionId = extractSessionId(result);
		expect(sessionId).toBeTruthy();
	});

	// --- Step 2: Agent gets the overview ---
	it("Step 2: get session overview to understand what happened", async () => {
		const overview = await ctx.callTool("session_overview", { session_id: sessionId });

		// Agent sees the timeline, markers, and error count
		expect(overview).toContain("Markers:");
		expect(overview).toContain("settings form keeps failing");
		// Agent sees there were errors
		expect(overview).toMatch(/Error|422|failed/i);
	});

	// --- Step 3: Agent searches for the specific errors ---
	it("Step 3: search for 422 validation errors", async () => {
		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			status_codes: [422],
			limit: 50,
		});

		// Should find at least 2 (both attempts got 422s)
		expect(searchResult).toContain("Found");
		expect(searchResult).toContain("422");

		// Save an event ID for inspection
		errorEventId = extractEventId(searchResult);
	});

	// --- Step 4: Agent inspects the first error in detail ---
	it("Step 4: inspect the 422 response to see validation details", async () => {
		const inspectResult = await ctx.callTool("session_inspect", {
			session_id: sessionId,
			event_id: errorEventId,
			include: ["surrounding_events", "network_body"],
		});

		// Agent sees the 422 status and the settings endpoint
		expect(inspectResult).toContain("422");
		expect(inspectResult).toContain("/api/settings");
		// Agent sees surrounding context
		expect(inspectResult).toContain("Context");
	});

	// --- Step 5: Agent searches for user actions to understand the flow ---
	it("Step 5: search for user input events around the marker", async () => {
		const inputs = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["user_input"],
			limit: 20,
		});

		// Agent sees form fills and button clicks
		expect(inputs).toContain("Found");
	});

	// --- Step 6: Agent generates reproduction steps ---
	it("Step 6: generate reproduction steps for the bug report", async () => {
		const steps = await ctx.callTool("session_replay_context", {
			session_id: sessionId,
			format: "reproduction_steps",
		});

		// Reproduction steps should be numbered and actionable
		expect(steps).toMatch(/1\.\s/);
		expect(steps).toMatch(/Navigate|Click|Set/);
	});

	// --- Step 7: Agent generates a test scaffold ---
	it("Step 7: generate Playwright test scaffold", async () => {
		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: sessionId,
			format: "test_scaffold",
			test_framework: "playwright",
		});

		expect(scaffold).toContain("import { test, expect }");
		expect(scaffold).toContain("page.goto");
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
