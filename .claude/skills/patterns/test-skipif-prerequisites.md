# Pattern: describe.skipIf for Optional Test Prerequisites

Integration and E2E tests that require external tooling (debuggers, Chrome, compilers) compute a `SKIP_NO_X` boolean at module load time and pass it to `describe.skipIf()`. This skips the entire suite gracefully when the dependency is absent.

## Rationale
Not all CI environments have all debuggers installed. Tests should skip cleanly rather than fail with confusing errors. Computing at module load time (top-level `await`) means the check runs once, not per-test.

## Examples

### Example 1: Node Debug Availability Check
**File**: `tests/helpers/node-check.ts:6-30`
```typescript
export async function isNodeDebugAvailable(): Promise<boolean> {
	return new Promise((resolve) => {
		const proc = spawn("node", ["--version"], { stdio: "pipe" });
		let output = "";
		proc.stdout?.on("data", (d: Buffer) => { output += d.toString(); });
		proc.on("close", (code) => {
			if (code !== 0) { resolve(false); return; }
			const match = output.trim().match(/^v(\d+)/);
			resolve(match ? parseInt(match[1], 10) >= 18 : false);
		});
		proc.on("error", () => resolve(false));
	});
}

// Computed once at module load time for describe.skipIf:
export const SKIP_NO_NODE_DEBUG: boolean = await isNodeDebugAvailable().then((ok) => !ok);
```

### Example 2: Using Skip Flag in Integration Tests
**File**: `tests/integration/adapters/node.test.ts:1-17`
```typescript
import { SKIP_NO_NODE_DEBUG } from "../../helpers/node-check.js";

describe.skipIf(SKIP_NO_NODE_DEBUG)("NodeAdapter integration", () => {
	it("launches and hits a breakpoint", async () => {
		// test body
	});
});
```

### Example 3: Multiple Skip Flags in E2E Tests
**File**: `tests/e2e/mcp/discount-bug.test.ts:1-15`
```typescript
import { SKIP_NO_NODE_DEBUG } from "../../helpers/node-check.js";

describe.skipIf(SKIP_NO_NODE_DEBUG)("E2E: discount bug scenario", () => {
	// ...
});
```

Corresponding check files exist for all supported debuggers:
- `tests/helpers/debugpy-check.ts` → `SKIP_NO_DEBUGPY`
- `tests/helpers/dlv-check.ts` → `SKIP_NO_DLV`
- `tests/helpers/cargo-check.ts` → `SKIP_NO_CARGO`
- `tests/helpers/chrome-check.ts` → `SKIP_NO_CHROME`
- etc.

## When to Use
- Any integration or E2E test that requires an external binary or service
- When the dependency may legitimately be absent in some environments

## When NOT to Use
- Unit tests — unit tests should not depend on external tools
- When the test should hard-fail if the dependency is missing (e.g., required CI dependency) — use a `beforeAll` that throws instead

## Common Violations
- Using `it.skip()` or `test.skip()` unconditionally — always skips even when the tool is available
- Calling the availability check inside `beforeEach` or `it` — re-runs the spawn on every test case
- Not exporting the `SKIP_NO_X` constant — forces each test file to duplicate the check logic
