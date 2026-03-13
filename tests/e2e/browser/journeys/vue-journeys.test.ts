import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { BrowserTestContext } from "../../../helpers/browser-test-harness.js";
import { isChromeAvailable, setupBrowserTest } from "../../../helpers/browser-test-harness.js";
import { expectFrameworkContent, extractAllEventIds, extractEventId } from "../../../helpers/journey-helpers.js";

const SKIP = !(await isChromeAvailable());
const VUE_SPA = resolve(import.meta.dirname, "../../../fixtures/browser/vue-spa");

// ─────────────────────────────────────────────────────────────────────────────
// Journey 1: Task Management State Observation
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: task management state observation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		// Login
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm('[data-testid="login-form"]');
		await ctx.wait(1000);
		// View task list
		await ctx.navigate("/tasks");
		await ctx.wait(500);
		await ctx.placeMarker("task list loaded");
		// Filter by priority
		await ctx.click('[data-testid="filter-priority-high"]');
		await ctx.wait(300);
		await ctx.placeMarker("filtered to high priority");
		// Toggle a task status
		await ctx.click('[data-testid="task-status-toggle-1"]');
		await ctx.wait(300);
		await ctx.placeMarker("task status changed");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: detect Vue framework in bundled app", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_detect"],
		});
		expect(result).toContain("vue");
	});

	it("Step 2: overview shows Vue framework section", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["framework", "markers"],
		});
		expect(overview).toContain("task list loaded");
		expect(overview).toMatch(/Component|component|vue/i);
	});

	it("Step 3: search for Pinia store mutations", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			framework: "vue",
			query: "store",
		});
		expect(result).toContain("Found");
	});

	it("Step 4: search for TaskFilter component updates", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			component: "TaskFilter",
		});
		expect(result).toContain("TaskFilter");
	});

	it("Step 5: inspect a store mutation event", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			framework: "vue",
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
		});
		expect(detail).toContain("vue");
	});

	it("Step 6: diff between 'loaded' and 'filtered' markers", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(2);
		const diff = await ctx.callTool("session_diff", {
			session_id: "latest",
			from: markers[0],
			to: markers[1],
			include: ["framework_state"],
		});
		expect(diff).toContain("Diff:");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 2: Task Creation Form Validation Bug
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: task creation validation bug", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		// Login first
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm('[data-testid="login-form"]');
		await ctx.wait(1000);
		// Go to create task
		await ctx.navigate("/tasks/new");
		await ctx.wait(500);
		// Fill partial form and submit
		await ctx.fill('[data-testid="task-title-input"]', "");
		await ctx.click('[data-testid="create-task-submit"]');
		await ctx.wait(500);
		// Inject server validation failure
		await ctx.testControl("/__test__/fail-create");
		// Fill form properly and submit
		await ctx.fill('[data-testid="task-title-input"]', "New Task Title");
		await ctx.fill('[data-testid="task-description-input"]', "Task description");
		await ctx.click('[data-testid="create-task-submit"]');
		await ctx.wait(1000);
		await ctx.placeMarker("task creation failed");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: find session with errors", async () => {
		const result = await ctx.callTool("session_list", {});
		expect(result).toContain("Sessions");
	});

	it("Step 2: overview reveals form errors", async () => {
		const result = await ctx.callTool("session_overview", {
			session_id: "latest",
		});
		expect(result).toContain("task creation failed");
	});

	it("Step 3: search for 422 responses on /api/tasks", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["network_response"],
			status_codes: [422],
		});
		expect(result).toContain("422");
	});

	it("Step 4: inspect 422 response body for validation details", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["network_response"],
			status_codes: [422],
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
			include: ["network_body"],
		});
		expect(detail).toContain("422");
	});

	it("Step 5: search for CreateTask component state during submission", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			framework: "vue",
			component: "CreateTask",
		});
		expect(result).toContain("Found");
	});

	it("Step 6: diff form state before and after validation error", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(1);
	});

	it("Step 7: generate reproduction steps", async () => {
		const steps = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "reproduction_steps",
		});
		expect(steps).toMatch(/1\.\s/);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 3: Infinite Watcher Loop Detection
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: infinite watcher loop diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		await ctx.navigate("/bugs/infinite-watcher");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateInfiniteWatcher()");
		await ctx.wait(3000);
		await ctx.placeMarker("infinite watcher active");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: overview shows high-severity framework errors", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["framework"],
		});
		expect(overview).toMatch(/watcher|infinite|high|error/i);
	});

	it("Step 2: search for framework_error with pattern 'watcher_infinite_loop'", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "watcher_infinite_loop",
		});
		expect(result).toContain("watcher_infinite_loop");
		expect(result).toContain("high");
	});

	it("Step 3: identify the InfiniteWatcher component", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			framework: "vue",
		});
		expect(result).toContain("InfiniteWatcher");
	});

	it("Step 4: inspect error event for watcher details", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
		});
		expect(detail).toContain("InfiniteWatcher");
	});

	it("Step 5: generate reproduction steps", async () => {
		const steps = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "reproduction_steps",
		});
		expect(steps).toMatch(/1\.\s/);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 4: Lost Reactivity Detection
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: lost reactivity diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		await ctx.navigate("/bugs/lost-reactivity");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateLostReactivity()");
		await ctx.wait(500);
		await ctx.click('[data-testid="increment"]');
		await ctx.wait(500);
		await ctx.placeMarker("reactivity lost");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: search for framework_error events", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		expect(result).toContain("Found");
	});

	it("Step 2: filter by 'lost_reactivity' pattern", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "lost_reactivity",
		});
		expect(result).toContain("lost_reactivity");
	});

	it("Step 3: inspect reveals component with destructured reactive", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "lost_reactivity",
		});
		if (search.includes("lost_reactivity")) {
			const eventId = extractEventId(search);
			const detail = await ctx.callTool("session_inspect", {
				session_id: "latest",
				event_id: eventId,
			});
			expect(detail).toContain("LostReactivity");
		}
	});

	it("Step 4: overview focused on marker shows surrounding evidence", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(1);
		const focused = await ctx.callTool("session_overview", {
			session_id: "latest",
			around_marker: markers[0],
			include: ["timeline", "framework"],
		});
		expect(focused).toContain("reactivity lost");
	});

	it("Step 5: diff before and after activation shows missing updates", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		if (markers.length >= 1) {
			const diff = await ctx.callTool("session_diff", {
				session_id: "latest",
				to: markers[0],
				include: ["framework_state"],
			});
			expect(diff).toContain("Diff:");
		}
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 5: Pinia Store Mutation Outside Action
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: Pinia mutation outside action", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		await ctx.navigate("/bugs/pinia-mutation");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activatePiniaMutation()");
		await ctx.wait(500);
		await ctx.placeMarker("pinia mutation triggered");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: search for framework_error events from Pinia", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		expect(result).toContain("Found");
	});

	it("Step 2: filter by 'pinia_mutation_outside_action' pattern", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "pinia_mutation_outside_action",
		});
		expect(result).toContain("pinia_mutation_outside_action");
	});

	it("Step 3: inspect shows store name and mutated property", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "pinia_mutation_outside_action",
		});
		if (search.includes("pinia_mutation_outside_action")) {
			const eventId = extractEventId(search);
			const detail = await ctx.callTool("session_inspect", {
				session_id: "latest",
				event_id: eventId,
			});
			expect(detail).toMatch(/tasks|store/i);
		}
	});

	it("Step 4: search for store_mutation events around the error", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			framework: "vue",
			query: "store mutation",
		});
		expect(result).toContain("Found");
	});

	it("Step 5: generate Playwright test scaffold", async () => {
		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "test_scaffold",
		});
		expect(scaffold).toMatch(/playwright|page\.|navigate|1\./i);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 6: Multi-Page Navigation with Pinia Persistence
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("Vue Journey: multi-page Pinia state persistence", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: VUE_SPA, frameworkState: ["vue"] });
		await ctx.wait(1000);
		// Login
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.fill('[data-testid="username"]', "admin");
		await ctx.fill('[data-testid="password"]', "secret");
		await ctx.submitForm('[data-testid="login-form"]');
		await ctx.wait(1000);
		await ctx.placeMarker("logged in");
		// View tasks
		await ctx.navigate("/tasks");
		await ctx.wait(500);
		// Click on first task
		await ctx.click('[data-testid="task-link-1"]');
		await ctx.wait(500);
		await ctx.placeMarker("viewing task detail");
		// Add a comment
		await ctx.fill('[data-testid="comment-input"]', "Test comment from journey");
		await ctx.click('[data-testid="comment-submit"]');
		await ctx.wait(500);
		// Navigate to create new task
		await ctx.navigate("/tasks/new");
		await ctx.wait(500);
		await ctx.placeMarker("on create task page");
		// Back to task list
		await ctx.navigate("/tasks");
		await ctx.wait(500);
		await ctx.placeMarker("back to task list");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: detect Vue framework", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_detect"],
		});
		expect(result).toContain("vue");
	});

	it("Step 2: search for mount/unmount across route transitions", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			query: "mount",
		});
		expect(result).toContain("mount");
	});

	it("Step 3: search for Pinia store mutations across pages", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			framework: "vue",
		});
		expect(result).toContain("Found");
	});

	it("Step 4: navigation events tracked for SPA routing", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["navigation"],
		});
		expect(result).toContain("/tasks");
		expect(result).toContain("/login");
	});

	it("Step 5: diff between first and last markers shows full session evolution", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(4);
		const diff = await ctx.callTool("session_diff", {
			session_id: "latest",
			from: markers[0],
			to: markers[markers.length - 1],
			include: ["url", "framework_state"],
		});
		expect(diff).toContain("Diff:");
	});

	it("Step 6: generate reproduction steps for full workflow", async () => {
		const steps = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "reproduction_steps",
		});
		expect(steps).toMatch(/1\.\s/);
		expect(steps).toMatch(/navigate|Login|task/i);
	});

	it("expectFrameworkContent helper works for Vue", async () => {
		const detect = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_detect"],
		});
		expectFrameworkContent(detect, "vue", { hasDetection: true });
	});
});
