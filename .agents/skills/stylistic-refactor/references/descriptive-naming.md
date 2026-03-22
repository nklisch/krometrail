# Style: Descriptive Naming

> Boolean variables and functions use is/has/should/can prefixes. Prefer descriptive names over abbreviations.

## Motivation

Code is read far more than it's written. Descriptive names eliminate the need to look up
definitions. Boolean prefixes make conditionals read like English: `if (isConnected)` is
immediately clear, while `if (connected)` is ambiguous (is it a connection object? a string?).

## Before / After

### From this codebase: boolean functions

**Good** (`src/core/compression.ts:42`):
```typescript
export function shouldUseDiffMode(tier: CompressionTier, sessionDiffMode?: boolean): boolean {
	return tier.diffMode || sessionDiffMode === true;
}
```

**Good** (`src/browser/investigation/query-engine.ts:25`):
```typescript
export function isTextContentType(contentType: string | undefined | null): boolean {
	if (!contentType) return false;
	const lower = contentType.toLowerCase().split(";")[0].trim();
	return TEXT_CONTENT_TYPE_PATTERNS.some((p) => p.test(lower));
}
```

### Synthetic example: vague naming

**Before:**
```typescript
const valid = schema.safeParse(input);
const open = socket.readyState === WebSocket.OPEN;
function check(path: string): boolean { ... }
function proc(items: Item[]): Result[] { ... }
```

**After:**
```typescript
const isValid = schema.safeParse(input);
const isOpen = socket.readyState === WebSocket.OPEN;
function isAccessible(path: string): boolean { ... }
function processItems(items: Item[]): Result[] { ... }
```

## Exceptions

- Destructured API responses where the upstream name is fixed (e.g., `const { ok } = res`).
- Loop variables and short-lived locals where context is obvious (e.g., `for (const item of items)`).
- Well-known abbreviations in the domain: `cwd`, `pid`, `url`, `dap`, `cdp`.

## Scope

- Applies to: all TypeScript source files — variable names, function names, method names
- Does NOT apply to: external API types, third-party interface implementations
