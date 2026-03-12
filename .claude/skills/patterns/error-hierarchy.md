# Pattern: Typed Error Hierarchy

All errors extend `AgentLensError` with a typed `code` string and domain-specific context fields. A top-level `getErrorMessage(err: unknown)` utility safely extracts messages from any thrown value.

## Rationale
Provides structured error handling across the codebase: callers can `instanceof`-check for specific errors and access typed context (sessionId, adapterId, limits). The `code` field enables error mapping in RPC responses without string parsing.

## Examples

### Example 1: Base Class and Utility
**File**: `src/core/errors.ts:1-19`
```typescript
export function getErrorMessage(err: unknown): string {
	return err instanceof Error ? err.message : String(err);
}

export class AgentLensError extends Error {
	constructor(
		message: string,
		public readonly code: string,
	) {
		super(message);
		this.name = "AgentLensError";
	}
}
```

### Example 2: Domain Error with Context Fields
**File**: `src/core/errors.ts:85-109`
```typescript
export class SessionLimitError extends AgentLensError {
	constructor(
		public readonly limitName: string,
		public readonly currentValue: number,
		public readonly maxValue: number,
		public readonly suggestion?: string,
	) {
		super(
			`Session limit '${limitName}' exceeded: ${currentValue}/${maxValue}. ${suggestion ?? ""}`,
			"SESSION_LIMIT_EXCEEDED",
		);
		this.name = "SessionLimitError";
	}
}

export class AdapterPrerequisiteError extends AgentLensError {
	constructor(
		public readonly adapterId: string,
		public readonly missing: string[],
		public readonly installHint?: string,
	) {
		super(
			`Adapter '${adapterId}' prerequisites not met: ${missing.join(", ")}. ${installHint ? `Install: ${installHint}` : ""}`,
			"ADAPTER_PREREQUISITES",
		);
		this.name = "AdapterPrerequisiteError";
	}
}
```

### Example 3: Usage in MCP Tool Handler
**File**: `src/mcp/tools/utils.ts:9-11`
```typescript
export function errorResponse(err: unknown): { content: Array<{ type: "text"; text: string }>; isError: true } {
	return { content: [{ type: "text" as const, text: getErrorMessage(err) }], isError: true };
}
```

## When to Use
- Any new error condition in the codebase — add a class to `src/core/errors.ts` with a unique `code`
- When callers need to branch on the error type or access context fields

## When NOT to Use
- Throwing bare `new Error()` for internal logic errors — prefer typed errors
- Re-throwing with a different type when the original type already has the needed context

## Common Violations
- Throwing `new Error("session not found")` instead of `new SessionNotFoundError(sessionId)` — loses structured context
- Forgetting to set `this.name` in the subclass constructor — breaks stack traces and logging
- Using string matching on `err.message` instead of `instanceof AgentLensError` — brittle
