# Design: Phase 18 — Browser Journey Test Suite (React & Vue)

## Overview

Comprehensive end-to-end journey test suite that validates realistic, multi-step agent debugging workflows against real React and Vue applications. Each journey simulates how an agent would actually use the browser tools — starting Chrome, recording user interactions, placing markers, then investigating via the full MCP tool chain (`session_list` → `session_overview` → `session_search` → `session_inspect` → `session_diff` → `session_replay_context`).

**Key differences from existing browser e2e tests:**
- **Realistic SPAs** — Vite-bundled React and Vue apps with routing, stores, API calls, error boundaries (not CDN toy apps)
- **Journey-oriented** — Each test is a multi-step narrative (5-10 sequential steps) modeling an agent's progressive discovery
- **Framework-specific tool validation** — Tests the `framework`, `component`, and `pattern` search filters, framework sections in `session_overview`, and framework state in `session_diff`
- **Full tool chain** — Every journey exercises the complete MCP investigation workflow end-to-end

**Depends on:** Phases 15-17 (React/Vue observers, framework-aware investigation tools)

---

## Implementation Units

### Unit 1: React SPA Fixture App

**Directory**: `tests/fixtures/browser/react-spa/`

A realistic single-page React app with routing, global state, API interactions, and intentional bug patterns. Built with Vite for production-like bundled output.

**App structure:**

```
tests/fixtures/browser/react-spa/
  package.json          # react, react-dom, react-router-dom, zustand, vite
  vite.config.ts        # dev server config
  server.ts             # Bun server: serves Vite build + API endpoints
  index.html            # Vite entry point
  src/
    main.tsx            # React root + router setup
    store.ts            # Zustand store (cart + auth state)
    api.ts              # Fetch wrappers for /api/* endpoints
    pages/
      Home.tsx          # Product listing, add-to-cart buttons
      ProductDetail.tsx # Single product view with reviews
      Cart.tsx          # Cart items, quantity update, checkout
      Checkout.tsx      # Multi-step form: shipping → payment → confirm
      Login.tsx         # Auth form
      ErrorBoundary.tsx # React error boundary wrapper
    components/
      Navbar.tsx        # Nav + cart badge (reads store)
      ProductCard.tsx   # Card component used in listing
      CartItem.tsx      # Single cart row with quantity controls
      PriceDisplay.tsx  # Formats price (intentional stale closure bug)
    bugs/
      InfiniteUpdater.tsx   # useEffect that triggers its own dependency
      StalePrice.tsx        # Closure over stale cart total
      LeakyInterval.tsx     # setInterval without cleanup in useEffect
      ContextFlood.tsx      # Theme context consumed by 30+ components
```

**Pages and routing:**

| Route | Component | Purpose |
|-------|-----------|---------|
| `/` | `Home` | Grid of products. Each has an "Add to Cart" button. |
| `/product/:id` | `ProductDetail` | Shows single product, reviews, "Add to Cart" |
| `/cart` | `Cart` | Lists cart items. Quantity +/- buttons, remove, "Checkout" |
| `/checkout` | `Checkout` | 3-step form: shipping address → payment → confirmation |
| `/login` | `Login` | Username/password form, sets auth token in store |
| `/bugs/:name` | (dynamic) | Renders a specific bug component for pattern detection tests |

**Zustand store** (`store.ts`):

```typescript
interface AppState {
	// Auth
	token: string | null;
	user: { id: number; name: string } | null;
	login: (username: string, password: string) => Promise<void>;
	logout: () => void;

	// Cart
	items: Array<{ productId: number; name: string; price: number; quantity: number }>;
	addToCart: (product: { id: number; name: string; price: number }) => void;
	updateQuantity: (productId: number, quantity: number) => void;
	removeFromCart: (productId: number) => void;
	clearCart: () => void;

	// Checkout
	shippingAddress: Record<string, string> | null;
	setShippingAddress: (address: Record<string, string>) => void;
	submitOrder: () => Promise<{ orderId: string }>;
}
```

**API endpoints** (in `server.ts`):

