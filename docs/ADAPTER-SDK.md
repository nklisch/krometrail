---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Creating an Krometrail Adapter

This guide walks through creating a new language adapter for Krometrail. By the end, you'll have a working adapter that integrates with the CLI, MCP server, and doctor command.

## Overview

Adapters are the bridge between Krometrail and language-specific debuggers. Each adapter:

1. Checks that the debugger is installed (`checkPrerequisites`)
2. Launches a debugger process and returns a DAP connection (`launch`)
3. Attaches to an already-running process (`attach`)
4. Cleans up resources when the session ends (`dispose`)

The session manager handles all DAP protocol communication — the adapter just needs to provide a readable/writable stream connected to the debugger.

## The DebugAdapter Interface

```typescript
// src/adapters/base.ts

export interface PrerequisiteResult {
    satisfied: boolean;
    missing?: string[];      // e.g., ["debugpy", "python3"]
    installHint?: string;    // shown in `krometrail doctor`
}

export interface DAPConnection {
    reader: Readable;  // reads DAP messages from the debugger
    writer: Writable;  // writes DAP messages to the debugger
    process?: ChildProcess;  // the debugger subprocess (for cleanup)
    launchArgs?: Record<string, unknown>;  // adapter-specific DAP launch request fields
}

export interface LaunchConfig {
    command: string;     // the user's command, e.g., "python3 app.py"
    cwd?: string;
    env?: Record<string, string>;
    port?: number;       // pre-allocated port (if you called allocatePort())
}

export interface AttachConfig {
    pid?: number;    // attach by process ID
    port?: number;   // attach to debug server port
    host?: string;   // debug server host
}

export interface DebugAdapter {
    id: string;                    // unique: "python", "node", "go", etc.
    fileExtensions: string[];      // [".py"] or [".js", ".ts"]
    displayName: string;           // shown in `krometrail doctor`
    checkPrerequisites(): Promise<PrerequisiteResult>;
    launch(config: LaunchConfig): Promise<DAPConnection>;
    attach(config: AttachConfig): Promise<DAPConnection>;
    dispose(): Promise<void>;
}
```

## Shared Helpers

All adapters should use helpers from `src/adapters/helpers.ts`:

```typescript
import { allocatePort, connectTCP, gracefulDispose, spawnAndWait } from "./helpers.js";

// Allocate a free TCP port (used before spawning the debugger)
const port = await allocatePort();

// Spawn a process and wait for a ready signal on stderr
await spawnAndWait(cmd, args, { cwd, env }, /Listening on port/);

// Connect TCP to the debugger's DAP server
const socket = await connectTCP(port, "127.0.0.1");

// Gracefully kill the socket and process
await gracefulDispose(socket, process);
```

## Step-by-Step Guide

### Step 1: Identify the DAP Debugger

Find the DAP-compatible debugger for your language:

- Does it communicate over **TCP** (most common) or **stdin/stdout** (GDB, some others)?
- Does it need to be **downloaded** (CodeLLDB, js-debug) or is it **system-installed** (GDB, dlv)?
- What **DAP capabilities** does it support? (conditional breakpoints, step-in, evaluate, etc.)
- What fields does it expect in the **DAP `launch` request**?

### Step 2: Create the Adapter File

```
src/adapters/{language}.ts
```

Start with the skeleton:

```typescript
import type { ChildProcess } from "node:child_process";
import type { Socket } from "node:net";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, gracefulDispose, spawnAndWait } from "./helpers.js";

export class MyLanguageAdapter implements DebugAdapter {
    id = "mylang";
    fileExtensions = [".ml"];
    displayName = "My Language (my-debugger)";

    private process: ChildProcess | null = null;
    private socket: Socket | null = null;

    async checkPrerequisites(): Promise<PrerequisiteResult> { /* ... */ }
    async launch(config: LaunchConfig): Promise<DAPConnection> { /* ... */ }
    async attach(config: AttachConfig): Promise<DAPConnection> { /* ... */ }
    async dispose(): Promise<void> { /* ... */ }
}
```

### Step 3: Implement checkPrerequisites()

Spawn the debugger's version command and check the result:

