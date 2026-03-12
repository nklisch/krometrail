# Pattern: Test Factory Functions (makeX)

Unit tests define lightweight factory functions named `makeX(overrides?)` that construct minimal valid instances of complex types. Overrides use Partial spread so tests only specify fields relevant to the assertion.

## Rationale
Eliminates repeated construction of deeply nested test objects. Makes tests readable — only the fields that matter to the assertion appear in the test body. Changing the type shape requires updating only the factory.

## Examples

### Example 1: Snapshot and Log Entry Factories
**File**: `tests/unit/core/session-logger.test.ts:5-25`
```typescript
const makeSnapshot = (overrides: Partial<ViewportSnapshot> = {}): ViewportSnapshot => ({
	file: "order.py",
	line: 147,
	function: "process_order",
	reason: "step",
	totalFrames: 1,
	stack: [{ file: "order.py", shortFile: "order.py", line: 147, function: "process_order", arguments: "" }],
	source: [{ line: 147, text: "  x = 1" }],
	locals: [],
	...overrides,
});

const makeEntry = (overrides: Partial<EnrichedActionLogEntry> = {}): EnrichedActionLogEntry => ({
	actionNumber: 1,
	tool: "debug_step",
	summary: "Stepped over",
	timestamp: Date.now(),
	keyParams: {},
	observations: [],
	...overrides,
});
```

### Example 2: DAP Stream and Protocol Helpers
**File**: `tests/unit/core/dap-client.test.ts:7-38`
```typescript
function makeStreams() {
	const toClient = new PassThrough();   // server writes here → client reads
	const fromClient = new PassThrough(); // client writes here → server reads
	return { toClient, fromClient };
}

function writeDAP(stream: PassThrough, message: object): void {
	const json = JSON.stringify(message);
	stream.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);
}

function makeMockResponse(stream: PassThrough, requestSeq: number, command: string, body: object = {}): void {
	writeDAP(stream, { type: "response", seq: requestSeq + 1000, request_seq: requestSeq, success: true, command, body });
}
```

### Example 3: Browser Event and Database Row Factories
**File**: `tests/unit/browser/rolling-buffer.test.ts:5-14`
```typescript
function makeEvent(overrides: Partial<RecordedEvent> = {}): RecordedEvent {
	return {
		id: crypto.randomUUID(),
		tabId: "tab-1",
		timestamp: Date.now(),
		type: "navigation",
		summary: "Navigation event",
		data: {},
		...overrides,
	};
}
```

## When to Use
- Any unit test that constructs a complex type — define a `makeX()` factory at the top of the test file
- Multiple test cases that need the same base object with small variations — use `overrides` parameter

## When NOT to Use
- Integration/E2E tests that launch real processes — use real fixtures instead
- Types with only 1-2 fields — construct inline

## Common Violations
- Building the full object inline in every test case instead of using a factory — makes tests verbose and fragile
- Not accepting an `overrides` parameter — forces creating multiple separate factories for minor variations
- Defining factories in a shared helpers file when they're only used in one test file — keep them local