| Endpoint | Method | Response | Test Controls |
|----------|--------|----------|---------------|
| `/api/products` | GET | Array of 6 products with id, name, price, image | - |
| `/api/products/:id` | GET | Single product with reviews | - |
| `/api/login` | POST | `{ token, user }` or 401 | - |
| `/api/checkout` | POST | `{ orderId }` or 422 (validation) | `/__test__/fail-checkout` |
| `/api/checkout` | POST | 500 on injected server error | `/__test__/error-checkout` |
| `/__test__/fail-checkout` | GET | Sets next checkout to return 422 | - |
| `/__test__/error-checkout` | GET | Sets next checkout to return 500 | - |
| `/__test__/slow-api` | GET | Sets delay on all API responses (ms param) | - |
| `/__test__/reset` | GET | Resets all test control flags | - |

**Bug components** (in `bugs/`):

1. **InfiniteUpdater** — `useEffect` with `count` in deps that calls `setCount(count + 1)`. Exposed via `window.__TEST_CONTROLS__.activateInfiniteUpdate()`
2. **StalePrice** — Captures cart total in a `useCallback` with empty deps. Shows stale total after cart changes. Exposed via `window.__TEST_CONTROLS__.showStalePrice()`
3. **LeakyInterval** — `useEffect` with `setInterval` but no cleanup return. Force re-mount to create multiple intervals. Exposed via `window.__TEST_CONTROLS__.activateLeakyInterval()`
4. **ContextFlood** — A `ThemeContext.Provider` with `value={{theme, toggleTheme}}` (object literal in render = new ref every render). 30 child consumers all re-render. Exposed via `window.__TEST_CONTROLS__.activateContextFlood()`

**server.ts contract:**
- Takes port as first CLI arg (0 = random)
- Runs `vite build` on first start if `dist/` doesn't exist
- Serves `dist/` as static files
- Serves API endpoints from Bun.serve
- Prints `READY:<port>` to stdout

```typescript
// server.ts structure
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";

const __dirname = dirname(new URL(import.meta.url).pathname);
const port = Number.parseInt(process.argv[2] ?? "0", 10);

// Build on first run
if (!existsSync(join(__dirname, "dist"))) {
	const result = Bun.spawnSync(["bunx", "vite", "build"], { cwd: __dirname, stdout: "pipe", stderr: "pipe" });
	if (result.exitCode !== 0) throw new Error(`Vite build failed: ${result.stderr}`);
}

// Test control state
let failCheckout = false;
let errorCheckout = false;
let apiDelayMs = 0;

const server = Bun.serve({
	port,
	async fetch(req) {
		const url = new URL(req.url);

		// API routes (before static file serving)
		if (url.pathname.startsWith("/api/")) { /* ... */ }
		if (url.pathname.startsWith("/__test__/")) { /* ... */ }

		// SPA fallback: serve dist/index.html for all routes
		const filePath = join(__dirname, "dist", url.pathname === "/" ? "index.html" : url.pathname);
		if (existsSync(filePath)) {
			return new Response(Bun.file(filePath));
		}
		// SPA fallback for client-side routes
		return new Response(Bun.file(join(__dirname, "dist/index.html")), {
			headers: { "Content-Type": "text/html" },
		});
	},
});

process.stdout.write(`READY:${server.port}\n`);
```

**Acceptance Criteria:**
- [ ] `bun install && bun run server.ts 0` starts and prints `READY:<port>`
- [ ] All routes render correct components in headless Chrome
- [ ] Zustand store mutations are observable by React devtools hook
- [ ] API endpoints return correct responses and respect test controls
- [ ] Bug components trigger detectable framework_error events when activated
- [ ] All interactive elements have `data-testid` attributes

---

### Unit 2: Vue SPA Fixture App

**Directory**: `tests/fixtures/browser/vue-spa/`

A realistic Vue 3 single-page app with Vue Router, Pinia stores, composables, and intentional bug patterns. Vite-bundled.

**App structure:**

```
tests/fixtures/browser/vue-spa/
  package.json          # vue, vue-router, pinia, vite
  vite.config.ts
  server.ts             # Bun server: serves Vite build + API endpoints
  index.html
  src/
    main.ts             # Vue app + router + Pinia setup
    stores/
      auth.ts           # Pinia auth store (login/logout/token)
      tasks.ts          # Pinia task store (CRUD, filters, stats)
    composables/
      useApi.ts         # Fetch wrapper with loading/error state
    router.ts           # Vue Router config
    pages/
      Home.vue          # Task dashboard with stats cards
      TaskList.vue      # Filtered task list with inline edit
      TaskDetail.vue    # Single task view with comments, status changes
      CreateTask.vue    # Multi-field task creation form
      Login.vue         # Auth form
      Settings.vue      # User preferences with reactive form
    components/
      AppNav.vue        # Navigation bar with auth state
      TaskCard.vue      # Task summary card
      TaskFilter.vue    # Filter controls (status, priority, search)
      StatCard.vue      # Dashboard stat display
      CommentThread.vue # Nested comments list
    bugs/
      InfiniteWatcher.vue  # watch() that mutates its own source
      LostReactivity.vue   # Destructured reactive() losing reactivity
      PiniaMutationOutsideAction.vue  # Direct store.$state mutation
```

