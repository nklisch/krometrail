# Style: Classes vs Functions

> Classes for stateful coordinators; plain functions for pure transformations and utilities.

## Motivation

Classes are the right tool when you need encapsulated mutable state with a lifecycle (connect,
use, dispose). Plain functions are simpler, more testable, and composable for stateless logic.
Mixing these up adds unnecessary complexity — a class with no instance state is just a namespace,
and a pure function that needs `this` is fighting the paradigm.

## Before / After

### From this codebase: stateful coordinator

**Good class** (`src/browser/investigation/diff.ts`):
```typescript
export class SessionDiffer {
	constructor(private queryEngine: QueryEngine) {}

	diff(params: DiffParams): DiffResult {
		const beforeTs = resolveTimestamp(this.queryEngine, params.sessionId, params.before);
		const afterTs = resolveTimestamp(this.queryEngine, params.sessionId, params.after);
		// ... coordinates queries via instance's queryEngine
		return result;
	}
}
```

**Good function** (`src/core/token-budget.ts`):
```typescript
export function estimateTokens(text: string): number {
	return Math.ceil(text.length / 4);
}
```

### Synthetic example: unnecessary class

**Before:**
```typescript
class ConfigValidator {
	validate(config: AppConfig): ValidationResult {
		const errors: string[] = [];
		if (!config.port) errors.push("port required");
		if (!config.host) errors.push("host required");
		return { valid: errors.length === 0, errors };
	}
}
```

**After:**
```typescript
function validateConfig(config: AppConfig): ValidationResult {
	const errors: string[] = [];
	if (!config.port) errors.push("port required");
	if (!config.host) errors.push("host required");
	return { valid: errors.length === 0, errors };
}
```

## Exceptions

- Framework-mandated classes (e.g., extending SDK base classes) are fine.
- A class with only static methods should be converted to a module of functions.

## Scope

- Applies to: all TypeScript source files
- Does NOT apply to: error classes (extending `KrometrailError` is the codified pattern)
