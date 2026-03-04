# Design: Phase 4 — Multi-Language

## Overview

Phase 4 proves the adapter contract works across languages by implementing Node.js and Go adapters, then validating that every MCP tool and CLI command produces consistent behavior regardless of the underlying debugger. This phase adds no new core features — it extends coverage horizontally.

Key deliverables:
1. **Node.js adapter** — `node --inspect-brk` with TCP DAP connection
2. **Go adapter** — Delve (`dlv dap`) with TCP DAP connection
3. **Language-aware value renderer** — extend `isInternalVariable()` and rendering heuristics for JS/Go types
4. **Prerequisite check helpers** — per-language availability checks analogous to `isDebugpyAvailable()`
5. **Cross-adapter test matrix** — shared test scenarios run against Python, Node.js, and Go
6. **Adapter registration in entry points** — register new adapters in MCP server, daemon, CLI doctor

Files created or modified:
- `src/adapters/node.ts` — new
- `src/adapters/go.ts` — new
- `src/adapters/helpers.ts` — new (shared `allocatePort` + `spawnAndWait` extracted from `python.ts`)
- `src/adapters/python.ts` — refactor to use shared helpers
- `src/adapters/registry.ts` — no changes (contract already supports multi-adapter)
- `src/core/value-renderer.ts` — add JS/Go internal name sets, type-aware rendering
- `src/mcp/index.ts` — register Node + Go adapters
- `src/daemon/entry.ts` — register Node + Go adapters
- `src/cli/commands/doctor.ts` — add version detection for Node inspector + Delve
- `tests/helpers/node-check.ts` — new
- `tests/helpers/dlv-check.ts` — new
- `tests/fixtures/node/simple-loop.js` — new
- `tests/fixtures/node/function-calls.js` — new
- `tests/fixtures/node/async-await.js` — new
- `tests/fixtures/go/simple-loop.go` — new
- `tests/fixtures/go/function-calls.go` — new
- `tests/integration/adapters/node.test.ts` — new
- `tests/integration/adapters/go.test.ts` — new
- `tests/e2e/mcp/cross-adapter.test.ts` — new

**No existing tests or interfaces are broken.** All changes are additive. The `DebugAdapter` interface, `DAPConnection`, `LaunchConfig`, and `AttachConfig` types are unchanged. The viewport format is unchanged. The value renderer changes are additive (new type patterns, new internal name sets).

---

## Implementation Units

### Unit 1: Shared Adapter Helpers

**File**: `src/adapters/helpers.ts`

Extract `allocatePort()` from `python.ts` into a shared module. Add a `spawnAndWait` helper that standardizes the pattern of spawning a debugger process and waiting for a readiness signal on stderr.

```typescript
import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { LaunchError } from "../core/errors.js";

/**
 * Allocate a free TCP port by binding to port 0, reading the
 * assigned port, and immediately closing the server.
 */
export function allocatePort(): Promise<number>;

export interface SpawnAndWaitOptions {
	/** Command to spawn */
	cmd: string;
	/** Arguments */
	args: string[];
	/** Working directory */
	cwd?: string;
	/** Environment variables */
	env?: Record<string, string>;
	/** Regex pattern to match on stderr indicating readiness */
	readyPattern: RegExp;
	/** Timeout in ms for the process to become ready */
	timeoutMs?: number;
	/** Label for error messages (e.g., "debugpy", "dlv") */
	label: string;
}

export interface SpawnResult {
	process: ChildProcess;
	/** Stderr output accumulated before readiness */
	stderrBuffer: string;
}

/**
 * Spawn a debugger process and wait for a readiness pattern on stderr.
 * Rejects with LaunchError on timeout, non-zero exit, or spawn failure.
 */
export function spawnAndWait(options: SpawnAndWaitOptions): Promise<SpawnResult>;

/**
 * Connect a TCP socket to host:port with retry logic.
 * Retries up to `maxRetries` times with `retryDelayMs` between attempts.
 * Returns the connected Socket.
 */
export function connectTCP(host: string, port: number, maxRetries?: number, retryDelayMs?: number): Promise<import("node:net").Socket>;
```

**Implementation Notes**:
- `allocatePort()` is moved verbatim from `python.ts`
- `spawnAndWait()` consolidates the pattern in `PythonAdapter.launch()`: spawn process, buffer stderr, wait for readiness pattern, handle timeout/error/early-exit. Used by all three adapters.
- `connectTCP()` wraps `net.createConnection()` with retry logic. Delve sometimes takes a moment to accept connections after printing the readiness message. Default: 3 retries, 200ms delay.
- `python.ts` is updated to import from `helpers.ts` instead of defining `allocatePort()` locally

**Acceptance Criteria**:
- [ ] `allocatePort()` returns valid ports (existing tests pass)
- [ ] `spawnAndWait()` resolves when readiness pattern is matched
- [ ] `spawnAndWait()` rejects with `LaunchError` on timeout
- [ ] `spawnAndWait()` rejects with `LaunchError` on non-zero exit
- [ ] `connectTCP()` connects on first attempt when port is open
- [ ] `connectTCP()` retries and eventually connects after brief delay
- [ ] `PythonAdapter` still passes all existing integration tests after refactor

---

### Unit 2: Node.js Adapter