**Pages and routing:**

| Route | Component | Purpose |
|-------|-----------|---------|
| `/` | `Home` | Dashboard with stat cards (total tasks, completed, overdue) |
| `/tasks` | `TaskList` | Filterable/sortable task list, inline status toggle |
| `/tasks/:id` | `TaskDetail` | Full task view with comments, status change dropdown |
| `/tasks/new` | `CreateTask` | Form: title, description, priority, due date, assignee |
| `/login` | `Login` | Username/password form |
| `/settings` | `Settings` | Preferences form with reactive validation |
| `/bugs/:name` | (dynamic) | Bug component for pattern detection |

**Pinia stores:**

```typescript
// stores/tasks.ts
interface Task {
	id: number;
	title: string;
	description: string;
	status: "todo" | "in-progress" | "done";
	priority: "low" | "medium" | "high";
	assignee: string;
	dueDate: string;
	comments: Array<{ id: number; text: string; author: string; createdAt: string }>;
}

interface TaskStore {
	tasks: Task[];
	filter: { status: string | null; priority: string | null; search: string };
	// Getters
	filteredTasks: Task[];
	stats: { total: number; completed: number; overdue: number; byPriority: Record<string, number> };
	// Actions
	fetchTasks(): Promise<void>;
	createTask(data: Omit<Task, "id" | "comments">): Promise<Task>;
	updateTask(id: number, patch: Partial<Task>): Promise<void>;
	deleteTask(id: number): Promise<void>;
	addComment(taskId: number, text: string): Promise<void>;
	setFilter(filter: Partial<TaskStore["filter"]>): void;
}

// stores/auth.ts
interface AuthStore {
	token: string | null;
	user: { id: number; name: string } | null;
	isAuthenticated: boolean;  // getter
	login(username: string, password: string): Promise<void>;
	logout(): void;
}
```

**API endpoints** (in `server.ts`):

| Endpoint | Method | Response | Test Controls |
|----------|--------|----------|---------------|
| `/api/tasks` | GET | Array of 8 seeded tasks | - |
| `/api/tasks/:id` | GET | Single task with comments | - |
| `/api/tasks` | POST | Created task or 422 | `/__test__/fail-create` |
| `/api/tasks/:id` | PATCH | Updated task or 422 | - |
| `/api/tasks/:id` | DELETE | 204 | - |
| `/api/tasks/:id/comments` | POST | Created comment | - |
| `/api/login` | POST | `{ token, user }` or 401 | - |
| `/api/settings` | PUT | Updated settings or 422 | `/__test__/fail-settings` |
| `/__test__/fail-create` | GET | Flags next POST /api/tasks to 422 | - |
| `/__test__/fail-settings` | GET | Flags next PUT /api/settings to 422 | - |
| `/__test__/slow-api` | GET | Sets global API delay | - |
| `/__test__/reset` | GET | Resets all flags | - |

**Bug components:**

1. **InfiniteWatcher** — `watch(count, (val) => { count.value = val + 1 })`. Exposed via `window.__TEST_CONTROLS__.activateInfiniteWatcher()`
2. **LostReactivity** — `const { x, y } = reactive({ x: 0, y: 0 })` — destructures away reactivity. Exposed via `window.__TEST_CONTROLS__.activateLostReactivity()`
3. **PiniaMutationOutsideAction** — Directly assigns `store.$state.count++` outside an action. Exposed via `window.__TEST_CONTROLS__.activatePiniaMutation()`

**Acceptance Criteria:**
- [ ] `bun install && bun run server.ts 0` starts and prints `READY:<port>`
- [ ] All routes render correct components
- [ ] Pinia store mutations observable by Vue devtools hook
- [ ] API endpoints functional with test controls
- [ ] Bug components trigger framework_error events
- [ ] All interactive elements have `data-testid` attributes