```typescript
async checkPrerequisites(): Promise<PrerequisiteResult> {
    return new Promise((resolve) => {
        const proc = spawn("my-debugger", ["--version"], { stdio: "pipe" });
        let output = "";
        proc.stdout?.on("data", (d: Buffer) => { output += d.toString(); });
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

### Step 4: Implement launch() — TCP Transport

Most debuggers listen on a TCP port. The pattern is:

```typescript
async launch(config: LaunchConfig): Promise<DAPConnection> {
    const cwd = config.cwd ?? process.cwd();

    // 1. Parse the command to extract script/args
    const parts = config.command.trim().split(/\s+/);
    const script = parts[1] ?? "";
    const args = parts.slice(2);

    // 2. Allocate a port
    const port = await allocatePort();

    // 3. Spawn the debugger (listens on the port)
    //    Use spawnAndWait if you can detect readiness via stderr
    this.process = await spawnAndWait(
        "my-debugger",
        ["--port", String(port), script, ...args],
        { cwd, env: { ...process.env, ...config.env } },
        /Listening on port \d+/,  // regex matching the ready signal
        5000,  // timeout in ms
    );

    // 4. Connect TCP
    this.socket = await connectTCP(port);

    // 5. Return the connection
    return {
        reader: this.socket,
        writer: this.socket,
        process: this.process,
        launchArgs: {
            // Adapter-specific fields for the DAP 'launch' request
            program: script,
            cwd,
            env: config.env ?? {},
        },
    };
}
```

### Step 4b: Implement launch() — Stdin/Stdout Transport

For debuggers that use stdin/stdout (like GDB with `--interpreter=dap`):

```typescript
async launch(config: LaunchConfig): Promise<DAPConnection> {
    const child = spawn("my-debugger", ["--interpreter=dap"], {
        cwd: config.cwd ?? process.cwd(),
        env: { ...process.env, ...config.env },
        stdio: ["pipe", "pipe", "pipe"],
    });

    this.process = child;

    // Wait briefly for early crash detection
    const earlyError = await new Promise<Error | null>((resolve) => {
        child.on("error", (err) => resolve(new LaunchError(`Failed to spawn: ${err.message}`)));
        child.on("close", (code) => {
            if (code !== null && code !== 0) resolve(new LaunchError(`Exited with code ${code}`));
            else resolve(null);
        });
        setTimeout(() => resolve(null), 500);
    });
    if (earlyError) throw earlyError;

    return {
        reader: child.stdout!,    // NOTE: no socket — use process streams directly
        writer: child.stdin!,
        process: child,
        launchArgs: { program: "/path/to/binary", cwd: config.cwd },
    };
}
```

### Step 5: Implement attach()

```typescript
async attach(config: AttachConfig): Promise<DAPConnection> {
    // For TCP attach: connect to existing debug server
    if (config.port) {
        this.socket = await connectTCP(config.port, config.host ?? "127.0.0.1");
        return {
            reader: this.socket,
            writer: this.socket,
            launchArgs: { request: "attach", port: config.port, host: config.host },
        };
    }

    // For PID attach: spawn debugger and attach to process
    const port = await allocatePort();
    this.process = await spawnAndWait(
        "my-debugger",
        ["--port", String(port), "--attach", String(config.pid)],
        {},
        /Listening/,
        5000,
    );
    this.socket = await connectTCP(port);
    return {
        reader: this.socket,
        writer: this.socket,
        process: this.process,
        launchArgs: { request: "attach", pid: config.pid },
    };
}
```

### Step 6: Implement dispose()

```typescript
async dispose(): Promise<void> {
    await gracefulDispose(this.socket, this.process);
    this.socket = null;
    this.process = null;
}
```

For stdin/stdout adapters (no socket):

```typescript
async dispose(): Promise<void> {
    await gracefulDispose(null, this.process);
    this.process = null;
}
```

### Step 7: Register the Adapter

Add to `src/adapters/registry.ts`:

```typescript
import { MyLanguageAdapter } from "./mylang.js";

export function registerAllAdapters(): void {
    // ... existing adapters ...
    registerAdapter(new MyLanguageAdapter());
}
```

The MCP server (`src/mcp/index.ts`) and daemon (`src/daemon/entry.ts`) both call `registerAllAdapters()` — no changes needed there.

### Step 8: Create Conformance Fixture

Create `tests/fixtures/mylang/conformance.ml` following this structure:

```
# A function that takes a parameter
function greet(name):
    message = "Hello, " + name  # insideFunctionLine

# Main program with a loop
function main():
    items = ["alpha", "beta", "gamma"]
    total = 0
    for i, item in items:
        total += len(item)   # loopBodyLine
        greet(item)          # functionCallLine
    print("Total: " + total)
```

The exact line numbers matter — record them for the conformance fixture definition.

### Step 9: Run Conformance Tests

Create `tests/integration/adapters/conformance-mylang.test.ts`:

```typescript
import { resolve } from "node:path";
import { describe } from "vitest";
import { MyLanguageAdapter } from "../../../src/adapters/mylang.js";
import { createSessionManager } from "../../../src/core/session-manager.js";
import type { ConformanceFixture } from "../../harness/adapter-conformance.js";
import { runConformanceSuite } from "../../harness/adapter-conformance.js";

// Create a skip check helper in tests/helpers/mylang-check.ts
const SKIP = !(await new MyLanguageAdapter().checkPrerequisites()).satisfied;

