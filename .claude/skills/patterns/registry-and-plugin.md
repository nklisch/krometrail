# Pattern: Registry + Plugin Interface

A central Map-based registry pairs with a shared interface to enable dynamic dispatch to multiple implementations. Both adapters and framework detectors follow the identical structure: `interface` → `register*()` → `get*()`/`detect*()` → `registerAll*()` called once at startup.

## Rationale
Decouples callers (session manager, CLI) from concrete implementations (10 language adapters, N framework detectors). New implementations are added by registering, not by modifying callers.

## Examples

### Example 1: Debug Adapter Registry
**File**: `src/adapters/registry.ts:13-49`
```typescript
const adapters = new Map<string, DebugAdapter>();

export function registerAdapter(adapter: DebugAdapter): void {
	adapters.set(adapter.id, adapter);
	for (const ext of adapter.fileExtensions) adapters.set(ext, adapter);
	for (const alias of adapter.aliases ?? []) adapters.set(alias, adapter);
}

export function getAdapter(idOrExtension: string): DebugAdapter | undefined {
	return adapters.get(idOrExtension);
}

export function registerAllAdapters(): void {
	registerAdapter(new PythonAdapter());
	registerAdapter(new NodeAdapter());
	// ...10 total
}
```

### Example 2: Framework Detector Registry
**File**: `src/frameworks/index.ts:43-93`
```typescript
const detectors: FrameworkDetector[] = [];

export function registerDetector(detector: FrameworkDetector): void {
	detectors.push(detector);
}

export function detectFramework(command: string, adapterId: string, cwd: string, explicitFramework?: string): FrameworkOverrides | null {
	for (const detector of detectors) {
		if (detector.adapterId !== adapterId) continue;
		const result = detector.detect(command, cwd);
		if (result) return result; // first match wins
	}
	return null;
}

export function registerAllDetectors(): void {
	for (const detector of pythonDetectors) registerDetector(detector);
	// ...
}
```

### Example 3: Plugin Interface Definitions
**File**: `src/adapters/base.ts:37-61` and `src/frameworks/index.ts:28-40`
```typescript
// Adapter plugin interface
export interface DebugAdapter {
	id: string;
	fileExtensions: string[];
	aliases?: string[];
	displayName: string;
	checkPrerequisites(): Promise<PrerequisiteResult>;
	launch(config: LaunchConfig): Promise<DAPConnection>;
	attach(config: AttachConfig): Promise<DAPConnection>;
	dispose(): Promise<void>;
}

// Framework plugin interface — same structure
export interface FrameworkDetector {
	id: string;
	displayName: string;
	adapterId: string;
	detect(command: string, cwd: string): FrameworkOverrides | null;
}
```

## When to Use
- Adding a new language debugger: implement `DebugAdapter`, register in `registerAllAdapters()`
- Adding a new framework detector: implement `FrameworkDetector`, register in `registerAllDetectors()`
- Any new "pluggable family" of implementations with a shared interface

## When NOT to Use
- Single implementation that will never vary — just call it directly
- Simple utility functions — no need for a registry

## Common Violations
- Adding a new adapter but forgetting to call `registerAdapter()` in `registerAllAdapters()` — it silently won't be available
- Bypassing the registry by importing an adapter class directly in callers instead of using `getAdapter()`