---

### Unit 3: Shared Journey Test Helpers

**File**: `tests/helpers/journey-helpers.ts`

Utilities shared across all journey tests to reduce boilerplate for common multi-step patterns.

```typescript
/**
 * Extract a session ID (UUID) from MCP tool output.
 * Throws with context if not found.
 */
export function extractSessionId(output: string): string;

/**
 * Extract an event ID (UUID) from search results.
 * Optionally specify which match (0-indexed) when multiple events are present.
 */
export function extractEventId(output: string, index?: number): string;

/**
 * Extract a marker ID from overview/search output.
 */
export function extractMarkerId(output: string): string;

/**
 * Extract all event IDs from search results as an array.
 */
export function extractAllEventIds(output: string): string[];

/**
 * Assert that an MCP tool result contains framework-specific content.
 * Provides better error messages than raw string matching.
 */
export function expectFrameworkContent(
	result: string,
	framework: "react" | "vue",
	expectations: {
		hasDetection?: boolean;
		hasStateEvents?: boolean;
		hasErrorEvents?: boolean;
		componentNames?: string[];
		patternNames?: string[];
	},
): void;

/**
 * Run the standard "find session → overview → search → inspect" sequence.
 * Returns all intermediate results for further assertions.
 */
export async function runInvestigationSequence(
	callTool: (name: string, args: Record<string, unknown>) => Promise<string>,
	options?: {
		searchFilters?: Record<string, unknown>;
		inspectIncludes?: string[];
	},
): Promise<{
	sessionId: string;
	listResult: string;
	overviewResult: string;
	searchResult: string;
	inspectResult: string;
}>;
```

**Implementation Notes:**
- `extractSessionId` / `extractEventId` / `extractMarkerId` are extracted from the existing duplicated helpers in current tests (agent-workflow.test.ts, form-validation-bug.test.ts, session-lifecycle.test.ts, react-observer.test.ts all define their own copies)
- `expectFrameworkContent` provides structured assertion output: instead of `expect(result).toContain("react")` which gives no context on failure, this reports exactly which expectations failed
- `runInvestigationSequence` encapsulates the repeated pattern of list→overview→search→inspect that appears in nearly every journey, returning all results for custom assertions

**Acceptance Criteria:**
- [ ] All extraction functions throw descriptive errors when patterns aren't found
- [ ] `runInvestigationSequence` works with both React and Vue sessions
- [ ] `expectFrameworkContent` produces clear assertion failure messages

---

### Unit 4: Browser Test Harness Extension for Vite Fixtures

**File**: `tests/helpers/browser-test-harness.ts` (extend existing)

The existing `setupBrowserTest` calls `startFixtureServer` which runs `bun run server.ts 0`. This works for Vite fixtures too since they have the same `server.ts` + `READY:<port>` contract. However, the Vite fixtures need a build step first. The existing harness already auto-installs `node_modules` — extend it to also auto-build.

```typescript
// Add to the existing setupBrowserTest function, after the bun install block:

// Auto-build Vite fixtures if dist/ doesn't exist
const distDir = join(fixtureDir, "dist");
const viteConfig = join(fixtureDir, "vite.config.ts");
if (existsSync(viteConfig) && !existsSync(distDir)) {
	const buildResult = Bun.spawnSync(["bunx", "vite", "build"], {
		cwd: fixtureDir,
		stdout: "pipe",
		stderr: "pipe",
	});
	if (buildResult.exitCode !== 0) {
		throw new Error(`Vite build failed in ${fixtureDir}: ${buildResult.stderr}`);
	}
}
```

**Acceptance Criteria:**
- [ ] Vite fixtures auto-build on first test run
- [ ] Subsequent runs skip the build (dist/ exists)
- [ ] No changes needed in test files — same `setupBrowserTest({ fixturePath })` API

---

### Unit 5: React Journey Tests

**File**: `tests/e2e/browser/journeys/react-journeys.test.ts`

Six journey tests, each a sequential multi-step scenario modeling a realistic agent workflow.

```typescript
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { BrowserTestContext } from "../../../helpers/browser-test-harness.js";
import { isChromeAvailable, setupBrowserTest } from "../../../helpers/browser-test-harness.js";
import {
	extractAllEventIds,
	extractEventId,
	extractMarkerId,
	extractSessionId,
	expectFrameworkContent,
	runInvestigationSequence,
} from "../../../helpers/journey-helpers.js";

const SKIP = !(await isChromeAvailable());
const REACT_SPA = resolve(import.meta.dirname, "../../../fixtures/browser/react-spa");
```

