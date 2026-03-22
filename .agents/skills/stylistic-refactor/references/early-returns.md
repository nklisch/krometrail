# Style: Early Returns

> Prefer early returns and guard clauses over nested if/else blocks.

## Motivation

Linear control flow is easier to read and reason about. Guard clauses at the top of a function
eliminate invalid states early, so the remaining logic runs at a single indentation level.
This is especially important in a codebase with many branching paths (adapter selection,
framework detection, error handling).

## Before / After

### From this codebase: framework detection

**Before:** (already good — `src/frameworks/index.ts:64-82`)
```typescript
export function detectFramework(command: string, adapterId: string, cwd: string, explicitFramework?: string): FrameworkOverrides | null {
	if (explicitFramework === "none") return null;

	if (explicitFramework) {
		const detector = detectors.find((d) => d.id === explicitFramework);
		if (!detector) return null;
		return detector.detect(command, cwd);
	}

	for (const detector of detectors) {
		if (detector.adapterId !== adapterId) continue;
		const result = detector.detect(command, cwd);
		if (result) return result;
	}

	return null;
}
```

### Synthetic example: processing a response

**Before:**
```typescript
function processResponse(res: Response) {
	if (res.ok) {
		const data = res.json();
		if (data.items) {
			if (data.items.length > 0) {
				return data.items.map(transform);
			} else {
				return [];
			}
		} else {
			return [];
		}
	} else {
		throw new Error(`HTTP ${res.status}`);
	}
}
```

**After:**
```typescript
function processResponse(res: Response) {
	if (!res.ok) throw new Error(`HTTP ${res.status}`);

	const data = res.json();
	if (!data.items?.length) return [];

	return data.items.map(transform);
}
```

## Exceptions

- When both branches of an if/else have equal weight and neither is a "guard" (e.g., a true
  binary decision), an if/else block is fine.
- Ternaries are acceptable for simple value selection: `const x = condition ? a : b`.

## Scope

- Applies to: all TypeScript source files
- Does NOT apply to: test assertions (expect chains naturally nest)
