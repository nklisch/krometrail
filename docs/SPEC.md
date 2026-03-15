---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Krometrail — Specification

This document defines the formal contracts, types, and configuration parameters that implementors must conform to.

---

## Language Adapter Interface

Each language adapter implements a single interface. The adapter's sole responsibility is to launch the debugee and return a DAP connection. All subsequent DAP communication is handled by the core.

### Adapter Contract

```typescript
interface DebugAdapter {
  /** Unique identifier, e.g., "python", "node", "go" */
  id: string;

  /** File extensions this adapter handles */
  fileExtensions: string[];

  /** Alternative language names that map to this adapter, e.g., ["javascript", "typescript", "ts", "js"] */
  aliases?: string[];

  /** Human-readable name for error messages */
  displayName: string;

  /** Check if the adapter's debugger is available on this system */
  checkPrerequisites(): Promise<PrerequisiteResult>;

  /** Launch the debugee and return a DAP connection */
  launch(config: LaunchConfig): Promise<DAPConnection>;

  /** Attach to an already-running process */
  attach(config: AttachConfig): Promise<DAPConnection>;

  /** Clean up adapter-specific resources */
  dispose(): Promise<void>;
}

interface PrerequisiteResult {
  satisfied: boolean;
  missing?: string[];         // e.g., ["debugpy not installed"]
  installHint?: string;       // e.g., "pip install debugpy"
}

interface DAPConnection {
  reader: Readable;           // DAP messages from debugger (Node stream)
  writer: Writable;           // DAP messages to debugger (Node stream)
  process?: ChildProcess;     // The debugee process, if launched
  launchArgs?: Record<string, unknown>;  // Adapter-specific DAP launch request fields
}

interface LaunchConfig {
  command: string;            // Full command to execute
  cwd?: string;
  env?: Record<string, string>;
  args?: string[];
  port?: number;              // Allocated by core, adapter should use this
}

interface AttachConfig {
  pid?: number;
  port?: number;
  host?: string;
  env?: Record<string, string>;
}
```

### Reference Adapters

| Language | Debugger | Extensions | Launch Pattern |
|----------|----------|------------|----------------|
| Python | debugpy | `.py` | `python -m debugpy --listen 0:PORT --wait-for-client script.py` |
| Node.js | built-in inspector | `.js`, `.ts`, `.mjs`, `.cjs`, `.mts`, `.cts`, `.tsx` | `node --inspect-brk=PORT script.js` |
| Go | delve (dlv) | `.go` | `dlv dap --listen :PORT` |
| Rust | codelldb | `.rs` | `codelldb --port PORT` |
| Java | java-debug-adapter | `.java` | `java -agentlib:jdwp=... -jar target.jar` |
| C/C++ | cppdbg (GDB/LLDB) | `.c`, `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp` | `gdb --interpreter=dap ./binary` |
| Ruby | rdbg | `.rb` | `rdbg --open --port PORT script.rb` |
| C# | netcoredbg | `.cs` | `netcoredbg --interpreter=dap` |
| Swift | lldb-dap | `.swift` | `lldb-dap --port PORT` |
| Kotlin | kotlin-debug-adapter | `.kt`, `.kts` | `kotlin-debug-adapter` |

> **Prior art note:** Existing projects take different approaches to adapter management. mcp-debugger uses a dynamic adapter registry with runtime discovery, lazy loading, and adapter vendoring (downloading vscode-js-debug and CodeLLDB during install). AIDB builds adapters in CI and downloads them on first run. debugger-mcp uses Docker containers with pre-installed debuggers. dap-mcp uses a config-driven approach with Pydantic discriminated unions. Krometrail keeps the adapter boundary deliberately narrow — the contract below is all an adapter needs to implement. See [PRIOR_ART.md](PRIOR_ART.md).

### Adding a New Adapter

Adding support for a new language requires implementing the `DebugAdapter` interface. The typical effort involves:

1. Identifying the language's DAP-compatible debugger and its launch protocol.
2. Implementing `launch`/`attach` methods to start the debugger and return a DAP socket or stream.
3. Implementing `checkPrerequisites` to verify the debugger is installed.
4. Registering the adapter with the core's adapter registry.

No changes to the core, viewport logic, or MCP tool definitions are required. The adapter boundary is intentionally narrow to make contributions straightforward.

---

## Breakpoint Type

```typescript
interface Breakpoint {
  line: number;                // Required. Line number.
  condition?: string;          // Expression that must be true to trigger.
                               // E.g., "discount < 0"
  hit_condition?: string;      // Break after N hits. E.g., ">=100"
  log_message?: string;        // Log instead of breaking.
                               // Supports {expression} interpolation.
}
```

---

## Resource Limits

Debug sessions consume system resources and agent context. The server enforces configurable safety limits:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `session_timeout_ms` | `300000` | Max wall-clock time for a session (5 min) |
| `max_actions_per_session` | `200` | Max debug actions before forced termination |
| `max_concurrent_sessions` | `3` | Per-agent concurrent session limit |
| `step_timeout_ms` | `30000` | Max time to wait for a single stop event |
| `max_output_bytes` | `1048576` | Max debugee stdout/stderr captured (1 MB) |
| `max_evaluate_time_ms` | `5000` | Max time for expression evaluation |

When a limit is hit, the server returns a structured error with the limit name, the current value, and a suggestion (e.g., "Consider using conditional breakpoints to reduce step count").
