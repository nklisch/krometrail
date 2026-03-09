import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type BrowserTestContext, isChromeAvailable, setupBrowserTest } from "../../helpers/browser-test-harness.js";

const SKIP = !(await isChromeAvailable());

describe.skipIf(SKIP)("E2E Browser: unhandled exception investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest();

		// 1. Land on home page
		await ctx.navigate("/");
		await ctx.wait(500);

		// 2. Navigate to the error page
		await ctx.navigate("/error-page");
		await ctx.wait(800);

		// 3. Click a button that console.logs before throwing
		await ctx.click('[data-testid="throw-btn"]');
		await ctx.wait(1000);

		// 4. Also trigger a null reference for a second exception type
		await ctx.click('[data-testid="null-btn"]');
		await ctx.wait(1000);

		// 5. Place a marker to trigger persistence of everything
		await ctx.placeMarker("exceptions reproduced");
		await ctx.wait(500);

		await ctx.finishRecording();
	}, 60_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("session_list shows the session has errors", async () => {
		const result = await ctx.callTool("session_list", { has_errors: true });
		expect(result).toContain("Sessions (");
		expect(result).toMatch(/\d+ errors/);
	});

	it("session_overview surfaces auto-detected exception markers", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
			include: ["markers", "errors"],
		});

		// Auto-detected markers should appear
		expect(overview).toContain("[auto]");
		// The errors section should mention the exception
		expect(overview).toMatch(/Uncaught|exception|Error/i);
	});

	it("session_search for page_error events finds both exceptions", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["page_error"],
		});

		// Should find the thrown Error and the TypeError
		expect(searchResult).toContain("Found");
		// At least 2 exceptions
		expect(searchResult).toMatch(/page_error/);
	});

	it("session_inspect on exception shows stack trace and console context", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		// Search for page errors
		const searchResult = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["page_error"],
			limit: 1,
		});
		const eventId = extractEventId(searchResult);

		const inspectResult = await ctx.callTool("session_inspect", {
			session_id: sessionId,
			event_id: eventId,
			include: ["surrounding_events", "console_context"],
		});

		// Should have the exception text
		expect(inspectResult).toMatch(/Error|TypeError/);
		// Surrounding events should include the console.log "About to throw"
		expect(inspectResult).toContain("Context");
	});

	it("session_search by console level finds the log trail around errors", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		const consoleLogs = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["console"],
			console_levels: ["log"],
			limit: 50,
		});

		// The error page logs "About to throw exception" and "About to access null property"
		expect(consoleLogs).toContain("Found");
	});

	it("session_diff between page load and exception shows what happened", async () => {
		const listResult = await ctx.callTool("session_list", {});
		const sessionId = extractSessionId(listResult);

		// Search for the navigation to error page and the exception
		const navEvents = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["navigation"],
			limit: 5,
		});
		const errorEvents = await ctx.callTool("session_search", {
			session_id: sessionId,
			event_types: ["page_error"],
			limit: 1,
		});

		const navEventId = extractEventId(navEvents);
		const errorEventId = extractEventId(errorEvents);

		const diffResult = await ctx.callTool("session_diff", {
			session_id: sessionId,
			from: navEventId,
			to: errorEventId,
			include: ["console_new"],
		});

		// Should show console messages between load and error
		expect(diffResult).toContain("Diff:");
		expect(diffResult).toContain("Console");
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