---

#### Journey 1: Shopping Cart State Observation

**Scenario:** Agent records a user browsing products, adding items to cart, and updating quantities. Agent then investigates the component tree and state mutations.

```typescript
describe.skipIf(SKIP)("React Journey: shopping cart state observation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
		// User flow: browse → add to cart → update quantity → view cart
		await ctx.wait(1000); // Wait for React mount
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

	afterAll(async () => { await ctx?.cleanup(); });

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
		// Framework section shows top components
		expect(overview).toMatch(/Component|component/);
	});

	it("Step 3: search for Navbar component updates (cart badge)", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_state"],
			component: "Navbar",
		});
		// Navbar should re-render when cart items change
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
```

**Acceptance Criteria:**
- [ ] Framework detection works with Vite-bundled React
- [ ] Component state updates tracked across route navigation
- [ ] Zustand store mutations generate observable framework_state events
- [ ] session_diff works between markers

---

#### Journey 2: Checkout Form Validation Bug

**Scenario:** Agent records a user going through checkout, hitting validation errors, and investigates the API failures alongside component state.

```typescript
describe.skipIf(SKIP)("React Journey: checkout validation bug investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
		await ctx.wait(1000);
		// Add item to cart
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.click('[data-testid="product-card-1"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		// Go to checkout
		await ctx.navigate("/checkout");
		await ctx.wait(500);
		// Fill shipping with incomplete data
		await ctx.fill('[data-testid="shipping-name"]', "Test User");
		await ctx.fill('[data-testid="shipping-address"]', "");  // Missing
		await ctx.click('[data-testid="next-step"]');
		await ctx.wait(500);
		// Inject server error for next attempt
		await ctx.testControl("/__test__/fail-checkout");
		// Fill properly and submit
		await ctx.fill('[data-testid="shipping-address"]', "123 Main St");
		await ctx.fill('[data-testid="shipping-city"]', "Springfield");
		await ctx.fill('[data-testid="shipping-zip"]', "62701");
		await ctx.click('[data-testid="next-step"]');
		await ctx.wait(300);
		// Payment step
		await ctx.fill('[data-testid="card-number"]', "4111111111111111");
		await ctx.click('[data-testid="submit-order"]');
		await ctx.wait(1000);
		await ctx.placeMarker("checkout failed");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => { await ctx?.cleanup(); });

	it("Step 1: find session with errors", async () => { /* session_list */ });
	it("Step 2: overview reveals checkout errors", async () => { /* session_overview */ });
	it("Step 3: search for 422 validation responses", async () => { /* session_search with status_codes */ });
	it("Step 4: inspect the 422 response body", async () => { /* session_inspect with network_body */ });
	it("Step 5: search for Checkout component state during failure", async () => {
		// session_search with framework: "react", component: "Checkout"
	});
	it("Step 6: diff form state before and after error", async () => { /* session_diff */ });
	it("Step 7: generate Playwright test scaffold", async () => { /* session_replay_context */ });
});
```

**Acceptance Criteria:**
- [ ] Multi-step checkout form state tracked across steps
- [ ] 422 responses from injected test control detected
- [ ] Component state and network errors correlated in timeline
- [ ] Test scaffold generated with checkout-specific selectors

---

#### Journey 3: Infinite Re-render Bug Detection

**Scenario:** Agent activates a known infinite re-render bug and uses framework-aware tools to diagnose it.

```typescript
describe.skipIf(SKIP)("React Journey: infinite re-render diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
		await ctx.wait(1000);
		await ctx.navigate("/bugs/infinite-updater");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateInfiniteUpdate()");
		await ctx.wait(3000); // Let it loop
		await ctx.placeMarker("infinite loop active");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => { await ctx?.cleanup(); });

	it("Step 1: overview shows high-severity framework errors", async () => {
		const overview = await ctx.callTool("session_overview", {
			session_id: "latest",
			include: ["framework"],
		});
		expect(overview).toMatch(/infinite_rerender|high/i);
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
		// Should show render count or rapid-fire update evidence
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
```