const fixture: ConformanceFixture = {
    filePath: resolve(import.meta.dirname, "../../fixtures/mylang/conformance.ml"),
    command: "my-interpreter conformance.ml",
    language: "mylang",
    loopBodyLine: 10,       // update with actual line numbers
    functionCallLine: 11,
    insideFunctionLine: 3,
    expectedLocals: ["items", "total", "i", "item"],
    evalExpression: "len(items)",
    evalExpectedSubstring: "3",
};

describe.skipIf(SKIP)("My Language adapter conformance", () => {
    runConformanceSuite(new MyLanguageAdapter(), fixture, createSessionManager);
});
```

### Step 10: Add Doctor Version Check

In `src/cli/commands/doctor.ts`, add a version getter:

```typescript
async function getMyLangVersion(): Promise<string | undefined> {
    try {
        const { spawn } = await import("node:child_process");
        const result = await new Promise<string>((resolve, reject) => {
            const proc = spawn("my-debugger", ["--version"], { stdio: "pipe" });
            let stdout = "";
            proc.stdout.on("data", (chunk: Buffer) => { stdout += chunk.toString(); });
            proc.on("close", (code) => {
                if (code === 0) resolve(stdout.trim());
                else reject(new Error("Non-zero exit"));
            });
            proc.on("error", reject);
        });
        const match = result.match(/(\d+\.\d+\.\d+)/);
        return match ? match[1] : undefined;
    } catch {
        return undefined;
    }
}
```

Add to the version detection switch in `runDoctorChecks()`:

```typescript
} else if (adapter.id === "mylang") {
    version = await getMyLangVersion();
}
```

## Reference: Existing Adapters

### Python Adapter (simplest — direct TCP)

`src/adapters/python.ts` — debugpy listens on TCP immediately on spawn. No compilation, no caching. The simplest reference implementation.

Key pattern: `python3 -m debugpy --listen {port} --wait-for-client {script}` then `connectTCP(port)`.

### Node.js Adapter (download/caching)

`src/adapters/node.ts` and `src/adapters/js-debug-adapter.ts` — downloads the js-debug VSIX to `~/.krometrail/adapters/js-debug/`, checks cache before downloading, extracts with unzip. TCP transport.

Key pattern: check cache → download VSIX → extract → spawn adapter with port → connect TCP.

### Go Adapter (build step, goroutine awareness)

`src/adapters/go.ts` — runs `dlv dap` which listens on TCP. If command starts with `go test`, converts to `dlv test`. Sets goroutine-aware `launchArgs`.

Key pattern: detect test vs. run mode, spawn `dlv dap --listen :{port}`, wait for "DAP server listening" on stderr.

### C/C++ Adapter (stdin/stdout transport)

`src/adapters/cpp.ts` — GDB 14+ with `--interpreter=dap` uses stdin/stdout, not TCP. Compiles source files automatically with `gcc -g` or `g++ -g`. Returns `process.stdout`/`process.stdin` as reader/writer.

## Common Patterns

### The `_dapFlow: "launch-first"` Pattern

Some debuggers (like debugpy) require the `launch` DAP request to come before `initialize`. The session manager detects this via the adapter's `launchArgs._dapFlow` field:

```typescript
launchArgs: {
    _dapFlow: "launch-first",  // tells session manager to send launch before initialize
    program: script,
}
```

See `src/adapters/python.ts` for an example.

### Handling Stderr for Readiness Detection

Use `spawnAndWait` with a regex to detect when the debugger is ready:

```typescript
await spawnAndWait(cmd, args, opts, /Listening on port \d+/, 5000);
```

If the debugger outputs nothing on readiness (rare), use a timeout-based approach instead.

### Environment Variable Forwarding

Always merge `process.env` with the adapter config's `env`:

```typescript
env: { ...process.env, ...config.env }
```

### Source Map Considerations

For TypeScript/compiled languages, the debugger may need `sourceMap: true` in launchArgs. Add to `launchArgs` and document which fields your adapter uses.

## FAQ

**What if the debugger doesn't support conditional breakpoints?**

The session manager will still call `setBreakpoints` with condition fields. Your adapter passes them to the debugger. If the debugger ignores them, that's okay — it just won't break conditionally. Document the limitation in your adapter's JSDoc.

**What if the debugger uses a non-TCP transport (stdin/stdout)?**

Return `process.stdout` as `reader` and `process.stdin` as `writer` in `DAPConnection`. Do not pass a `socket`. Call `gracefulDispose(null, this.process)` in `dispose()`. See `src/adapters/cpp.ts` for reference.

**How do I handle debugger-specific launch.json fields?**

Add your type to the `TYPE_TO_LANGUAGE` map in `src/core/launch-json.ts`. If your debugger uses custom launch fields (like `module` for Python), add handling in `configToOptions()`.