**File**: `src/adapters/node.ts`

Implement `NodeAdapter` using Node's built-in `--inspect-brk` flag for DAP debugging via TCP.

```typescript
import type { ChildProcess } from "node:child_process";
import type { Socket } from "node:net";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";

export class NodeAdapter implements DebugAdapter {
	id = "node";
	fileExtensions = [".js", ".mjs", ".cjs"];
	displayName = "Node.js (inspector)";

	private process: ChildProcess | null;
	private socket: Socket | null;

	checkPrerequisites(): Promise<PrerequisiteResult>;
	launch(config: LaunchConfig): Promise<DAPConnection>;
	attach(config: AttachConfig): Promise<DAPConnection>;
	dispose(): Promise<void>;
}

/**
 * Parse a Node.js command string, stripping "node" prefix if present.
 * E.g., "node app.js --verbose" => { script: "app.js", args: ["--verbose"] }
 * Handles "node" and "node --" prefixes.
 */
export function parseNodeCommand(command: string): { script: string; args: string[] };
```

**Implementation Notes**:

- **`checkPrerequisites()`** — Spawn `node --version`, parse output. Require Node 18+ (stable inspector protocol). Return `{ satisfied: false, missing: ["node"], installHint: "Install Node.js 18+ from https://nodejs.org" }` if missing or version too old.

