import { describe, expect, it } from "vitest";
import { expectFrameworkContent, extractAllEventIds, extractEventId, extractMarkerId, extractSessionId, runInvestigationSequence } from "../helpers/journey-helpers.js";

// ─────────────────────────────────────────────────────────────────────────────
// extractSessionId
// ─────────────────────────────────────────────────────────────────────────────

describe("extractSessionId", () => {
	it("extracts UUID from session_list output", () => {
		const output = `Sessions (1 total):\n  • 550e8400-e29b-41d4-a716-446655440000 — localhost:3000 (2 markers)`;
		const id = extractSessionId(output);
		expect(id).toBe("550e8400-e29b-41d4-a716-446655440000");
	});

	it("throws descriptive error when no UUID found", () => {
		expect(() => extractSessionId("No sessions found.")).toThrow("Could not extract session ID");
	});

	it("extracts from minimal output with just a UUID", () => {
		const output = "id: a1b2c3d4-e5f6-7890-abcd-ef1234567890";
		expect(extractSessionId(output)).toBe("a1b2c3d4-e5f6-7890-abcd-ef1234567890");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// extractEventId
// ─────────────────────────────────────────────────────────────────────────────

describe("extractEventId", () => {
	const SAMPLE_SEARCH = `Found 3 events:

  [1] framework_state — Cart component update
      id: aaaabbbb-cccc-dddd-eeee-000011112222
      component: Cart

  [2] framework_state — Navbar re-render
      id: 11112222-3333-4444-5555-666677778888
      component: Navbar

  [3] framework_state — Product mount
      id: 99990000-1111-2222-3333-444455556666
      component: ProductCard`;

	it("extracts first event ID by default", () => {
		const id = extractEventId(SAMPLE_SEARCH);
		expect(id).toBe("aaaabbbb-cccc-dddd-eeee-000011112222");
	});

	it("extracts Nth event ID with index parameter", () => {
		const id = extractEventId(SAMPLE_SEARCH, 1);
		expect(id).toBe("11112222-3333-4444-5555-666677778888");
	});

	it("throws when index out of range", () => {
		expect(() => extractEventId(SAMPLE_SEARCH, 10)).toThrow("index 10");
	});

	it("throws descriptive error when no event IDs found", () => {
		expect(() => extractEventId("No matching events found.")).toThrow("Could not extract event ID");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// extractMarkerId
// ─────────────────────────────────────────────────────────────────────────────

describe("extractMarkerId", () => {
	it("extracts marker UUID from overview output", () => {
		const output = `Markers (2):\n  • deadbeef-dead-beef-dead-beefdeadbeef — form validation failed\n  • cafecafe-cafe-cafe-cafe-cafecafecafe — checkout complete`;
		const id = extractMarkerId(output);
		expect(id).toBe("deadbeef-dead-beef-dead-beefdeadbeef");
	});

	it("throws when no UUID found", () => {
		expect(() => extractMarkerId("No markers.")).toThrow("Could not extract marker ID");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// extractAllEventIds
// ─────────────────────────────────────────────────────────────────────────────

describe("extractAllEventIds", () => {
	it("returns all UUIDs from multi-event output with id: prefix", () => {
		const output = `Found 2 events:
      id: aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee
      id: 11111111-2222-3333-4444-555555555555`;
		const ids = extractAllEventIds(output);
		expect(ids).toHaveLength(2);
		expect(ids[0]).toBe("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
		expect(ids[1]).toBe("11111111-2222-3333-4444-555555555555");
	});

	it("returns empty array when no events", () => {
		const ids = extractAllEventIds("No matching events found.");
		expect(ids).toEqual([]);
	});

	it("falls back to any UUIDs when no id: prefix matches", () => {
		const output = `Markers:\n  • deadbeef-dead-beef-dead-beefdeadbeef — marker 1\n  • cafecafe-cafe-cafe-cafe-cafecafecafe — marker 2`;
		const ids = extractAllEventIds(output);
		expect(ids.length).toBeGreaterThanOrEqual(2);
	});

	it("deduplicates UUIDs from fallback path", () => {
		const uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
		const output = `${uuid} appears ${uuid} twice ${uuid}`;
		const ids = extractAllEventIds(output);
		// fallback deduplicates
		const unique = [...new Set(ids)];
		expect(unique).toHaveLength(ids.length);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// expectFrameworkContent
// ─────────────────────────────────────────────────────────────────────────────

describe("expectFrameworkContent", () => {
	const REACT_OUTPUT = `
framework_detect: react (v18.3.1)
Components: App, Navbar, ProductCard
framework_state: Cart — update
framework_error: InfiniteUpdater — infinite_rerender [high]
`;

	it("passes when all expectations met", () => {
		expect(() =>
			expectFrameworkContent(REACT_OUTPUT, "react", {
				hasDetection: true,
				hasStateEvents: true,
				hasErrorEvents: true,
				componentNames: ["Navbar", "Cart"],
				patternNames: ["infinite_rerender"],
			}),
		).not.toThrow();
	});

	it("throws with structured message on failure — missing framework", () => {
		expect(() => expectFrameworkContent("some output without framework name", "react", { hasDetection: true })).toThrow("Expected framework detection for 'react'");
	});

	it("throws listing all failures when multiple expectations fail", () => {
		expect(() =>
			expectFrameworkContent("empty output", "vue", {
				hasDetection: true,
				componentNames: ["MyComponent"],
				patternNames: ["some_pattern"],
			}),
		).toThrow(/vue.*\n.*MyComponent.*\n.*some_pattern/s);
	});

	it("passes for Vue detection", () => {
		const vueOutput = "framework_detect: vue (v3.5.13)\nComponents: App, TaskList";
		expect(() => expectFrameworkContent(vueOutput, "vue", { hasDetection: true, componentNames: ["TaskList"] })).not.toThrow();
	});

	it("throws with actual output context on failure", () => {
		expect(() => expectFrameworkContent("short output", "react", { hasDetection: true })).toThrow("Actual output");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// runInvestigationSequence
// ─────────────────────────────────────────────────────────────────────────────

describe("runInvestigationSequence", () => {
	const SESSION_ID = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";

	function makeMockCallTool(overrides?: Partial<Record<string, string>>) {
		const defaults: Record<string, string> = {
			session_list: `Sessions (1):\n  • ${SESSION_ID} — localhost`,
			session_overview: `Timeline:\n  [nav] /\n\nMarkers: none`,
			session_search: `Found 1 event:\n  id: eeeeeeee-ffff-0000-1111-222233334444\n  type: navigation`,
			session_inspect: `Event detail:\n  id: eeeeeeee-ffff-0000-1111-222233334444\n  type: navigation`,
		};
		const responses = { ...defaults, ...overrides };
		return async (name: string, _args: Record<string, unknown>): Promise<string> => {
			return responses[name] ?? "Unknown tool";
		};
	}

	it("returns all intermediate results", async () => {
		const results = await runInvestigationSequence(makeMockCallTool());
		expect(results.sessionId).toBe(SESSION_ID);
		expect(results.listResult).toContain(SESSION_ID);
		expect(results.overviewResult).toContain("Timeline:");
		expect(results.searchResult).toContain("Found");
		expect(results.inspectResult).toContain("Event detail:");
	});

	it("passes custom search filters through", async () => {
		let capturedArgs: Record<string, unknown> | null = null;
		const callTool = async (name: string, args: Record<string, unknown>): Promise<string> => {
			if (name === "session_search") capturedArgs = args;
			if (name === "session_list") return `Sessions (1):\n  • ${SESSION_ID} — localhost`;
			if (name === "session_overview") return "Timeline:";
			if (name === "session_search") return "Found 1 event:\n  id: eeeeeeee-ffff-0000-1111-222233334444";
			return "";
		};
		await runInvestigationSequence(callTool, { searchFilters: { framework: "react", component: "App" } });
		expect(capturedArgs).not.toBeNull();
		expect(capturedArgs?.framework).toBe("react");
		expect(capturedArgs?.component).toBe("App");
	});

	it("handles no events gracefully (empty inspectResult)", async () => {
		const results = await runInvestigationSequence(makeMockCallTool({ session_search: "No matching events found." }));
		expect(results.inspectResult).toBe("");
	});
});
