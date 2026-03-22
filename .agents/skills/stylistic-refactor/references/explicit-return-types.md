# Style: Explicit Return Types

> Exported/public functions must have explicit return type annotations.

## Motivation

Explicit return types on exports serve as documentation and catch accidental type changes.
When a function's return type is inferred, a small internal change can silently widen the type,
breaking downstream consumers without any error at the definition site. Internal/private
functions can rely on inference to reduce boilerplate.

## Before / After

### From this codebase: missing return type

**Before:** (`src/core/session-logger.ts:60`)
```typescript
export function formatSessionLogSummary(
	entries: EnrichedActionLogEntry[],
	compressionWindowSize: number,
	sessionElapsedMs: number,
	tokenStats: { viewportTokensConsumed: number; viewportCount: number },
) {
	// returns string but not annotated
```

**After:**
```typescript
export function formatSessionLogSummary(
	entries: EnrichedActionLogEntry[],
	compressionWindowSize: number,
	sessionElapsedMs: number,
	tokenStats: { viewportTokensConsumed: number; viewportCount: number },
): string {
```

### Synthetic example: inferred type leaks implementation detail

**Before:**
```typescript
export function loadConfig(path: string) {
	const raw = readFileSync(path, "utf-8");
	return { ...JSON.parse(raw), _loadedFrom: path }; // return type includes _loadedFrom
}
```

**After:**
```typescript
export function loadConfig(path: string): AppConfig {
	const raw = readFileSync(path, "utf-8");
	return { ...JSON.parse(raw), _loadedFrom: path };
}
```

## Exceptions

- Internal (non-exported) functions: inference is fine, reduces noise.
- One-liner utility functions where the return type is obvious from the body (e.g., `(): boolean`).
- Functions whose return type is complex and hard to name — consider creating a named type first.

## Scope

- Applies to: all `export function` and `export async function` declarations
- Does NOT apply to: private methods, inline callbacks, non-exported helpers
