# Pattern: Zod Validation at Boundaries

All external inputs — MCP tool parameters, daemon RPC requests, browser DB queries, buffer config — are parsed/validated with Zod schemas at the entry point. Internal code trusts already-validated data and never re-validates.

## Rationale
Validates user and network inputs at system boundaries so internal code receives well-typed data. Zod `.describe()` on schemas doubles as self-documenting MCP tool metadata.

## Examples

### Example 1: MCP Tool Input Schema
**File**: `src/core/types.ts:5-14` and used across `src/mcp/tools/`
```typescript
export const ViewportConfigSchema = z.object({
	source_context_lines: z.number().optional().describe("Lines of source shown above/below current line. Default: 5"),
	stack_depth: z.number().optional().describe("Max call stack frames shown. Default: 10"),
	string_truncate_length: z.number().optional().describe("Max string length before truncation. Default: 120"),
	show_types: z.boolean().optional().describe("Show variable type annotations. Default: false"),
	show_return_value: z.boolean().optional().describe("Show return value on step-out. Default: true"),
	token_budget: z.number().optional().describe("Approximate token budget for viewport output. Default: 8000"),
	diff_mode: z.boolean().optional().describe("Show only changed variables vs. previous stop. Default: false"),
});
```

### Example 2: Breakpoint Schema with Inference
**File**: `src/core/types.ts:47-65`
```typescript
export const BreakpointSchema = z.object({
	line: z.number(),
	condition: z.string().optional(),
	hitCondition: z.string().optional(),
	logMessage: z.string().optional(),
});

export const FileBreakpointsSchema = z.object({
	file: z.string(),
	breakpoints: z.array(BreakpointSchema),
});

export type Breakpoint = z.infer<typeof BreakpointSchema>;
export type FileBreakpoints = z.infer<typeof FileBreakpointsSchema>;
```

### Example 3: Rolling Buffer Config
**File**: `src/browser/recorder/rolling-buffer.ts:1-20`
```typescript
export const BufferConfigSchema = z.object({
	maxEvents: z.number().default(50_000),
	maxAgeMs: z.number().default(30 * 60 * 1000),
	markerPaddingMs: z.number().default(30_000),
});

export type BufferConfig = z.infer<typeof BufferConfigSchema>;

// Usage — parsed once at construction, never re-validated internally:
constructor(config: BufferConfig) {
	this.config = config;
}
```

## When to Use
- Any input that crosses a system boundary: MCP tool params, RPC request params, config objects from users, query filters from external callers
- When schema doubles as documentation (use `.describe()`)

## When NOT to Use
- Internal function calls between modules in the same layer — trust the TypeScript types
- Data that has already been validated at a higher boundary

## Common Violations
- Passing `unknown` through multiple layers before validating — validate immediately at the boundary
- Using `z.any()` to skip validation — defeats the purpose; use `z.unknown()` if type is truly unknown and handle it
- Forgetting to export the inferred type with `z.infer<typeof Schema>` — forces callers to re-declare the type