**Acceptance Criteria:**
- [ ] Infinite re-render detected with high severity in Vite-bundled app
- [ ] Pattern filter isolates infinite_rerender events
- [ ] Component name (InfiniteUpdater) identified in error events
- [ ] Inspect shows actionable detail about the render loop

---

#### Journey 4: Stale Closure and Missing Cleanup Detection

**Scenario:** Agent activates stale closure and leaky interval bugs, uses framework tools to find and differentiate them.

```typescript
describe.skipIf(SKIP)("React Journey: stale closure + leaky interval diagnosis", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
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

	afterAll(async () => { await ctx?.cleanup(); });

	it("Step 1: search all framework errors across both bugs", async () => {
		const result = await ctx.callTool("session_search", {
			session_id: "latest",
			event_types: ["framework_error"],
		});
		// Should find both stale_closure and missing_cleanup patterns
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
		// Shows how the session state changed between the two bugs being activated
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
```

**Acceptance Criteria:**
- [ ] Both stale_closure and missing_cleanup patterns detected
- [ ] Pattern filter correctly isolates each bug type
- [ ] Diff between markers shows state progression
- [ ] Inspect reveals component name and pattern details

---

#### Journey 5: Route Transition with State Persistence

**Scenario:** Agent records a user navigating between routes and verifies that store state persists correctly. Tests component mount/unmount cycle observation.

```typescript
describe.skipIf(SKIP)("React Journey: route transition state persistence", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
		await ctx.wait(1000);
		// Build up cart state
		await ctx.navigate("/");
		await ctx.wait(500);
		await ctx.click('[data-testid="product-card-1"] [data-testid="add-to-cart"]');
		await ctx.click('[data-testid="product-card-2"] [data-testid="add-to-cart"]');
		await ctx.wait(300);
		await ctx.placeMarker("cart populated");
		// Navigate away and back
		await ctx.navigate("/login");
		await ctx.wait(500);
		await ctx.placeMarker("navigated away");
		await ctx.navigate("/cart");
		await ctx.wait(500);
		await ctx.placeMarker("returned to cart");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => { await ctx?.cleanup(); });

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
		// Diff: navigated_away → returned_to_cart
		const diff = await ctx.callTool("session_diff", {
			session_id: "latest",
			from: markers[1], // navigated away
			to: markers[2],   // returned to cart
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
```

**Acceptance Criteria:**
- [ ] Mount/unmount events tracked during route transitions
- [ ] Navigation events captured for SPA client-side routing
- [ ] Store state persists across route changes (observable in framework events)
- [ ] Marker-focused overview narrows context window correctly

---

#### Journey 6: Context Performance + Full Agent Investigation

**Scenario:** Agent activates the context flood bug, then performs the complete investigation workflow from discovery through test scaffold generation.

```typescript
describe.skipIf(SKIP)("React Journey: context flood full investigation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: REACT_SPA,
			frameworkState: ["react"],
		});
		await ctx.wait(1000);
		await ctx.navigate("/bugs/context-flood");
		await ctx.wait(500);
		await ctx.evaluate("window.__TEST_CONTROLS__.activateContextFlood()");
		await ctx.wait(2000);
		await ctx.placeMarker("context flood triggered");
		await ctx.finishRecording();
	}, 90_000);

	afterAll(async () => { await ctx?.cleanup(); });

	it("Step 1: session_list finds the session", async () => { /* ... */ });
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
	it("Step 4: session_inspect for context consumer details", async () => { /* ... */ });
	it("Step 5: session_diff from start to flood", async () => { /* ... */ });
	it("Step 6: session_replay_context generates Cypress test", async () => {
		const scaffold = await ctx.callTool("session_replay_context", {
			session_id: "latest",
			format: "test_scaffold",
			test_framework: "cypress",
		});
		expect(scaffold).toContain("cy.");
	});
});
```

**Acceptance Criteria:**
- [ ] Excessive context re-render pattern detected
- [ ] Framework + pattern compound filter works
- [ ] Cypress test scaffold generated (not just Playwright)
- [ ] Full investigation workflow executes end-to-end

---

### Unit 6: Vue Journey Tests

**File**: `tests/e2e/browser/journeys/vue-journeys.test.ts`

Six journey tests mirroring the React journeys but for Vue-specific patterns.

