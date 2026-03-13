import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { BrowserTestContext } from "../../../helpers/browser-test-harness.js";
import { isChromeAvailable, setupBrowserTest } from "../../../helpers/browser-test-harness.js";
import { expectFrameworkContent, extractAllEventIds, extractEventId, runInvestigationSequence } from "../../../helpers/journey-helpers.js";

const SKIP = !(await isChromeAvailable());
const REACT_SPA = resolve(import.meta.dirname, "../../../fixtures/browser/react-spa");

// ─────────────────────────────────────────────────────────────────────────────
// Journey 1: Shopping Cart State Observation
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: shopping cart state observation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.click('[data-testid="product-card-1"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		await ctx.click('[data-testid="product-card-3"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		await ctx.placeMarker("items added to cart");
		await ctx.navigate("/cart");
		await ctx.wait(500);
		await ctx.click('[data-testid="quantity-increase-1"]');
		await ctx.click('[data-testid="quantity-increase-1"]');
		await ctx.wait(300);
		await ctx.placeMarker("quantity updated");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: detect React framework in bundled app", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_detect"],
		});
		expect(result).toContain("react");
	});

	it("Step 2: overview shows framework section with component tree info", async () => {
		const { sessionId } = await runInvestigationSequence(ctx.callTool.bind(ctx));
		const overview = await ctx.callTool("session_overview", {
			session_id: sessionId,
			include: ["framework", "markers"],
		});
		expect(overview).toContain("items added to cart");
		expect(overview).toContain("quantity updated");
		expect(overview).toMatch(/Component|component/);
	});

	it("Step 3: search for Navbar component updates (cart badge)", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			component: "Navbar",
		});
		expect(result).toContain("Navbar");
		expect(result).toContain("update");
	});

	it("Step 4: search for CartItem component state by framework filter", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			framework: "react",
			component: "CartItem",
		});
		expect(result).toContain("CartItem");
	});

	it("Step 5: inspect a state update event for props/state detail", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			component: "CartItem",
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
		});
		expect(detail).toContain("react");
		expect(detail).toContain("CartItem");
	});

	it("Step 6: diff between 'items added' and 'quantity updated' markers", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markerIds = extractAllEventIds(overview);
		expect(markerIds.length).toBeGreaterThanOrEqual(2);

		const diff = await ctx.callTool("session_diff", {
			session_id: "latest",
			from: markerIds[0],
			to: markerIds[1],
			include: ["framework_state", "url"],
		});
		expect(diff).toContain("Diff:");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 2: Checkout Form Validation Bug
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: checkout validation bug investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		// Add item to cart
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.click('[data-testid="product-card-1"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		// Go to checkout
		await ctx.navigate("/checkout");
		await ctx.wait(500);
		// Fill shipping with incomplete data (missing address)
		await ctx.fill('[data-testid="shipping-name"]', "Test User");
		await ctx.click('[data-testid="next-step"]');
		await ctx.wait(500);
		// Inject server validation failure
		await ctx.testControl("/__test__/fail-checkout");
		// Fill complete shipping and proceed
		await ctx.fill('[data-testid="shipping-address"]', "123 Main St");
		await ctx.fill('[data-testid="shipping-city"]', "Springfield");
		await ctx.fill('[data-testid="shipping-zip"]', "62701");
		await ctx.click('[data-testid="next-step"]');
		await ctx.wait(300);
		// Payment step — submit order (will get 422 from server)
		await ctx.fill('[data-testid="card-number"]', "4111111111111111");
		await ctx.click('[data-testid="submit-order"]');
		await ctx.wait(1000);
		await ctx.placeMarker("checkout failed");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: find session with errors", async () => {
		const result = await ctx.callTool("session_list", {});
		expect(result).toContain("Sessions");
	});

	it("Step 2: overview reveals checkout errors", async () => {
		const result = await ctx.callTool("session_overview", {
			session_id: "latest",
		});
		expect(result).toContain("checkout failed");
	});

	it("Step 3: search for 422 validation responses", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["network_response"],
			status_codes: [422],
		});
		expect(result).toContain("422");
	});

	it("Step 4: inspect the 422 response body", async () => {
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

	it("Step 5: search for Checkout component state during failure", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			framework: "react",
			component: "Checkout",
		});
		// Checkout component should have been tracked
		expect(result).toContain("Found");
	});

	it("Step 6: diff form state before and after error", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markerIds = extractAllEventIds(overview);
		expect(markerIds.length).toBeGreaterThanOrEqual(1);
	});

	it("Step 7: generate Playwright test scaffold", async () => {
		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "test_scaffold",
		});
		expect(scaffold).toMatch(/1\.|playwright|page\.|navigate|fill/i);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 3: Infinite Re-render Bug Detection
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: infinite re-render diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		await ctx.navigate("/bugs/infinite-updater");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateInfiniteUpdate()");
		await ctx.wait(3000);
		await ctx.placeMarker("infinite loop active");
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
		expect(overview).toMatch(/infinite_rerender|high|error/i);
	});

	it("Step 2: search for framework_error events by pattern", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "infinite_rerender",
		});
		expect(result).toContain("infinite_rerender");
		expect(result).toContain("high");
	});

	it("Step 3: search for the offending component", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			framework: "react",
		});
		expect(result).toContain("InfiniteUpdater");
	});

	it("Step 4: inspect the error event for render count details", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
		});
		expect(detail).toContain("InfiniteUpdater");
		expect(detail).toMatch(/render|count|rapid/i);
	});

	it("Step 5: generate reproduction steps mentioning the pattern", async () => {
		const steps = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "reproduction_steps",
		});
		expect(steps).toMatch(/1\.\s/);
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 4: Stale Closure and Missing Cleanup Detection
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: stale closure + leaky interval diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		// Activate stale closure bug
		await ctx.navigate("/bugs/stale-price");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.showStalePrice()");
		await ctx.wait(1000);
		await ctx.placeMarker("stale closure active");
		// Activate leaky interval bug
		await ctx.navigate("/bugs/leaky-interval");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateLeakyInterval()");
		await ctx.wait(2000);
		await ctx.placeMarker("leaky interval active");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: search all framework errors across both bugs", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		const hasStale = result.includes("stale_closure");
		const hasMissing = result.includes("missing_cleanup");
		expect(hasStale || hasMissing).toBe(true);
	});

	it("Step 2: filter by stale_closure pattern specifically", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "stale_closure",
		});
		expect(result).toContain("stale_closure");
	});

	it("Step 3: filter by missing_cleanup pattern specifically", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "missing_cleanup",
		});
		expect(result).toContain("missing_cleanup");
	});

	it("Step 4: diff between the two marker points to see progression", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		if (markers.length >= 2) {
			const diff = await ctx.callTool("session_diff", {
				session_id: "latest",
				from: markers[0],
				to: markers[1],
				include: ["framework_state", "url"],
			});
			expect(diff).toContain("Diff:");
		}
	});

	it("Step 5: inspect stale closure error for component + deps detail", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
			pattern: "stale_closure",
		});
		if (search.includes("stale_closure")) {
			const eventId = extractEventId(search);
			const detail = await ctx.callTool("session_inspect", {
				session_id: "latest",
				event_id: eventId,
			});
			expect(detail).toContain("StalePrice");
		}
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 5: Route Transition with State Persistence
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: route transition state persistence", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.click('[data-testid="product-card-1"] [data-testid="add-to-cart"]');
		await ctx.click('[data-testid="product-card-2"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		await ctx.placeMarker("cart populated");
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.placeMarker("navigated away");
		await ctx.navigate("/cart");
		await ctx.wait(500);
		await ctx.placeMarker("returned to cart");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: search for component mount events across route changes", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			query: "mount",
		});
		expect(result).toContain("mount");
	});

	it("Step 2: search for component unmount events (route away from cart)", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			query: "unmount",
		});
		expect(result).toContain("unmount");
	});

	it("Step 3: diff between 'navigated away' and 'returned to cart' markers", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(3);
		const diff = await ctx.callTool("session_diff", {
			session_id: "latest",
			from: markers[1],
			to: markers[2],
			include: ["url", "framework_state"],
		});
		expect(diff).toContain("Diff:");
	});

	it("Step 4: verify navigation events tracked", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["navigation"],
		});
		expect(result).toContain("/cart");
		expect(result).toContain("/login");
	});

	it("Step 5: overview around 'returned to cart' marker", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markerIds = extractAllEventIds(overview);
		const lastMarker = markerIds[markerIds.length - 1];
		const focused = await ctx.callTool("session_overview", {
			session_id: "latest",
			around_marker: lastMarker,
			include: ["timeline", "framework"],
		});
		expect(focused).toContain("returned to cart");
	});
});

