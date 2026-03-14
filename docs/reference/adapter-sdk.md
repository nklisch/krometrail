---
title: Adapter SDK
description: How to build a new language adapter — the DebugAdapter interface, lifecycle, and testing requirements.
---

# Adapter SDK

Adding a new language to Krometrail means implementing the `DebugAdapter` interface. The adapter's only job is to launch the debugger and return a DAP connection — all protocol communication is handled by the core.

## The DebugAdapter Interface

```typescript
// src/adapters/base.ts

export interface DebugAdapter {
	id: string;                    // "python", "node", "go", etc.
	fileExtensions: string[];      // [".py"] or [".js", ".ts"]
	displayName: string;           // shown in `krometrail doctor`

	checkPrerequisites(): Promise<PrerequisiteResult>;
	launch(config: LaunchConfig): Promise<DAPConnection>;
	attach(config: AttachConfig): Promise<DAPConnection>;
	dispose(): Promise<void>;
}

export interface PrerequisiteResult {
	satisfied: boolean;
	missing?: string[];      // e.g., ["debugpy", "python3"]
	installHint?: string;    // shown in `krometrail doctor`
}

export interface DAPConnection {
	reader: Readable;        // reads DAP messages from the debugger
	writer: Writable;        // writes DAP messages to the debugger
	process?: ChildProcess;  // the debugger subprocess (for cleanup)
	launchArgs?: Record<string, unknown>;  // DAP launch request fields
}

export interface LaunchConfig {
	command: string;     // the user's command, e.g., "python3 app.py"
	cwd?: string;
	env?: Record<string, string>;
	port?: number;       // pre-allocated port (if you called allocatePort())
}
```

## Shared Helpers

Use helpers from `src/adapters/helpers.ts` — do not duplicate these inline:

```typescript
import { allocatePort, connectTCP, gracefulDispose, spawnAndWait } from "./helpers.js";

// Allocate a free TCP port
const port = await allocatePort();

// Spawn and wait for readiness signal on stderr
const proc = await spawnAndWait(
	"my-debugger",
	["--port", String(port), script],
	{ cwd, env: { ...process.env, ...config.env } },
	/Listening on port \d+/,  // regex matching readiness
	5000                       // timeout ms
);

// Connect TCP to the debugger
const socket = await connectTCP(port, "127.0.0.1");

// Graceful cleanup
await gracefulDispose(socket, process);
```

## Step-by-Step Guide

### 1. Create the adapter file

```
src/adapters/<language>.ts
```

### 2. Implement `checkPrerequisites`

Spawn the debugger's version command and check the exit code:

```typescript
async checkPrerequisites(): Promise<PrerequisiteResult> {
	return new Promise((resolve) => {
		const proc = spawn("my-debugger", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => {
			if (code === 0) {
				resolve({ satisfied: true });
			} else {
				resolve({
					satisfied: false,
					missing: ["my-debugger"],
					installHint: "Install with: brew install my-debugger",
				});
			}
		});
		proc.on("error", () => resolve({
			satisfied: false,
			missing: ["my-debugger"],
			installHint: "Install with: brew install my-debugger",
		}));
	});
}
```

### 3. Implement `launch` — TCP transport

Most debuggers listen on a TCP port:

```typescript
async launch(config: LaunchConfig): Promise<DAPConnection> {
	const port = await allocatePort();
	const parts = config.command.trim().split(/\s+/);
	const script = parts[1] ?? "";

	this.process = await spawnAndWait(
		"my-debugger",
		["--port", String(port), script],
		{ cwd: config.cwd ?? process.cwd(), env: { ...process.env, ...config.env } },
		/Listening on port \d+/,
		5000
	);

	this.socket = await connectTCP(port);

	return {
		reader: this.socket,
		writer: this.socket,
		process: this.process,
		launchArgs: { program: script, cwd: config.cwd },
	};
}
```

### 4. Implement `launch` — stdin/stdout transport

For debuggers like GDB that communicate via pipes:

```typescript
async launch(config: LaunchConfig): Promise<DAPConnection> {
	const child = spawn("my-debugger", ["--interpreter=dap"], {
		cwd: config.cwd ?? process.cwd(),
		env: { ...process.env, ...config.env },
		stdio: ["pipe", "pipe", "pipe"],
	});
	this.process = child;
	return {
		reader: child.stdout!,
		writer: child.stdin!,
		process: child,
		launchArgs: { program: "/path/to/binary" },
	};
}
```

### 5. Implement `dispose`

```typescript
async dispose(): Promise<void> {
	await gracefulDispose(this.socket, this.process);
	this.socket = null;
	this.process = null;
}
```

### 6. Register the adapter

Add to `src/adapters/registry.ts`:

```typescript
import { MyLanguageAdapter } from "./mylang.js";

export function registerAllAdapters(): void {
	// ... existing adapters ...
	registerAdapter(new MyLanguageAdapter());
}
```

No changes to the MCP server, CLI, or core are required.

### 7. Write conformance tests

Create `tests/integration/adapters/conformance-mylang.test.ts` using the shared conformance harness:

```typescript
import { describe } from "vitest";
import { MyLanguageAdapter } from "../../../src/adapters/mylang.js";
import { createSessionManager } from "../../../src/core/session-manager.js";
import { runConformanceSuite } from "../../harness/adapter-conformance.js";

const SKIP = !(await new MyLanguageAdapter().checkPrerequisites()).satisfied;

describe.skipIf(SKIP)("My Language adapter conformance", () => {
	runConformanceSuite(new MyLanguageAdapter(), {
		filePath: resolve(import.meta.dirname, "../../fixtures/mylang/conformance.ml"),
		command: "my-interpreter conformance.ml",
		language: "mylang",
		loopBodyLine: 10,
		functionCallLine: 11,
		insideFunctionLine: 3,
		expectedLocals: ["items", "total"],
		evalExpression: "len(items)",
		evalExpectedSubstring: "3",
	}, createSessionManager);
});
```

The conformance suite verifies: launch, breakpoints, step over/into/out, evaluate, variables, conditional breakpoints, and dispose.

## Reference Adapters

| Adapter | File | Transport | Key pattern |
|---------|------|-----------|-------------|
| Python | `src/adapters/python.ts` | TCP | debugpy with `launch-first` DAP flow |
| Node.js | `src/adapters/node.ts` | TCP | js-debug download + cache |
| Go | `src/adapters/go.ts` | TCP | dlv test detection, goroutine support |
| C/C++ | `src/adapters/cpp.ts` | stdin/stdout | GDB `--interpreter=dap`, auto-compile |

Start with the Python adapter as the simplest reference.
