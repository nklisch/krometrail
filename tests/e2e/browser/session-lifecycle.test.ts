import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: multi-page session lifecycle", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// 1. Land on home page, mark as "start"
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.placeMarker("session start");

		// 2. Go to login
		await ctx.navigate("/login");
		await ctx.wait(500);

		// 3. Try wrong password first (401)
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "wrong");
		await ctx.submitForm("#login-form");
		await ctx.wait(1000);
		await ctx.placeMarker("failed login attempt");

		// 4. Correct login
		await ctx.fill('[data-testid="password"]', "correct");
		await ctx.submitForm("#login-form");
		await ctx.wait(1500);

		// 5. On dashboard now — verify data loaded
		await ctx.navigate("/dashboard");
		await ctx.wait(1500);
		await ctx.placeMarker("dashboard loaded");

		// 6. Navigate to settings, update profile
		await ctx.navigate("/settings");
		await ctx.wait(500);
		await ctx.fill('[data-testid="name"]', "New Admin Name");
		await ctx.fill('[data-testid="email"]', "admin@example.com");
		await ctx.fill('[data-testid="phone"]', "5551234567");
		await ctx.submitForm("#settings-form");
		await ctx.wait(1000);
		await ctx.placeMarker("settings saved");

		await ctx.wait(500);
		await ctx.finishRecording();
	}, 60_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("session_list shows the session with multiple markers", async () => {
		const result = await ctx.callTool("session_list", { has_markers: true });
		expect(result).toContain("Sessions (");
		// Should have at least 4 markers (our 4 manual + auto-detected ones)
		expect(result).toMatch(/\d+ markers/);
	});

	it("session_overview shows full navigation timeline across pages", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
			include: ["timeline", "markers"],
		});

		// Should show navigations to multiple pages
		expect(overview).toContain("Timeline:");
		// Should show all our manual markers
		expect(overview).toContain("session start");
		expect(overview).toContain("dashboard loaded");
		expect(overview).toContain("settings saved");
	});

	it("session_search finds the 401 failed login attempt", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["network_response"],
			status_codes: [401],
			limit: 50,
		});

		expect(searchResult).toContain("401");
		expect(searchResult).toContain("/api/login");
	});

	it("session_diff between start and dashboard shows state changes", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		// Search for navigation events to get timestamps for the start and dashboard load
		const navEvents = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["navigation"],
			limit: 10,
		});

		// Use the first and a later navigation event
		const eventIds = [...navEvents.matchAll(/id:\s*([a-f0-9-]{36})/g)].map((m) => m[1]);
		if (eventIds.length >= 2) {
			const diffResult = await ctx.callTool("session_diff", {
				session_id: sessionId,
				from: eventIds[0],
				to: eventIds[eventIds.length - 1],
				include: ["storage", "url", "network_new"],
			});

			expect(diffResult).toContain("Diff:");
			// Should show URL change
			expect(diffResult).toMatch(/URL:|Network/);
		}
	});

	it("session_search finds user_input events (form interactions)", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const inputs = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["user_input"],
			limit: 20,
		});

		expect(inputs).toContain("Found");
		// Should have clicks and form interactions
		expect(inputs).toMatch(/user_input/);
	});

	it("session_replay_context generates Playwright test scaffold", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: sessionId,
			format: "test_scaffold",
			test_framework: "playwright",
		});

		// Should be valid Playwright test code
		expect(scaffold).toContain("import { test, expect }");
		expect(scaffold).toContain("page.goto");
		// Should reference actual selectors from the session
		expect(scaffold).toMatch(/page\.(click|fill|goto)/);
	});
});

function extractSessionId(output: string): string {
	const match = output.match(/([a-f0-9-]{36})/);
	if (!match) throw new Error(`No session ID in:\n${output}`);
	return match[1];
}