```typescript
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { BrowserTestContext } from "../../../helpers/browser-test-harness.js";
import { isChromeAvailable, setupBrowserTest } from "../../../helpers/browser-test-harness.js";
import {
	extractAllEventIds,
	extractEventId,
	extractSessionId,
	expectFrameworkContent,
	runInvestigationSequence,
} from "../../../helpers/journey-helpers.js";

const SKIP = !(await isChromeAvailable());
const VUE_SPA = resolve(import.meta.dirname, "../../../fixtures/browser/vue-spa");
```

---

#### Journey 1: Task Management State Observation

**Scenario:** Agent records a user creating tasks, filtering them, and updating statuses. Observes Pinia store mutations and component re-renders.

```typescript
describe.skipIf(SKIP)("Vue Journey: task management state observation", () => {
	let ctx: BrowserTestContext;

	beforeAll(async () => {
		ctx = await setupBrowserTest({
			fixturePath: VUE_SPA,
			frameworkState: ["vue"],
		});
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

	afterAll(async () => { await ctx?.cleanup(); });

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
		// Should see store_mutation events from Pinia
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
```

**Acceptance Criteria:**
- [ ] Vue 3 detected in Vite-bundled app
- [ ] Pinia store mutations generate framework_state events
- [ ] Component filter works for Vue components
- [ ] Diff between markers captures store state changes

---

#### Journey 2: Task Creation Form Validation Bug

**Scenario:** Agent records a user attempting to create a task with invalid data, server rejecting with 422, and investigates the validation failure alongside Vue component state.

```typescript
describe.skipIf(SKIP)("Vue Journey: task creation validation bug", () => {
	// beforeAll: login → navigate to /tasks/new → fill partial form → submit →
	// inject /__test__/fail-create → fill complete form → submit → 422 → marker

	it("Step 1: find session with errors");
	it("Step 2: overview reveals form errors");
	it("Step 3: search for 422 responses on /api/tasks");
	it("Step 4: inspect 422 response body for validation details");
	it("Step 5: search for CreateTask component state during submission");
	it("Step 6: diff form state before and after validation error");
	it("Step 7: generate reproduction steps");
});
```

**Acceptance Criteria:**
- [ ] Task creation form validation errors captured
- [ ] Component state shows form field values at time of submission
- [ ] 422 response body inspectable via session_inspect

---

#### Journey 3: Infinite Watcher Loop Detection

**Scenario:** Agent activates the infinite watcher bug and diagnoses it using Vue-specific framework tools.

```typescript
describe.skipIf(SKIP)("Vue Journey: infinite watcher loop diagnosis", () => {
	// beforeAll: navigate to /bugs/infinite-watcher → activate → wait → marker

	it("Step 1: overview shows high-severity framework errors");
	it("Step 2: search for framework_error with pattern 'watcher_infinite_loop'");
	it("Step 3: identify the InfiniteWatcher component");
	it("Step 4: inspect error event for watcher details");
	it("Step 5: generate reproduction steps");
});
```

**Acceptance Criteria:**
- [ ] Watcher infinite loop detected with high severity
- [ ] Pattern filter works for `watcher_infinite_loop`
- [ ] Component name (InfiniteWatcher) in error events

---

#### Journey 4: Lost Reactivity Detection

**Scenario:** Agent activates the lost reactivity bug and investigates why a component stops updating.

```typescript
describe.skipIf(SKIP)("Vue Journey: lost reactivity diagnosis", () => {
	// beforeAll: navigate to /bugs/lost-reactivity → activate → interact → marker

	it("Step 1: search for framework_error events");
	it("Step 2: filter by 'lost_reactivity' pattern");
	it("Step 3: inspect reveals component with destructured reactive");
	it("Step 4: overview focused on marker shows surrounding evidence");
	it("Step 5: diff before and after activation shows missing updates");
});
```

**Acceptance Criteria:**
- [ ] Lost reactivity pattern detected
- [ ] Pattern filter isolates lost_reactivity events
- [ ] Component detail shows which reactive property lost tracking

---

#### Journey 5: Pinia Store Mutation Outside Action

**Scenario:** Agent detects direct store mutation (bypassing Pinia actions) and investigates.

```typescript
describe.skipIf(SKIP)("Vue Journey: Pinia mutation outside action", () => {
	// beforeAll: navigate to /bugs/pinia-mutation → activate → marker

	it("Step 1: search for framework_error events from Pinia");
	it("Step 2: filter by 'pinia_mutation_outside_action' pattern");
	it("Step 3: inspect shows store name and mutated property");
	it("Step 4: search for store_mutation events around the error");
	it("Step 5: generate Playwright test scaffold");
});
```