- **`launch(config)`**:
  1. Allocate port via `allocatePort()` if `config.port` is not set
  2. Parse command with `parseNodeCommand(config.command)`
  3. Spawn: `node --inspect-brk=127.0.0.1:{port} {script} {args}`
  4. Wait for readiness via `spawnAndWait()` with pattern `/Debugger listening on ws:\/\//i`
  5. Node's inspector uses a WebSocket-based protocol, but since DAP over the inspector requires a DAP adapter, we use the Chrome DevTools Protocol (CDP) → DAP bridge approach. **However**, Node.js does NOT natively speak DAP. The `--inspect-brk` flag exposes the V8 Inspector Protocol (CDP), not DAP.

  **Revised approach**: Use `@vscode/js-debug` or a simpler approach:
  - Node 20.11+ supports `--inspect-brk` with the `--experimental-vm-modules` flag and can work with DAP adapters.
  - The practical approach: spawn the `js-debug-adapter` DAP server (from `@vscode/js-debug`) which internally manages Node's inspector. This is what mcp-debugger does.
  - **Simpler alternative**: Use `node --inspect-brk={port}` and implement a minimal CDP-to-DAP translation layer.

  **Decision**: Use the VS Code `js-debug` adapter (`@vscode/js-debug`). Download the DAP adapter binary on first use (similar to mcp-debugger's approach). The adapter binary is a self-contained Node script that speaks DAP over stdin/stdout.

  **Revised launch sequence**:
  1. Ensure the `js-debug` DAP adapter is available (download if needed, cache in `~/.agent-lens/adapters/`)
  2. Allocate port for the DAP adapter
  3. Spawn the DAP adapter: `node {dapAdapterPath} {dapPort}`
  4. Connect TCP to the DAP adapter port
  5. Send DAP `initialize` → `launch` with `{ program: script, args, cwd, stopOnEntry: true, type: "pwa-node" }`
  6. The DAP adapter internally spawns Node with `--inspect-brk` and manages the CDP connection

  **Alternative simpler approach (preferred for Phase 4)**: Skip the js-debug adapter complexity. Instead, use `node --inspect-brk` and implement a thin CDP-to-DAP shim. But this is very complex.

  **Final decision**: Use the simplest viable approach. Node.js has a native DAP integration when using the VS Code js-debug adapter. We'll spawn `js-debug-adapter` as a DAP server that communicates over stdin/stdout (not TCP), similar to how VS Code uses it. This gives us full DAP compliance without building a CDP shim.

  **Simplified launch sequence**:
  1. Allocate a free port
  2. Spawn: `node --inspect-brk=127.0.0.1:{port} {script} {args}`
  3. Wait for the "Debugger listening" message on stderr
  4. Start the `js-debug` DAP adapter as a child process with stdin/stdout streams
  5. Send DAP `attach` request to the adapter with `{ type: "pwa-node", port: {port} }`
  6. Return `DAPConnection` with the adapter's stdin/stdout as writer/reader

  Actually, let's take an even simpler approach that avoids the js-debug dependency entirely for Phase 4:

  **Final approach — direct CDP via `node --inspect-brk`**:

  Since Node's `--inspect-brk` does NOT speak DAP, and adding a CDP-to-DAP translation layer or vendoring `js-debug` both add significant complexity, the cleanest Phase 4 approach is:

  1. Allocate a free port for the DAP adapter
  2. Use the `@anthropic-ai/agent-lens-js-debug-adapter` package (or bundle the js-debug adapter) — **No, this doesn't exist.**

  Let me re-examine mcp-debugger's approach:
  - mcp-debugger downloads `vscode-js-debug` during `npm install` via a postinstall script
  - The downloaded package contains `js-debug-dap-node.js` which is a DAP server

  **Final decision for Phase 4**: Follow mcp-debugger's proven approach.
  1. Download `js-debug` DAP adapter on first use (or via `agent-lens doctor --install`)
  2. Spawn the adapter: `node {path-to-js-debug-dap-node.js} --port={port}`
  3. Connect TCP to the DAP port
  4. Send DAP `launch` with `{ program, args, cwd }`
  5. The adapter handles all CDP/inspector complexity internally

  The adapter download/cache logic lives in a new helper in `src/adapters/js-debug-adapter.ts`.

- **`attach(config)`**: Connect to the js-debug DAP adapter, send DAP `attach` request with port/pid.

- **`dispose()`**: Kill the js-debug adapter process, then kill the Node debugee if it's still running. Same pattern as `PythonAdapter.dispose()`.

- **`parseNodeCommand(command)`**: Strip "node", "node --", handle remaining as script + args. Don't strip node flags that aren't `--inspect*`.

**Acceptance Criteria**:
- [ ] `checkPrerequisites()` returns satisfied when Node 18+ is installed
- [ ] `checkPrerequisites()` returns unsatisfied with install hint when Node is missing or < 18
- [ ] `launch()` spawns Node process and returns working DAPConnection
- [ ] DAP messages can be sent/received through the connection
- [ ] `dispose()` kills both the adapter and debugee processes
- [ ] `parseNodeCommand("node app.js --verbose")` returns `{ script: "app.js", args: ["--verbose"] }`
- [ ] `parseNodeCommand("app.js")` returns `{ script: "app.js", args: [] }`

---

### Unit 3: js-debug DAP Adapter Manager

**File**: `src/adapters/js-debug-adapter.ts`

Manage downloading, caching, and locating the VS Code js-debug DAP adapter.

```typescript
/**
 * Get the path to the js-debug DAP adapter entry point.
 * Downloads the adapter if not already cached.
 * Cache location: ~/.agent-lens/adapters/js-debug/
 */
export async function getJsDebugAdapterPath(): Promise<string>;

/**
 * Check if the js-debug adapter is available in the cache.
 */
export function isJsDebugAdapterCached(): boolean;

/**
 * Download and extract the js-debug DAP adapter.
 * Fetches the latest vsix from the VS Code marketplace or GitHub releases.
 */
export async function downloadJsDebugAdapter(): Promise<void>;

/**
 * Path to the adapter cache directory.
 */
export function getAdapterCacheDir(): string;
```

**Implementation Notes**:
- Cache directory: `~/.agent-lens/adapters/js-debug/`
- Download source: VS Code marketplace VSIX for `ms-vscode.js-debug` (it's a zip containing the DAP adapter JS files)
- The key file is `js-debug/src/dapDebugServer.js` — a self-contained DAP server that can be spawned with `node dapDebugServer.js {port}`
- Version pinning: store a version file in the cache dir. If the version matches, skip download.
- The download is triggered lazily on first `NodeAdapter.launch()` call, or eagerly via `agent-lens doctor --install`
- VSIX download URL: `https://marketplace.visualstudio.com/_apis/public/gallery/publishers/ms-vscode/vsextensions/js-debug/{version}/vspackage`
- Extract using Node's built-in `zlib` + tar handling (VSIX is a zip)

**Acceptance Criteria**:
- [ ] `getJsDebugAdapterPath()` returns a valid path to `dapDebugServer.js`
- [ ] First call downloads and caches the adapter
- [ ] Subsequent calls return the cached path without downloading
- [ ] `isJsDebugAdapterCached()` reflects presence/absence of the cached adapter
- [ ] Download failure produces a clear error with install hint

---

### Unit 4: Go Adapter

**File**: `src/adapters/go.ts`

Implement `GoAdapter` using Delve (`dlv dap`) as the DAP server.

```typescript
import type { ChildProcess } from "node:child_process";
import type { Socket } from "node:net";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";

export class GoAdapter implements DebugAdapter {
	id = "go";
	fileExtensions = [".go"];
	displayName = "Go (Delve)";

	private dlvProcess: ChildProcess | null;
	private socket: Socket | null;

	checkPrerequisites(): Promise<PrerequisiteResult>;
	launch(config: LaunchConfig): Promise<DAPConnection>;
	attach(config: AttachConfig): Promise<DAPConnection>;
	dispose(): Promise<void>;
}

/**
 * Parse a Go command string.
 * E.g., "go run main.go" => { mode: "debug", program: "main.go", args: [] }
 *       "./mybinary --flag" => { mode: "exec", program: "./mybinary", args: ["--flag"] }
 *       "go test ./..." => { mode: "test", program: "./...", args: [] }
 */
export function parseGoCommand(command: string): {
	mode: "debug" | "exec" | "test";
	program: string;
	buildFlags?: string[];
	args: string[];
};
```

**Implementation Notes**:

- **`checkPrerequisites()`** — Spawn `dlv version`, parse output. Return `{ satisfied: false, missing: ["dlv"], installHint: "go install github.com/go-delve/delve/cmd/dlv@latest" }` if missing.

- **`launch(config)`**:
  1. Allocate port via `allocatePort()` if `config.port` is not set
  2. Parse command with `parseGoCommand(config.command)`
  3. Spawn Delve as a DAP server: `dlv dap --listen 127.0.0.1:{port}`
  4. Wait for readiness via `spawnAndWait()` with pattern `/DAP server listening at/i`
  5. Connect TCP to Delve's DAP port via `connectTCP()` (with retries — Delve can be slow to accept)
  6. Return `DAPConnection` with the TCP socket as reader/writer and the `dlv` process

  **DAP launch request**: After connecting, the core's `DAPClient.initialize()` then `DAPClient.launch()` sends the DAP `launch` request. For Delve, the launch request arguments are:
  ```json
  {
    "mode": "debug",
    "program": "/path/to/main.go",
    "args": [],
    "cwd": "/path/to/project",
    "stopOnEntry": false
  }
  ```
  For `go test`: `mode: "test"`, `program: "./..."`.
  For pre-compiled binaries: `mode: "exec"`, `program: "./mybinary"`.

  **Important**: Delve in DAP mode expects the `launch` request to contain the program info. The `dlv dap` process itself doesn't launch the debugee — it waits for the DAP client to tell it what to debug via the `launch` request. This is different from debugpy which launches the script directly.

  The session manager's `launch()` method currently constructs a DAP `launch` request with `noDebug: false` (line ~120 in session-manager.ts). For Delve, it needs to pass `mode`, `program`, `args`, and `cwd`. These must be extracted from the `LaunchConfig` command and passed through.

  **Adapter-specific DAP launch args**: Add an optional `dapLaunchArgs` method to the adapter interface (or handle this in the adapter's `launch()` return value). The cleanest approach: add an optional `launchArgs` field to `DAPConnection` that the session manager passes to `DAPClient.launch()`.

- **`attach(config)`**: Spawn `dlv dap --listen 127.0.0.1:{port}`, connect, then issue DAP `attach` request with `{ mode: "local", processId: config.pid }` or `{ mode: "remote", remotePath: "", substitutePath: [] }`.

- **`dispose()`**: Send DAP `disconnect` with `terminateDebuggee: true`, then kill the `dlv` process. Same SIGTERM → SIGKILL pattern as `PythonAdapter.dispose()`.

- **`parseGoCommand(command)`**:
  - `"go run main.go --flag"` → `{ mode: "debug", program: "main.go", args: ["--flag"] }`
  - `"go test ./pkg/..."` → `{ mode: "test", program: "./pkg/...", args: [] }`
  - `"./mybinary --flag"` → `{ mode: "exec", program: "./mybinary", args: ["--flag"] }`
  - Strip `go run` prefix, detect `go test`, handle bare binary paths

**Acceptance Criteria**:
- [ ] `checkPrerequisites()` returns satisfied when `dlv` is installed
- [ ] `checkPrerequisites()` returns unsatisfied with install hint when `dlv` is missing
- [ ] `launch()` starts Delve DAP server and returns working DAPConnection
- [ ] DAP messages can be sent/received through the connection
- [ ] `dispose()` kills the Delve process and debugee
- [ ] `parseGoCommand("go run main.go")` returns `{ mode: "debug", program: "main.go", args: [] }`
- [ ] `parseGoCommand("go test ./...")` returns `{ mode: "test", program: "./...", args: [] }`
- [ ] `parseGoCommand("./mybinary --flag")` returns `{ mode: "exec", program: "./mybinary", args: ["--flag"] }`

---

### Unit 5: DAPConnection Extension for Adapter-Specific Launch Args

**File**: `src/adapters/base.ts`

Extend `DAPConnection` with an optional `launchArgs` field that adapters can populate with debugger-specific DAP launch request arguments.

```typescript
export interface DAPConnection {
	reader: Readable;
	writer: Writable;
	process?: ChildProcess;
	/**
	 * Adapter-specific arguments to pass in the DAP launch request.
	 * Merged with the session manager's default launch args.
	 * Used by adapters like Go/Delve that need mode/program/args in the DAP launch.
	 */
	launchArgs?: Record<string, unknown>;
}
```

**File**: `src/core/session-manager.ts`

Update `launch()` to merge `connection.launchArgs` into the DAP launch request.

```typescript
// In SessionManager.launch(), after connecting and initializing:
const dapLaunchArgs: Record<string, unknown> = {
	noDebug: false,
	...connection.launchArgs, // adapter-specific args (e.g., Delve's mode, program)
};
await dapClient.launch(dapLaunchArgs as DebugProtocol.LaunchRequestArguments);
```

**Implementation Notes**:
- For Python (debugpy): no `launchArgs` needed — debugpy launches the script as part of its spawn command, and the DAP `launch` request just says "go". `launchArgs` remains `undefined`.
- For Node.js (js-debug): `launchArgs` = `{ type: "pwa-node", program: script, args, cwd, sourceMaps: true }`
- For Go (Delve): `launchArgs` = `{ mode: "debug", program: "/abs/path/to/main.go", args: [], cwd }`
- The `launchArgs` approach avoids modifying the `DebugAdapter` interface signature — it piggybacks on the existing `DAPConnection` return type.

**Acceptance Criteria**:
- [ ] `DAPConnection.launchArgs` is optional and backward-compatible
- [ ] Session manager merges `launchArgs` into DAP launch request
- [ ] Existing Python adapter tests pass (no `launchArgs` = no change)
- [ ] Go adapter's `launchArgs` are correctly passed through to Delve

---

### Unit 6: Value Renderer — Multi-Language Support

**File**: `src/core/value-renderer.ts`

Extend the value renderer to handle JavaScript and Go type names and internal variables.

```typescript
/**
 * JavaScript internal variable names to filter from the default locals display.
 */
export const JS_INTERNAL_NAMES: ReadonlySet<string> = new Set([
	"__proto__",
	"constructor",
	"__defineGetter__",
	"__defineSetter__",
	"__lookupGetter__",
	"__lookupSetter__",
	"hasOwnProperty",
	"isPrototypeOf",
	"propertyIsEnumerable",
	"toLocaleString",
	"toString",
	"valueOf",
]);

/**
 * Go internal variable names to filter.
 * Delve exposes runtime internals that are not useful for debugging.
 */
export const GO_INTERNAL_NAMES: ReadonlySet<string> = new Set([
	"runtime.curg",
	"runtime.frameoff",
	"&runtime.g",
]);
```

Update `isInternalVariable()`:

```typescript
export function isInternalVariable(name: string): boolean {
	return (
		PYTHON_INTERNAL_NAMES.has(name) ||
		JS_INTERNAL_NAMES.has(name) ||
		GO_INTERNAL_NAMES.has(name) ||
		/^__\w+__$/.test(name)
	);
}
```

Update `renderDAPVariable()` to handle JS and Go types:

```typescript
// Add to renderDAPVariable() — JS type handling:
// JavaScript types from js-debug: "number", "string", "boolean", "undefined", "null", "object", "function", "symbol", "bigint"
if (type === "number" || type === "bigint") return value;
if (type === "boolean") return value;
if (type === "string") return renderString(value, options.stringTruncateLength);
if (type === "undefined") return "undefined";
if (type === "null" || value === "null") return "null";
if (type === "function") return `<function ${value.length > 40 ? value.slice(0, 40) + "..." : value}>`;
if (type === "symbol") return value;

// Go types from Delve: "int", "int64", "float64", "string", "bool", "[]int", "map[string]int", "*main.Foo", "main.Foo"
// Go slices
if (type.startsWith("[]")) return renderCollection(value, type, options.collectionPreviewItems);
// Go maps
if (type.startsWith("map[")) return renderCollection(value, type, options.collectionPreviewItems);
// Go pointers
if (type.startsWith("*")) return renderObject(value, type, options.depth, options.maxDepth);
```

**Implementation Notes**:
- The existing Python type checks (`"int"`, `"float"`, `"bool"`, `"str"`, `"NoneType"`, `"list"`, `"dict"`) remain unchanged
- JS types are checked after Python types — the overlap (`"int"` appears in both but Python takes precedence, `"number"` is JS-only) is handled by ordering
- Go types often include package prefixes (`main.User`, `*main.User`). The renderer strips the package prefix for display: `<User>` not `<main.User>`
- Go slices display like Python lists: `[1, 2, 3, ... (47 items)]`
- Go maps display like Python dicts: `{key1: val1, key2: val2, ... (5 items)}`

**Acceptance Criteria**:
- [ ] JS internal variables are filtered
- [ ] Go internal variables are filtered
- [ ] JS primitives render correctly: `number`, `string`, `boolean`, `undefined`, `null`
- [ ] JS `function` type renders as `<function name>`
- [ ] Go slices render as collections with item preview
- [ ] Go maps render as dict-like collections
- [ ] Go pointer types render as objects
- [ ] Go struct types render as objects with package prefix stripped
- [ ] Existing Python rendering is unchanged (all unit tests pass)

---

### Unit 7: Test Fixtures

**Files**: Multiple new fixture files

**`tests/fixtures/node/simple-loop.js`**:
```javascript
/**
 * Simple loop for basic stepping and variable inspection.
 * Equivalent to python/simple-loop.py.
 */
function sumRange(n) {
	let total = 0;
	for (let i = 0; i < n; i++) {
		total += i;
	}
	return total;
}

const result = sumRange(10);
console.log(`Sum: ${result}`);
```

**`tests/fixtures/node/function-calls.js`**:
```javascript
/**
 * Nested function calls for call stack testing.
 * Equivalent to python/function-calls.py.
 */
function add(a, b) {
	return a + b;
}

function multiply(a, b) {
	let result = 0;
	for (let i = 0; i < b; i++) {
		result = add(result, a);
	}
	return result;
}

function calculate(x, y) {
	const product = multiply(x, y);
	const sum = add(product, 10);
	return sum;
}

const answer = calculate(5, 3);
console.log(`Answer: ${answer}`);
```

**`tests/fixtures/node/async-await.js`**:
```javascript
/**
 * Async/await for testing async stack traces.
 */
async function fetchData(id) {
	const data = { id, name: `item-${id}`, value: id * 10 };
	return data;
}

async function processItems(ids) {
	const results = [];
	for (const id of ids) {
		const data = await fetchData(id);
		results.push(data);
	}
	return results;
}

async function main() {
	const items = await processItems([1, 2, 3]);
	console.log(`Processed ${items.length} items`);
}

main();
```

**`tests/fixtures/go/simple-loop.go`**:
```go
// Simple loop for basic stepping and variable inspection.
// Equivalent to python/simple-loop.py.
package main

import "fmt"

func sumRange(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

func main() {
	result := sumRange(10)
	fmt.Printf("Sum: %d\n", result)
}
```

**`tests/fixtures/go/function-calls.go`**:
```go
// Nested function calls for call stack testing.
// Equivalent to python/function-calls.py.
package main

import "fmt"

func add(a, b int) int {
	return a + b
}

func multiply(a, b int) int {
	result := 0
	for i := 0; i < b; i++ {
		result = add(result, a)
	}
	return result
}

func calculate(x, y int) int {
	product := multiply(x, y)
	sum := add(product, 10)
	return sum
}

func main() {
	answer := calculate(5, 3)
	fmt.Printf("Answer: %d\n", answer)
}
```

**Acceptance Criteria**:
- [ ] All fixture programs run successfully outside the debugger
- [ ] Node fixtures produce expected output (`Sum: 45`, `Answer: 25`, `Processed 3 items`)
- [ ] Go fixtures compile and produce expected output (`Sum: 45`, `Answer: 25`)

---

### Unit 8: Prerequisite Check Helpers

**File**: `tests/helpers/node-check.ts`

```typescript
import { spawn } from "node:child_process";

/**
 * Check if Node.js 18+ is available and js-debug adapter is usable.
 */
export async function isNodeDebugAvailable(): Promise<boolean>;

/**
 * Whether Node debug is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_NODE_DEBUG: boolean;
```

**File**: `tests/helpers/dlv-check.ts`

```typescript
import { spawn } from "node:child_process";

/**
 * Check if Delve (dlv) is installed and usable.
 */
export async function isDlvAvailable(): Promise<boolean>;

/**
 * Whether dlv is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_DLV: boolean;
```

**Implementation Notes**:
- Follow the exact same pattern as `tests/helpers/debugpy-check.ts`
- `isNodeDebugAvailable()`: spawn `node --version`, check version ≥ 18, and verify js-debug adapter is cached (or can be obtained)
- `isDlvAvailable()`: spawn `dlv version`, check exit code 0

**Acceptance Criteria**:
- [ ] `SKIP_NO_NODE_DEBUG` correctly reflects Node.js debug availability
- [ ] `SKIP_NO_DLV` correctly reflects Delve availability
- [ ] Tests using these skip cleanly when the debugger is not installed

---

### Unit 9: Adapter Registration in Entry Points

**File**: `src/mcp/index.ts`

```typescript
import { GoAdapter } from "../adapters/go.js";
import { NodeAdapter } from "../adapters/node.js";
import { PythonAdapter } from "../adapters/python.js";
import { registerAdapter } from "../adapters/registry.js";

registerAdapter(new PythonAdapter());
registerAdapter(new NodeAdapter());
registerAdapter(new GoAdapter());
```

**File**: `src/daemon/entry.ts`

Same three `registerAdapter()` calls.

**File**: `src/cli/commands/doctor.ts`

Add version detection for Node.js and Delve alongside the existing Python/debugpy version detection.

```typescript
// In runDoctorChecks(), extend the version detection:
if (adapter.id === "python") {
	version = await getPythonDebugpyVersion();
} else if (adapter.id === "node") {
	version = await getNodeVersion();
} else if (adapter.id === "go") {
	version = await getDlvVersion();
}
```

```typescript
async function getNodeVersion(): Promise<string | undefined>;
async function getDlvVersion(): Promise<string | undefined>;
```

**Implementation Notes**:
- `getNodeVersion()`: spawn `node --version`, parse output (e.g., `v20.11.0` → `20.11.0`)
- `getDlvVersion()`: spawn `dlv version`, parse the "Version:" line from output
- Doctor command also needs to import and register the new adapters (currently it only registers `PythonAdapter`)

**Acceptance Criteria**:
- [ ] `agent-lens doctor` shows Node.js and Go adapter status
- [ ] Version strings are correctly parsed and displayed
- [ ] Missing adapters show install hints
- [ ] MCP server registers all three adapters
- [ ] Daemon registers all three adapters

---

### Unit 10: Node.js Integration Tests

**File**: `tests/integration/adapters/node.test.ts`

```typescript
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { NodeAdapter } from "../../../src/adapters/node.js";
import { SKIP_NO_NODE_DEBUG } from "../../helpers/node-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/node/simple-loop.js");

describe.skipIf(SKIP_NO_NODE_DEBUG)("NodeAdapter integration", () => {
	let adapter: NodeAdapter;

	afterEach(async () => {
		try { await adapter?.dispose(); } catch { /* ignore */ }
	});

	it("checkPrerequisites() returns satisfied: true", async () => { /* ... */ });
	it("launch() spawns Node and returns a working DAPConnection", async () => { /* ... */ });
	it("DAPConnection can send/receive DAP messages", async () => { /* ... */ });
	it("dispose() kills the child processes", async () => { /* ... */ });
	it("launch with bad script path produces clear error", async () => { /* ... */ });
});
```

**Implementation Notes**:
- Follow the exact same test structure as `tests/integration/adapters/python.test.ts`
- Test fixture: `tests/fixtures/node/simple-loop.js`
- Verify DAP initialize handshake works through the js-debug adapter

**Acceptance Criteria**:
- [ ] All 5 tests pass when Node.js and js-debug adapter are available
- [ ] Tests skip cleanly when Node.js is not installed
- [ ] Process cleanup is verified (no orphaned Node or adapter processes)

---

### Unit 11: Go Integration Tests

**File**: `tests/integration/adapters/go.test.ts`

```typescript
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { GoAdapter } from "../../../src/adapters/go.js";
import { SKIP_NO_DLV } from "../../helpers/dlv-check.js";

const FIXTURE = resolve(import.meta.dirname, "../../fixtures/go/simple-loop.go");

describe.skipIf(SKIP_NO_DLV)("GoAdapter integration", () => {
	let adapter: GoAdapter;

	afterEach(async () => {
		try { await adapter?.dispose(); } catch { /* ignore */ }
	});

	it("checkPrerequisites() returns satisfied: true", async () => { /* ... */ });
	it("launch() starts Delve and returns a working DAPConnection", async () => { /* ... */ });
	it("DAPConnection can send/receive DAP messages", async () => { /* ... */ });
	it("dispose() kills the Delve process", async () => { /* ... */ });
	it("launch with bad Go file produces clear error", async () => { /* ... */ });
});
```

**Implementation Notes**:
- Follow the exact same test structure as the Python and Node adapter tests
- Go compilation can take a few seconds — set a longer timeout for the launch test (15s)
- Delve's DAP server takes a moment to start accepting connections — the `connectTCP` retry logic in Unit 1 handles this

**Acceptance Criteria**:
- [ ] All 5 tests pass when Go and Delve are available
- [ ] Tests skip cleanly when `dlv` is not installed
- [ ] Go programs are compiled and debugged successfully
- [ ] Process cleanup is verified (no orphaned Delve or Go processes)

---

### Unit 12: Cross-Adapter Test Matrix

**File**: `tests/e2e/mcp/cross-adapter.test.ts`

Shared test scenarios run against all three adapters. Each scenario sets up the same logical program in Python, Node.js, and Go, then verifies consistent viewport behavior.

```typescript
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { SKIP_NO_DEBUGPY } from "../../helpers/debugpy-check.js";
import { SKIP_NO_DLV } from "../../helpers/dlv-check.js";
import { SKIP_NO_NODE_DEBUG } from "../../helpers/node-check.js";
import { callTool, createTestClient } from "../../helpers/mcp-test-client.js";

interface AdapterFixture {
	name: string;
	skip: boolean;
	command: string;
	fixture: string;
	/** Line number of `total += i` or equivalent inside the loop */
	loopLine: number;
	/** Line number of the function entry */
	functionEntryLine: number;
	/** Expected variable names at loop breakpoint */
	expectedVars: string[];
}

const ADAPTERS: AdapterFixture[] = [
	{
		name: "python",
		skip: SKIP_NO_DEBUGPY,
		command: "python3 {fixture}",
		fixture: "tests/fixtures/python/simple-loop.py",
		loopLine: 7,
		functionEntryLine: 4,
		expectedVars: ["total", "i", "n"],
	},
	{
		name: "node",
		skip: SKIP_NO_NODE_DEBUG,
		command: "node {fixture}",
		fixture: "tests/fixtures/node/simple-loop.js",
		loopLine: 8,
		functionEntryLine: 6,
		expectedVars: ["total", "i", "n"],
	},
	{
		name: "go",
		skip: SKIP_NO_DLV,
		command: "{fixture}",
		fixture: "tests/fixtures/go/simple-loop.go",
		loopLine: 11,
		functionEntryLine: 8,
		expectedVars: ["total", "i", "n"],
	},
];

for (const adapter of ADAPTERS) {
	describe.skipIf(adapter.skip)(`Cross-adapter: ${adapter.name}`, () => {
		// ... shared test scenarios
	});
}
```

**Shared Test Scenarios** (10 total):

1. **Launch and stop** — `debug_launch` → `debug_stop`. Verify session creates and terminates cleanly.
2. **Breakpoint hit** — Set breakpoint at loop line, continue, verify viewport shows `STOPPED` at correct location.
3. **Step over** — Step over from loop body, verify line advances.
4. **Step into** — Set breakpoint at function call, step into, verify entering the function.
5. **Step out** — Inside a function, step out, verify returning to caller.
6. **Variable inspection** — At a breakpoint, verify `debug_variables` returns expected local variables with rendered values.
7. **Expression evaluation** — At a breakpoint, evaluate `total + 1`, verify the result is correct.
8. **Stack trace** — At a breakpoint inside a function, verify stack trace shows the function and caller.
9. **Continue to end** — Remove breakpoints, continue, verify session terminates.
10. **Stop on entry** — Launch with `stop_on_entry: true`, verify viewport shows first line.

**Implementation Notes**:
- Each adapter block uses `beforeAll`/`afterAll` to create/destroy the MCP test client
- The `command` template uses `{fixture}` placeholder replaced with the resolved absolute path
- Go requires `go run {fixture}` or the adapter's `parseGoCommand` handles the `.go` file extension
- Viewport assertions use `toContain()` for text matching — we verify structure, not exact format (type names differ across languages: `int` vs `number` vs `int`)
- Variable value assertions check that expected variable names appear, not exact type rendering
- Tests have a 30-second timeout per test (Go compilation can be slow)

**Acceptance Criteria**:
- [ ] All 10 scenarios pass for Python (when debugpy available)
- [ ] All 10 scenarios pass for Node.js (when js-debug available)
- [ ] All 10 scenarios pass for Go (when dlv available)
- [ ] Viewport structure is consistent across adapters (header, stack, source, locals sections present)
- [ ] Variable names match across adapters for equivalent programs
- [ ] Tests skip cleanly per-adapter when the debugger is not installed

---

### Unit 13: Python Adapter Refactor

**File**: `src/adapters/python.ts`

Refactor to use shared helpers from `helpers.ts`.

```typescript
// Before (in python.ts):
export function allocatePort(): Promise<number> { /* ... */ }

// After (in python.ts):
import { allocatePort, connectTCP, spawnAndWait } from "./helpers.js";
// Remove local allocatePort definition
// Update launch() to use spawnAndWait() and connectTCP()
```

**Implementation Notes**:
- `allocatePort()` export is removed from `python.ts` (it moves to `helpers.ts`)
- Any code that imports `allocatePort` from `python.ts` must be updated to import from `helpers.ts`
- The `launch()` method body is simplified to use `spawnAndWait()` and `connectTCP()`
- `parseCommand()` stays in `python.ts` (it's Python-specific)
- All existing Python tests must continue to pass after this refactor

**Acceptance Criteria**:
- [ ] `python.ts` no longer exports `allocatePort`
- [ ] `python.ts` uses `spawnAndWait()` from `helpers.ts`
- [ ] `python.ts` uses `connectTCP()` from `helpers.ts`
- [ ] All existing unit and integration tests for Python adapter pass
- [ ] No behavior changes — the refactor is purely structural

---

## Implementation Order

1. **Unit 1: Shared Adapter Helpers** — extract common code, foundation for all adapters
2. **Unit 13: Python Adapter Refactor** — update Python to use shared helpers, verify nothing breaks
3. **Unit 5: DAPConnection Extension** — add `launchArgs` field, update session manager
4. **Unit 6: Value Renderer** — add JS/Go type support (can be done in parallel with adapters)
5. **Unit 7: Test Fixtures** — create all fixture files (can be done in parallel)
6. **Unit 8: Prerequisite Check Helpers** — create test skip helpers (can be done in parallel)
7. **Unit 3: js-debug Adapter Manager** — download/cache js-debug DAP adapter
8. **Unit 2: Node.js Adapter** — implement NodeAdapter (depends on Units 1, 3, 5)
9. **Unit 4: Go Adapter** — implement GoAdapter (depends on Units 1, 5)
10. **Unit 9: Adapter Registration** — register new adapters in entry points (depends on Units 2, 4)
11. **Unit 10: Node.js Integration Tests** — test NodeAdapter (depends on Units 2, 7, 8)
12. **Unit 11: Go Integration Tests** — test GoAdapter (depends on Units 4, 7, 8)
13. **Unit 12: Cross-Adapter Test Matrix** — end-to-end cross-adapter validation (depends on all above)

**Parallelization opportunities**:
- Units 4+6+7+8 can be implemented in parallel after Unit 1+5
- Units 10+11 can be implemented in parallel after their respective adapters
- Unit 12 depends on everything else

---

## Testing

### Unit Tests

**`tests/unit/adapters/node.test.ts`**:
- `parseNodeCommand()` — various command strings
- `NodeAdapter` — id, fileExtensions, displayName properties

**`tests/unit/adapters/go.test.ts`**:
- `parseGoCommand()` — `go run`, `go test`, bare binary, with flags
- `GoAdapter` — id, fileExtensions, displayName properties

**`tests/unit/adapters/helpers.test.ts`**:
- `allocatePort()` — returns valid port numbers (moved from python.test.ts)
- `spawnAndWait()` — readiness pattern matching, timeout handling (mocked process)
- `connectTCP()` — connection and retry logic (mocked socket)

**`tests/unit/core/value-renderer.test.ts`** (additions):
- JS type rendering: `number`, `string`, `boolean`, `undefined`, `null`, `function`, `symbol`, `bigint`
- Go type rendering: slices (`[]int`), maps (`map[string]int`), pointers (`*main.User`), structs (`main.User`)
- JS internal variable filtering
- Go internal variable filtering

### Integration Tests

**`tests/integration/adapters/node.test.ts`**: 5 tests (Unit 10)
**`tests/integration/adapters/go.test.ts`**: 5 tests (Unit 11)

### E2E Tests

**`tests/e2e/mcp/cross-adapter.test.ts`**: 10 scenarios × 3 adapters = 30 tests (Unit 12)

---

## Verification Checklist

```bash
# All existing tests still pass
bun run test:unit
bun run test:integration
bun run test:e2e

# New unit tests
bun run test tests/unit/adapters/helpers.test.ts
bun run test tests/unit/adapters/node.test.ts
bun run test tests/unit/adapters/go.test.ts

# New integration tests (require debuggers)
bun run test tests/integration/adapters/node.test.ts
bun run test tests/integration/adapters/go.test.ts

# Cross-adapter matrix (require all debuggers)
bun run test tests/e2e/mcp/cross-adapter.test.ts

# Doctor shows all adapters
bun run dev doctor

# Lint passes
bun run lint
```
