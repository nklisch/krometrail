# Style: Composition over Inheritance

> Flat interface implementations; no deep class hierarchies. Share behavior via helper modules.

## Motivation

Deep inheritance chains create tight coupling and make it hard to understand what a class
actually does without tracing up the chain. This project uses a flat model: adapters implement
the `DebugAdapter` interface directly, and shared behavior lives in `helpers.ts` as importable
functions. This keeps each adapter self-contained and testable.

## Before / After

### From this codebase: flat interface implementation

**Good** (`src/adapters/python.ts` and `src/adapters/node.ts`):
```typescript
// Each adapter implements the interface directly — no base class
export class PythonAdapter implements DebugAdapter {
	id = "python";
	fileExtensions = [".py"];
	displayName = "Python (debugpy)";

	async checkPrerequisites(): Promise<PrerequisiteResult> {
		return checkCommand("python3", ["--version"]); // shared helper
	}

	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const port = await allocatePort(); // shared helper
		// ... adapter-specific logic
	}
}
```

### Synthetic example: unnecessary inheritance

**Before:**
```typescript
abstract class BaseAdapter {
	abstract get id(): string;
	async checkPrerequisites(): Promise<Result> { /* default impl */ }
	async launch(config: Config): Promise<Connection> { /* default impl with hooks */ }
	protected abstract onLaunch(config: Config): Promise<void>;
}

class PythonAdapter extends BaseAdapter {
	get id() { return "python"; }
	protected async onLaunch(config: Config) { /* override hook */ }
}
```

**After:**
```typescript
class PythonAdapter implements DebugAdapter {
	id = "python";
	async checkPrerequisites(): Promise<Result> {
		return checkCommand("python3", ["--version"]);
	}
	async launch(config: Config): Promise<Connection> {
		// Full implementation — no hidden base class behavior
	}
}
```

## Exceptions

- Error classes extending `KrometrailError` (single level of inheritance for typed error context).
- SDK-mandated base classes (when a framework requires `extends BaseClass`).
- Max 1 level of `extends` — if you need more, refactor to composition.

## Scope

- Applies to: all TypeScript source files, especially adapters and coordinators
- Does NOT apply to: error hierarchy (codified pattern), framework-required base classes
