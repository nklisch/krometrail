# Pattern: MCP Tool Handler Wrapper

All MCP tool implementations wrap their logic with `toolHandler()` from `src/mcp/tools/utils.ts`. This converts async handler functions into MCP-compliant responses: string result → `textResponse()`, thrown error → `errorResponse()`.

## Rationale
MCP tools must return `{ content: Array<{ type: "text"; text: string }>; isError?: true }`. The wrapper eliminates 3+ lines of try/catch boilerplate from every tool handler and guarantees consistent error formatting.

## Examples

### Example 1: toolHandler Wrapper Definition
**File**: `src/mcp/tools/utils.ts:21-29`
```typescript
export function toolHandler<T>(fn: (params: T) => Promise<string>): (params: T) => Promise<ToolResult> {
	return async (params) => {
		try {
			return textResponse(await fn(params));
		} catch (err) {
			return errorResponse(err);
		}
	};
}
```

### Example 2: Response Helpers
**File**: `src/mcp/tools/utils.ts:9-15`
```typescript
export function errorResponse(err: unknown): { content: Array<{ type: "text"; text: string }>; isError: true } {
	return { content: [{ type: "text" as const, text: getErrorMessage(err) }], isError: true };
}

export function textResponse(text: string): { content: Array<{ type: "text"; text: string }> } {
	return { content: [{ type: "text" as const, text }] };
}
```

### Example 3: Tool Registration Using toolHandler
Used in `src/mcp/tools/` throughout — each tool's handler is wrapped:
```typescript
server.tool(
	"debug_step",
	"Step over current line",
	{ session_id: z.string().optional(), viewport_config: ViewportConfigSchema },
	toolHandler(async (params) => {
		const session = await manager.step(params.session_id);
		return renderViewport(session, params.viewport_config);
	}),
);
```

## When to Use
- Every new MCP tool handler — always wrap with `toolHandler()`
- When the entire handler logic returns a single string — the `toolHandler` signature enforces this

## When NOT to Use
- Handlers that need different error handling per error type — call `textResponse`/`errorResponse` manually in those cases
- Non-MCP code (CLI, daemon) — those have their own response conventions

## Common Violations
- Writing inline try/catch in tool handlers instead of using `toolHandler()` — creates inconsistent error response formats
- Returning raw strings from tool handlers without wrapping — causes MCP protocol errors
- Using `errorResponse()` for non-error content — only use it in catch blocks
