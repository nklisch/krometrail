# Style: Arrow vs Declaration

> Arrow functions for callbacks and inline expressions; function declarations for named exports and top-level functions.

## Motivation

Function declarations are hoisted and clearly signal "this is a named unit of work" when
scanning a file. Arrow functions are concise for callbacks, `.map()`, `.filter()`, and short
inline logic. Using `const fn = () => ...` for top-level named exports obscures intent and
loses hoisting.

## Before / After

### From this codebase: good separation

**Function declaration for export** (`src/adapters/python.ts:145`):
```typescript
export function parseCommand(command: string): { script: string; args: string[] } {
	const parts = command.trim().split(/\s+/);
	// ...
	return { script, args };
}
```

**Arrow for callback** (`src/core/token-budget.ts:26-40`):
```typescript
export function fitToBudget(sections: RenderSection[], budget: number): RenderSection[] {
	const byPriority = [...sections].sort((a, b) => b.priority - a.priority);
	// ...
	return sections.filter((s) => included.has(s.key));
}
```

### Synthetic example: arrow as top-level export

**Before:**
```typescript
export const formatError = (err: Error): string => {
	return `[${err.name}] ${err.message}`;
};

export const buildUrl = (base: string, path: string): string => {
	return `${base}/${path.replace(/^\//, "")}`;
};
```

**After:**
```typescript
export function formatError(err: Error): string {
	return `[${err.name}] ${err.message}`;
}

export function buildUrl(base: string, path: string): string {
	return `${base}/${path.replace(/^\//, "")}`;
}
```

## Exceptions

- `export const x = defineCommand(...)` or similar builder patterns that return objects are fine
  as `const` — they're not function logic, they're configuration.
- Higher-order functions returning functions may use arrows: `const withAuth = (handler: H) => ...`

## Scope

- Applies to: all TypeScript source files
- Does NOT apply to: inline callbacks, `.map()`, `.filter()`, `.sort()` comparators