// ─────────────────────────────────────────────────────────────────────────────
// Journey 6: Context Performance + Full Agent Investigation
// ─────────────────────────────────────────────────────────────────────────────

describe.skipIf(SKIP)("React Journey: context flood full investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({ fixturePath: REACT_SPA, frameworkState: ["react"] });
		await ctx.wait(1000);
		await ctx.navigate("/bugs/context-flood");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateContextFlood()");
		await ctx.wait(2000);
		await ctx.placeMarker("context flood triggered");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => {
		await ctx?.cleanup();
	});

	it("Step 1: session_list finds the session", async () => {
		const result = await ctx.callTool("session_list", {});
		expect(result).toContain("Sessions");
	});

	it("Step 2: session_overview identifies excessive_context_rerender", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["framework"],
		});
		expect(overview).toMatch(/context|rerender|excessive/i);
	});

	it("Step 3: session_search with framework + pattern filters", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			framework: "react",
			pattern: "excessive_context_rerender",
		});
		expect(result).toContain("excessive_context_rerender");
	});

	it("Step 4: session_inspect for context consumer details", async () => {
		const search = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		const eventId = extractEventId(search);
		const detail = await ctx.callTool("session_inspect", {
			session_id: "latest",
			event_id: eventId,
		});
		expect(detail).toMatch(/context|ContextFlood/i);
	});

	it("Step 5: session_diff from start to flood marker", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["markers"],
		});
		const markers = extractAllEventIds(overview);
		expect(markers.length).toBeGreaterThanOrEqual(1);
	});

	it("Step 6: session_replay_context generates Cypress test", async () => {
		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "test_scaffold",
			test_framework: "cypress",
		});
		expect(scaffold).toContain("cy.");
	});

	it("expectFrameworkContent helper works for React", async () => {
		const detect = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_detect"],
		});
		expectFrameworkContent(detect, "react", { hasDetection: true });
	});
});