**Acceptance Criteria:**
- [ ] Pinia mutation outside action detected
- [ ] Store name and property visible in error detail
- [ ] Framework + pattern compound filter works for Vue

---

#### Journey 6: Multi-Page Navigation with Pinia Persistence

**Scenario:** Agent records a user navigating between task-related pages and verifies store state observation across routes.

```typescript
describe.skipIf(SKIP)("Vue Journey: multi-page Pinia state persistence", () => {
	// beforeAll: login → /tasks → click task → /tasks/:id → add comment →
	// /tasks/new → create task → back to /tasks → verify filter state

	it("Step 1: detect Vue framework");
	it("Step 2: search for mount/unmount across route transitions");
	it("Step 3: search for Pinia store mutations across pages");
	it("Step 4: navigation events tracked for SPA routing");
	it("Step 5: diff between first and last markers shows full session evolution");
	it("Step 6: generate reproduction steps for full workflow");
});
```

**Acceptance Criteria:**
- [ ] Store state persists across Vue Router navigation
- [ ] Component lifecycle (mount/unmount) tracked across routes
- [ ] Full workflow generates meaningful reproduction steps

---

## Implementation Order

1. **Unit 3: Journey test helpers** — No external dependencies, pure utility functions. Needed by all journey tests.
2. **Unit 4: Browser test harness extension** — Small addition to existing harness. Needed before Vite fixtures work.
3. **Unit 1: React SPA fixture app** — Build the React app. Depends on #4 for harness support.
4. **Unit 2: Vue SPA fixture app** — Build the Vue app. Depends on #4 for harness support. Can parallel with #3.
5. **Unit 5: React journey tests** — Depends on #1 and #3.
6. **Unit 6: Vue journey tests** — Depends on #2 and #3.

Units 1 and 2 (React and Vue SPAs) can be implemented in parallel.
Units 5 and 6 (React and Vue journey tests) can be implemented in parallel.

```
[3: Helpers] ─┬─→ [5: React journeys]
              │
[4: Harness]──┤
              │
[1: React SPA]┘
              ├─→ [6: Vue journeys]
[2: Vue SPA]──┘
```

---

## Testing

### Running Journey Tests

```bash
# All journey tests
bun vitest run tests/e2e/browser/journeys/

# React journeys only
bun vitest run tests/e2e/browser/journeys/react-journeys.test.ts

# Vue journeys only
bun vitest run tests/e2e/browser/journeys/vue-journeys.test.ts
```

### Prerequisites

Journey tests require:
- Chrome/Chromium installed (uses `describe.skipIf` via `isChromeAvailable()`)
- Bun runtime
- First run builds Vite fixtures automatically (~10s per fixture)

### Test Timeouts

Each journey's `beforeAll` has a 90-second timeout to accommodate:
- Vite build on first run
- Chrome launch
- Multi-step user interaction recording
- MCP server startup for queries

Individual test steps use the default 30-second vitest timeout.

### Unit Tests for Helpers

**File**: `tests/unit/journey-helpers.test.ts`

```typescript
describe("extractSessionId", () => {
	it("extracts UUID from session_list output");
	it("throws descriptive error when no UUID found");
});

describe("extractEventId", () => {
	it("extracts first event ID by default");
	it("extracts Nth event ID with index parameter");
	it("throws when index out of range");
});

describe("extractAllEventIds", () => {
	it("returns all UUIDs from multi-event output");
	it("returns empty array when no events");
});

describe("expectFrameworkContent", () => {
	it("passes when all expectations met");
	it("throws with structured message on failure");
});

describe("runInvestigationSequence", () => {
	it("returns all intermediate results");
	it("passes custom search filters through");
});
```

---

## Verification Checklist

```bash
# 1. Fixture apps build and serve
cd tests/fixtures/browser/react-spa && bun install && bun run server.ts 0
cd tests/fixtures/browser/vue-spa && bun install && bun run server.ts 0

# 2. Helpers compile
bun run build

# 3. Unit tests for helpers
bun vitest run tests/unit/journey-helpers.test.ts

# 4. Journey tests (requires Chrome)
bun vitest run tests/e2e/browser/journeys/

# 5. Existing tests still pass
bun vitest run tests/e2e/browser/

# 6. Lint
bun run lint
```
