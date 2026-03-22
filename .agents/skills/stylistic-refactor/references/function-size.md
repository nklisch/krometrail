# Style: Function Size

> Functions should generally stay under ~50 lines. Extract when logic can stand alone as a named concept.

## Motivation

Small functions are easier to test, name, and reason about. When a function exceeds 50 lines,
it usually contains multiple logical steps that could be named and tested independently.
The goal is not rigid enforcement but a soft signal to consider extraction.

## Before / After

### From this codebase: well-sized function

**Good** (`src/adapters/python.ts:145-171`, 27 lines):
```typescript
export function parseCommand(command: string): { script: string; args: string[] } {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	if (parts[i] === "python3" || parts[i] === "python") i++;

	if (parts[i] === "-m") return { script: "-m", args: parts.slice(i + 1) };
	if (parts[i] === "-c") return { script: "-c", args: parts.slice(i + 1) };

	const script = parts[i] ?? "";
	const args = parts.slice(i + 1);
	return { script, args };
}
```

### Synthetic example: oversized function

**Before:**
```typescript
function handleRequest(req: Request): Response {
	// 20 lines: validate input
	// 15 lines: fetch data
	// 20 lines: transform data
	// 15 lines: format response
	// Total: 70+ lines
}
```

**After:**
```typescript
function handleRequest(req: Request): Response {
	const input = validateInput(req);
	const data = fetchData(input);
	const transformed = transformData(data);
	return formatResponse(transformed);
}
```

## Exceptions

- Database migrations and schema setup (procedural by nature, splitting adds indirection).
- Complex render functions that build a single output string with many sections — splitting
  into tiny renderers can obscure the overall structure.
- Switch/match statements with many cases — each case is short, the function is just a router.

## Scope

- Applies to: all TypeScript source files
- Does NOT apply to: test functions (test bodies can be longer for readability)
