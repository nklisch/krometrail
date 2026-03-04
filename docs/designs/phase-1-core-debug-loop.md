# Design: Phase 1 — Core Debug Loop

## Overview

Phase 1 delivers the end-to-end debug loop: an agent launches a Python script via MCP, hits a breakpoint, sees a viewport, steps, evaluates expressions, and stops. All 7 roadmap sub-units (1.1–1.7) are covered. The design builds on the existing scaffold — types in `core/types.ts`, adapter interface in `adapters/base.ts`, DAP client framing in `core/dap-client.ts`, viewport renderer in `core/viewport.ts`, MCP server entry in `mcp/index.ts`, and CLI stub in `cli/index.ts`.

---

## Implementation Units

### Unit 1: DAP Client Hardening

**File**: `src/core/dap-client.ts`

Extends the existing `DAPClient` class with timeout support, connection lifecycle, initialization handshake, typed request helpers, and event promise helpers.

```typescript
import type { Readable, Writable } from "node:stream";
import type { Socket } from "node:net";
import type { DebugProtocol } from "@vscode/debugprotocol";

export interface DAPClientOptions {
	/** Timeout for individual DAP requests in ms. Default: 10000 */
	requestTimeoutMs: number;
	/** Timeout for waiting on stop events in ms. Default: 30000 */
	stopTimeoutMs: number;
}

export const DEFAULT_DAP_CLIENT_OPTIONS: DAPClientOptions = {
	requestTimeoutMs: 10_000,
	stopTimeoutMs: 30_000,
};

/**
 * Result of waitForStop() — either a stopped event or a terminated event.
 */
export type StopResult =
	| { type: "stopped"; event: DebugProtocol.StoppedEvent }
	| { type: "terminated"; event: DebugProtocol.TerminatedEvent }
	| { type: "exited"; event: DebugProtocol.ExitedEvent };

export class DAPClient {
	private seq: number;
	private pendingRequests: Map<
		number,
		{
			resolve: (response: DebugProtocol.Response) => void;
			reject: (error: Error) => void;
			timer: ReturnType<typeof setTimeout>;
		}
	>;
	private eventHandlers: Map<string, ((event: DebugProtocol.Event) => void)[]>;
	private buffer: Buffer;
	private options: DAPClientOptions;
	private reader: Readable | null;
	private writer: Writable | null;
	private _capabilities: DebugProtocol.Capabilities | null;
	private _connected: boolean;

	constructor(options?: Partial<DAPClientOptions>);

	/** The DAP server's capabilities, available after initialize(). */
	get capabilities(): DebugProtocol.Capabilities;

	/** Whether the client is connected to a DAP server. */
	get connected(): boolean;

	/**
	 * Connect to a DAP server over TCP.
	 * Resolves when the TCP connection is established.
	 */
	connect(host: string, port: number): Promise<void>;

	/**
	 * Attach to existing streams (for testing or non-TCP transports).
	 */
	attachStreams(reader: Readable, writer: Writable): void;

	/**
	 * Disconnect from the DAP server, closing the underlying transport.
	 */
	disconnect(): void;

	/**
	 * Run the full DAP initialization handshake:
	 * 1. Send `initialize` request, store capabilities.
	 * 2. Wait for `initialized` event.
	 * 3. Return capabilities (caller sets breakpoints between this and configurationDone).
	 */
	initialize(): Promise<DebugProtocol.Capabilities>;

	/**
	 * Send a DAP request and wait for the correlated response.
	 * Rejects after requestTimeoutMs with a descriptive error.
	 */
	send<T extends DebugProtocol.Response>(
		command: string,
		args?: Record<string, unknown>,
	): Promise<T>;

	/**
	 * Register an event handler for a specific DAP event type.
	 */
	on(event: string, handler: (event: DebugProtocol.Event) => void): void;

	/**
	 * Remove an event handler.
	 */
	off(event: string, handler: (event: DebugProtocol.Event) => void): void;

	/**
	 * Wait for the debugee to stop (breakpoint, step, exception, pause)
	 * or terminate. Returns a discriminated union.
	 * Rejects after timeoutMs (default: options.stopTimeoutMs).
	 */
	waitForStop(timeoutMs?: number): Promise<StopResult>;

	// --- Typed Request Helpers ---

	/** DAP configurationDone — signals end of breakpoint configuration phase. */
	configurationDone(): Promise<DebugProtocol.ConfigurationDoneResponse>;

	/** DAP launch — launch the debugee. */
	launch(args: DebugProtocol.LaunchRequestArguments): Promise<DebugProtocol.LaunchResponse>;

	/** DAP setBreakpoints — set breakpoints in a single source file. */
	setBreakpoints(
		source: DebugProtocol.Source,
		breakpoints: DebugProtocol.SourceBreakpoint[],
	): Promise<DebugProtocol.SetBreakpointsResponse>;

	/** DAP setExceptionBreakpoints — configure exception breakpoint filters. */
	setExceptionBreakpoints(
		filters: string[],
	): Promise<DebugProtocol.SetExceptionBreakpointsResponse>;

	/** DAP continue — resume execution. */
	continue(threadId: number): Promise<DebugProtocol.ContinueResponse>;

	/** DAP next — step over. */
	next(threadId: number): Promise<DebugProtocol.NextResponse>;

	/** DAP stepIn — step into. */
	stepIn(threadId: number): Promise<DebugProtocol.StepInResponse>;

	/** DAP stepOut — step out. */
	stepOut(threadId: number): Promise<DebugProtocol.StepOutResponse>;

	/** DAP stackTrace — get the call stack for a thread. */
	stackTrace(
		threadId: number,
		startFrame?: number,
		levels?: number,
	): Promise<DebugProtocol.StackTraceResponse>;

	/** DAP scopes — get scopes for a stack frame. */
	scopes(frameId: number): Promise<DebugProtocol.ScopesResponse>;

	/** DAP variables — get variables for a scope/variable reference. */
	variables(variablesReference: number): Promise<DebugProtocol.VariablesResponse>;

	/** DAP evaluate — evaluate an expression in a frame context. */
	evaluate(
		expression: string,
		frameId?: number,
		context?: "watch" | "repl" | "hover" | "clipboard",
	): Promise<DebugProtocol.EvaluateResponse>;

	/** DAP disconnect — ask the debug adapter to disconnect. */
	sendDisconnect(
		terminateDebuggee?: boolean,
	): Promise<DebugProtocol.DisconnectResponse>;

	/** DAP terminate — ask the debugee to terminate gracefully. */
	terminate(): Promise<DebugProtocol.TerminateResponse>;

	/** DAP threads — get all threads. */
	threads(): Promise<DebugProtocol.ThreadsResponse>;

	// --- Private methods (existing, enhanced) ---
	private writeMessage(message: DebugProtocol.ProtocolMessage): void;
	private onData(chunk: Buffer): void;
	private processBuffer(): void;
	private handleMessage(message: DebugProtocol.ProtocolMessage): void;

	/**
	 * Dispose the client: reject all pending requests, clear handlers,
	 * disconnect transport.
	 */
	dispose(): void;
}
```

**Implementation Notes**:
- The `send()` method must create a timeout timer per request. On timeout, reject with `DAPTimeoutError` containing the command name and timeout value, then delete from `pendingRequests`.
- `connect()` uses `net.createConnection({ host, port })` and wraps the socket as both reader and writer. Store the socket reference for `disconnect()`.
- `initialize()` sends the `initialize` request with `clientID: "agent-lens"`, `adapterID: "agent-lens"`, `supportsVariableType: true`, `linesStartAt1: true`, `columnsStartAt1: true`. Then registers a one-shot handler for the `initialized` event using a Promise. The `initialized` event handler resolves the promise. Returns the capabilities from the `InitializeResponse`.
- `waitForStop()` registers handlers for `stopped`, `terminated`, and `exited` events. Whichever fires first resolves the promise. All handlers are cleaned up on resolution. Timeout rejects with `DAPTimeoutError`.
- Each typed helper is a thin wrapper: e.g., `configurationDone()` calls `this.send('configurationDone')`.
- On `disconnect()`, close the socket if it exists, set `_connected = false`.
- `dispose()` calls `disconnect()`, rejects all pending requests with `DAPClientDisposedError`, clears event handlers.

**Acceptance Criteria**:
- [ ] `send()` rejects with `DAPTimeoutError` after `requestTimeoutMs` if no response received
- [ ] `connect()` establishes a TCP connection and sets `connected = true`
- [ ] `disconnect()` closes the transport and sets `connected = false`
- [ ] `initialize()` completes the handshake and stores capabilities
- [ ] `waitForStop()` resolves on `stopped` event with the event payload
- [ ] `waitForStop()` resolves on `terminated` event
- [ ] `waitForStop()` rejects on timeout
- [ ] All typed helpers send the correct DAP command and return typed responses
- [ ] `dispose()` rejects all pending requests and cleans up

---

### Unit 2: Python Adapter

**File**: `src/adapters/python.ts`

Replace the stub `PythonAdapter` with a full implementation that launches debugpy and returns a `DAPConnection`.

```typescript
import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { createConnection, type Socket } from "node:net";
import type {
	AttachConfig,
	DAPConnection,
	DebugAdapter,
	LaunchConfig,
	PrerequisiteResult,
} from "./base.js";

export class PythonAdapter implements DebugAdapter {
	id: string; // "python"
	fileExtensions: string[]; // [".py"]
	displayName: string; // "Python (debugpy)"

	private process: ChildProcess | null;
	private socket: Socket | null;

	constructor();

	/**
	 * Check for python3 and debugpy availability.
	 * Spawns `python3 -m debugpy --version` and parses output.
	 */
	checkPrerequisites(): Promise<PrerequisiteResult>;

	/**
	 * Launch a Python script under debugpy.
	 *
	 * Steps:
	 * 1. Allocate a free port (bind temp socket to port 0, read assigned port, close).
	 * 2. Spawn: `python3 -m debugpy --listen 0.0.0.0:{port} --wait-for-client {script} {args}`
	 * 3. Wait for debugpy's stderr to contain "waiting for client" (with timeout).
	 * 4. Connect a TCP socket to localhost:{port}.
	 * 5. Return DAPConnection with socket as reader/writer and the child process.
	 */
	launch(config: LaunchConfig): Promise<DAPConnection>;

	/**
	 * Attach to an already-running debugpy instance.
	 * Connects to host:port via TCP.
	 */
	attach(config: AttachConfig): Promise<DAPConnection>;

	/**
	 * Kill the child process and close the socket.
	 */
	dispose(): Promise<void>;
}

/**
 * Allocate a free TCP port by binding to port 0, reading the
 * assigned port, and immediately closing the server.
 */
export function allocatePort(): Promise<number>;

/**
 * Parse the script path and arguments from a command string.
 * E.g., "python app.py --verbose" => { script: "app.py", args: ["--verbose"] }
 * Strips leading "python3", "python", or "python3 -m debugpy ..." prefixes.
 */
export function parseCommand(command: string): { script: string; args: string[] };
```

**Implementation Notes**:
- `allocatePort()` creates a `net.Server`, calls `listen(0)`, reads `server.address().port`, then closes. Wrap in a Promise.
- `launch()` spawns the process with `stdio: ['pipe', 'pipe', 'pipe']`. Listen on `stderr` for the string `"waiting"` (debugpy outputs `"Listening on 0.0.0.0:PORT\nWaiting for client..."` or similar). Use a 10-second timeout for this wait.
- `parseCommand()` must handle: `"python app.py"`, `"python3 app.py --arg"`, `"python -m pytest tests/"`, bare `"app.py"`. If the command starts with `python` or `python3`, strip it. If it contains `-m <module>`, keep `-m <module>` as part of the args array.
- For the `-m` case (e.g., `python -m pytest tests/`), the debugpy command becomes: `python3 -m debugpy --listen 0.0.0.0:{port} --wait-for-client -m pytest tests/`.
- `dispose()` sends SIGTERM to the child process, waits 2 seconds, then SIGKILL if still alive. Destroys the socket.
- Store `stderr` output in a buffer for diagnostics (accessible for error messages if launch fails).

**Acceptance Criteria**:
- [ ] `checkPrerequisites()` returns `{ satisfied: true }` when debugpy is installed
- [ ] `checkPrerequisites()` returns `{ satisfied: false, missing: ["debugpy"], installHint: "pip install debugpy" }` when debugpy is missing
- [ ] `launch()` spawns debugpy, waits for listening, returns a working DAPConnection
- [ ] The DAPConnection's reader/writer can send/receive DAP messages
- [ ] `parseCommand()` correctly handles `python app.py`, `python3 -m pytest tests/`, and bare `app.py`
- [ ] `allocatePort()` returns a valid unused port
- [ ] `dispose()` kills the child process and closes the socket
- [ ] Launch failure (bad script path) produces a clear error message including stderr

---

### Unit 3: Session Manager

**File**: `src/core/session-manager.ts`

Replace the stub with a full session orchestrator managing the debug lifecycle.

```typescript
import type { DebugProtocol } from "@vscode/debugprotocol";
import type { DAPClient, StopResult } from "./dap-client.js";
import type {
	Breakpoint,
	ResourceLimits,
	SessionStatus,
	StopReason,
	ViewportConfig,
	ViewportSnapshot,
	Variable,
} from "./types.js";
import { renderViewport } from "./viewport.js";
import type { DebugAdapter, DAPConnection } from "../adapters/base.js";

// --- Launch Options (Zod-validated at the MCP boundary, plain type here) ---

export interface LaunchOptions {
	command: string;
	language?: string;
	breakpoints?: Array<{ file: string; breakpoints: Breakpoint[] }>;
	cwd?: string;
	env?: Record<string, string>;
	viewportConfig?: Partial<ViewportConfig>;
	stopOnEntry?: boolean;
}

// --- Session State Machine ---

export type SessionState = "launching" | "running" | "stopped" | "terminated" | "error";

// --- Action Log Entry ---

export interface ActionLogEntry {
	actionNumber: number;
	tool: string;
	summary: string;
	timestamp: number;
}

// --- Debug Session (internal) ---

export interface DebugSession {
	id: string;
	state: SessionState;
	language: string;
	adapter: DebugAdapter;
	dapClient: DAPClient;
	connection: DAPConnection;
	viewportConfig: ViewportConfig;
	limits: ResourceLimits;
	startedAt: number;
	actionCount: number;
	lastStoppedThreadId: number | null;
	lastStoppedFrameId: number | null;
	/** File path -> Breakpoint[] currently set */
	breakpointMap: Map<string, Breakpoint[]>;
	/** Watch expressions for this session */
	watchExpressions: string[];
	/** Captured stdout/stderr output */
	outputBuffer: OutputBuffer;
	/** Action log */
	actionLog: ActionLogEntry[];
	/** Source file cache: path -> lines */
	sourceCache: Map<string, string[]>;
	/** Session timeout timer */
	timeoutTimer: ReturnType<typeof setTimeout>;
}

// --- Output Buffer ---

export interface OutputBuffer {
	stdout: Array<{ text: string; actionNumber: number }>;
	stderr: Array<{ text: string; actionNumber: number }>;
	totalBytes: number;
}

// --- Launch Result ---

export interface LaunchResult {
	sessionId: string;
	viewport?: string; // Rendered viewport text if stopOnEntry or initial breakpoint hit
	status: SessionStatus;
}

// --- Stop Result ---

export interface StopSessionResult {
	duration: number;
	actionCount: number;
}

// --- Session Manager ---

export class SessionManager {
	private sessions: Map<string, DebugSession>;
	private limits: ResourceLimits;

	constructor(limits: ResourceLimits);

	/**
	 * Launch a new debug session.
	 * 1. Check concurrent session limit.
	 * 2. Resolve adapter from registry (by language or file extension).
	 * 3. Call adapter.launch(config) to get DAPConnection.
	 * 4. Create DAPClient, attach streams, run initialize().
	 * 5. Set initial breakpoints (if any).
	 * 6. Send configurationDone.
	 * 7. Send DAP launch request.
	 * 8. If stopOnEntry, waitForStop and build viewport.
	 * 9. Start session timeout timer.
	 * 10. Register DAP output event handler for capture.
	 */
	launch(options: LaunchOptions): Promise<LaunchResult>;

	/**
	 * Terminate a debug session.
	 * Sends DAP disconnect, kills process, removes from session map,
	 * clears timeout timer.
	 */
	stop(sessionId: string): Promise<StopSessionResult>;

	/**
	 * Continue execution until next stop.
	 * Increments action count, enforces limits, sends DAP continue,
	 * waits for stop, builds viewport.
	 */
	continue(sessionId: string, timeoutMs?: number): Promise<string>;

	/**
	 * Step in the given direction.
	 * Supports count > 1 by looping: step + waitForStop repeated count times.
	 * Returns the viewport after the final step.
	 */
	step(
		sessionId: string,
		direction: "over" | "into" | "out",
		count?: number,
	): Promise<string>;

	/**
	 * Run to a specific location by setting a temp breakpoint, continuing,
	 * then removing the temp breakpoint.
	 */
	runTo(
		sessionId: string,
		file: string,
		line: number,
		timeoutMs?: number,
	): Promise<string>;

	/**
	 * Set breakpoints in a file (DAP semantics: replaces all in that file).
	 */
	setBreakpoints(
		sessionId: string,
		file: string,
		breakpoints: Breakpoint[],
	): Promise<DebugProtocol.Breakpoint[]>;

	/**
	 * Set exception breakpoint filters.
	 */
	setExceptionBreakpoints(
		sessionId: string,
		filters: string[],
	): Promise<void>;

	/**
	 * List all breakpoints across all files for a session.
	 */
	listBreakpoints(sessionId: string): Map<string, Breakpoint[]>;

	/**
	 * Evaluate an expression in a stack frame.
	 * Returns the rendered value string.
	 */
	evaluate(
		sessionId: string,
		expression: string,
		frameIndex?: number,
		maxDepth?: number,
	): Promise<string>;

	/**
	 * Get variables for a scope.
	 * Queries DAP scopes + variables, filters by scope name and regex,
	 * renders with value renderer.
	 */
	getVariables(
		sessionId: string,
		scope?: "local" | "global" | "closure" | "all",
		frameIndex?: number,
		filter?: string,
		maxDepth?: number,
	): Promise<string>;

	/**
	 * Get the full stack trace.
	 * Returns rendered text with optional source context per frame.
	 */
	getStackTrace(
		sessionId: string,
		maxFrames?: number,
		includeSource?: boolean,
	): Promise<string>;

	/**
	 * Read source file, return numbered lines for the requested range.
	 */
	getSource(
		sessionId: string,
		file: string,
		startLine?: number,
		endLine?: number,
	): Promise<string>;

	/**
	 * Get current session status with viewport if stopped.
	 */
	getStatus(sessionId: string): Promise<{ status: SessionStatus; viewport?: string }>;

	/**
	 * Add watch expressions to the session.
	 * Returns confirmed watch list.
	 */
	addWatchExpressions(sessionId: string, expressions: string[]): string[];

	/**
	 * Get the action log.
	 * Stub for Phase 3: returns raw action list.
	 */
	getSessionLog(
		sessionId: string,
		format?: "summary" | "detailed",
	): string;

	/**
	 * Get captured output from the debugee.
	 */
	getOutput(
		sessionId: string,
		stream?: "stdout" | "stderr" | "both",
		sinceAction?: number,
	): string;

	/**
	 * Build a ViewportSnapshot from the current DAP state.
	 * Queries stack trace, scopes, variables, reads source file.
	 */
	private buildViewport(session: DebugSession): Promise<ViewportSnapshot>;

	/**
	 * Read source file from disk, using per-session cache.
	 * Returns lines array.
	 */
	private readSourceFile(session: DebugSession, filePath: string): Promise<string[]>;

	/**
	 * Map a DAP StoppedEvent reason string to our StopReason type.
	 */
	private mapStopReason(dapReason: string): StopReason;

	/**
	 * Increment action count, check limits (action count + session timeout).
	 * Throws SessionLimitError if exceeded.
	 */
	private checkAndIncrementAction(session: DebugSession, toolName: string): void;

	/**
	 * Get the session or throw a clear error.
	 */
	private getSession(sessionId: string): DebugSession;

	/**
	 * Assert the session is in one of the allowed states.
	 * Throws SessionStateError if not.
	 */
	private assertState(session: DebugSession, ...allowedStates: SessionState[]): void;

	/**
	 * Generate a unique session ID.
	 */
	private generateSessionId(): string;

	/**
	 * Clean up all active sessions (for server shutdown).
	 */
	disposeAll(): Promise<void>;
}
```

**Implementation Notes**:
- `generateSessionId()` uses `crypto.randomUUID()` truncated to 8 chars.
- `buildViewport()` is the core data assembly method. It: (1) calls `dapClient.stackTrace(threadId, 0, config.stackDepth)` to get frames, (2) for the top frame, calls `dapClient.scopes(frameId)` then `dapClient.variables(scope.variablesReference)` for the "Locals" scope, (3) reads the source file for the top frame, (4) filters out Python internals (`__builtins__`, `__doc__`, `__name__`, `__package__`, `__spec__`, `__loader__`, `__file__`, `__cached__`), (5) renders variables with the value renderer, (6) evaluates watch expressions, (7) assembles the `ViewportSnapshot`.
- Source reading uses `Bun.file(path).text()` split by newlines. The `sourceContextLines` config determines the range: `currentLine - floor(context/2)` to `currentLine + ceil(context/2)`.
- The `continue()` method: checks state is `stopped`, increments action, sends DAP continue, transitions state to `running`, calls `waitForStop()`, transitions to `stopped` or `terminated`, builds viewport, returns rendered text.
- `step()` with `count > 1`: loops `count` times, each iteration stepping then waiting for stop. Returns viewport only from the final stop.
- `runTo()`: calls `setBreakpoints` adding a temporary breakpoint, continues, on stop checks if we hit the target line, removes the temp breakpoint (restoring previous breakpoints for that file).
- Session timeout: start a `setTimeout` in `launch()`. When fired, call `stop()` on the session and set state to `error` with a timeout message.
- Output capture: register a handler for DAP `output` events. Append to `outputBuffer.stdout` or `outputBuffer.stderr` based on `event.body.category`. Track `totalBytes`. When `totalBytes > maxOutputBytes`, remove oldest entries from the front.
- All public methods that modify session state should log an `ActionLogEntry`.
- `LaunchOptions.breakpoints` is restructured from the flat roadmap format: it's an array of `{ file, breakpoints }` objects, since DAP `setBreakpoints` operates per-file.

**Acceptance Criteria**:
- [ ] `launch()` creates a session, connects to debugpy, completes DAP handshake
- [ ] `launch()` with `stopOnEntry: true` returns a viewport
- [ ] `launch()` with initial breakpoints sets them before configurationDone
- [ ] `continue()` resumes execution and returns viewport at next stop
- [ ] `step()` performs the requested step direction and returns viewport
- [ ] `step()` with `count: 3` steps 3 times and returns final viewport
- [ ] `runTo()` sets temp breakpoint, continues, removes temp breakpoint
- [ ] `setBreakpoints()` sends DAP setBreakpoints and returns verified breakpoints
- [ ] `evaluate()` evaluates expression and returns rendered value
- [ ] `getVariables()` returns filtered, rendered variables
- [ ] `getStackTrace()` returns formatted stack trace
- [ ] `getSource()` reads and returns numbered source lines
- [ ] `getOutput()` returns captured stdout/stderr
- [ ] Action limit triggers clean termination with descriptive error
- [ ] Session timeout triggers clean termination
- [ ] Concurrent session limit returns error on launch
- [ ] `stop()` sends DAP disconnect, kills process, cleans up session
- [ ] `disposeAll()` terminates all active sessions

---

### Unit 4: Value Renderer

**File**: `src/core/value-renderer.ts`

Transform DAP variable values into compact viewport strings.

```typescript
import type { DebugProtocol } from "@vscode/debugprotocol";
import type { ViewportConfig, Variable } from "./types.js";

/**
 * Configuration for rendering a single variable value.
 */
export interface RenderOptions {
	/** Current nesting depth (0 = top level). */
	depth: number;
	/** Maximum depth to render. Beyond this, show type summary only. */
	maxDepth: number;
	/** Maximum string length before truncation. */
	stringTruncateLength: number;
	/** Number of collection items to preview. */
	collectionPreviewItems: number;
}

/**
 * Python internal variable names to filter from the default locals display.
 */
export const PYTHON_INTERNAL_NAMES: ReadonlySet<string> = new Set([
	"__builtins__",
	"__doc__",
	"__name__",
	"__package__",
	"__spec__",
	"__loader__",
	"__file__",
	"__cached__",
	"__annotations__",
]);

/**
 * Generic internal variable name patterns to filter.
 * Matches names starting and ending with double underscores.
 */
export function isInternalVariable(name: string): boolean;

/**
 * Render a DAP variable into a compact string representation.
 *
 * Rendering rules:
 * - Primitives (int, float, bool, NoneType): value as-is
 * - Strings (str): quoted, truncated to stringTruncateLength with "..."
 * - Collections (list, tuple, set, dict, Array): type + length + preview
 *   e.g., `[1, 2, 3, ... (47 items)]`
 * - Objects with variablesReference > 0: `<TypeName: key=val, ...>`
 *   at depth 0, or `<TypeName>` at maxDepth
 * - None/null: "None" or "null" depending on type context
 */
export function renderDAPVariable(
	variable: DebugProtocol.Variable,
	options: RenderOptions,
): string;

/**
 * Convert a DAP variable to our Variable type for the viewport.
 * Applies filtering (removes internal variables) and rendering.
 */
export function convertDAPVariables(
	dapVariables: DebugProtocol.Variable[],
	config: ViewportConfig,
): Variable[];

/**
 * Render a string value, adding quotes and truncating.
 */
export function renderString(value: string, maxLength: number): string;

/**
 * Render a collection value with type, length, and preview items.
 * Input: the DAP variable's `value` string (e.g., "[1, 2, 3, 4, 5, ...]")
 * and its `type` (e.g., "list").
 */
export function renderCollection(
	value: string,
	type: string,
	previewItems: number,
): string;

/**
 * Render an object/class instance value.
 * Input: the DAP variable's `value` string and `type`.
 * Output: `<TypeName: key=val, key=val>` at depth < maxDepth,
 *         `<TypeName>` at maxDepth.
 */
export function renderObject(
	value: string,
	type: string,
	depth: number,
	maxDepth: number,
): string;
```

**Implementation Notes**:
- DAP variables from debugpy have `type` as a string like `"int"`, `"str"`, `"list"`, `"dict"`, `"NoneType"`, `"bool"`, `"float"`, or a class name like `"Cart"`.
- For `str` type, the `value` field is already quoted by debugpy (e.g., `"'hello world'"`). Remove outer quotes, truncate, re-add quotes.
- For `list`/`tuple`/`set` types, debugpy returns `value` like `"[1, 2, 3]"` for small lists, or `"[1, 2, 3, ...]"` for large ones. Parse and re-render with our preview limit.
- For `dict` type, debugpy returns `value` like `"{'a': 1, 'b': 2}"`. Render similarly to collections.
- `variablesReference > 0` means the variable is expandable (has children). At the viewport level, we show a compact summary; agents use `debug_evaluate` or `debug_variables` to expand.
- `convertDAPVariables()` is the main entry point called by the session manager. It filters out internal variables, then maps each through `renderDAPVariable()`.
- `isInternalVariable()` checks both the exact set (`PYTHON_INTERNAL_NAMES`) and the `__dunder__` pattern. This can be extended per-language later.

**Acceptance Criteria**:
- [ ] Integers, floats, booleans rendered as-is
- [ ] `NoneType` renders as `"None"`
- [ ] Strings are quoted and truncated with `"..."` suffix at limit
- [ ] Lists show `[1, 2, 3, ... (N items)]` with configurable preview count
- [ ] Dicts show `{key: val, ... (N items)}` with preview
- [ ] Objects show `<TypeName: key=val>` at depth 0
- [ ] Objects show `<TypeName>` at maxDepth
- [ ] Python internals (`__builtins__`, etc.) are filtered out by `convertDAPVariables()`
- [ ] Empty collections render correctly: `[] (0 items)`, `{} (0 items)`
- [ ] `renderDAPVariable()` handles missing `type` field gracefully

---

### Unit 5: MCP Tool Implementation

**File**: `src/mcp/tools/index.ts`

Register all 15 MCP tools (plus `debug_output`), each validating inputs with Zod 4 and delegating to the `SessionManager`.

```typescript
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { SessionManager } from "../../core/session-manager.js";

/**
 * Register all debug tools with the MCP server.
 * Each tool:
 * 1. Validates input with Zod schema
 * 2. Delegates to SessionManager
 * 3. Returns viewport text as MCP TextContent
 * 4. Handles errors with descriptive messages
 */
export function registerTools(server: McpServer, sessionManager: SessionManager): void;
```

The `registerTools` function registers these 15 tools (plus `debug_output` = 16 total):

**Tool 1: `debug_launch`**
```typescript
// Zod input schema
{
	command: z.string().describe(
		"Command to execute, e.g. 'python app.py' or 'python -m pytest tests/'. " +
		"The debugger will launch this command and pause at breakpoints."
	),
	language: z.enum(["python", "javascript", "typescript", "go", "rust", "java", "cpp"])
		.optional()
		.describe("Override automatic language detection based on file extension"),
	breakpoints: z.array(z.object({
		file: z.string().describe("Source file path (relative or absolute)"),
		breakpoints: z.array(z.object({
			line: z.number().describe("Line number"),
			condition: z.string().optional().describe(
				"Expression that must be true to trigger. E.g., 'discount < 0'"
			),
			hitCondition: z.string().optional().describe(
				"Break after N hits. E.g., '>=100'"
			),
			logMessage: z.string().optional().describe(
				"Log instead of breaking. Supports {expression} interpolation."
			),
		})),
	})).optional().describe(
		"Initial breakpoints to set before execution begins. " +
		"Note: breakpoints on non-executable lines (comments, blank lines, decorators) " +
		"may be adjusted by the debugger to the nearest executable line."
	),
	cwd: z.string().optional().describe("Working directory for the debug target"),
	env: z.record(z.string(), z.string()).optional()
		.describe("Additional environment variables for the debug target"),
	viewport_config: z.object({
		source_context_lines: z.number().optional(),
		stack_depth: z.number().optional(),
		locals_max_depth: z.number().optional(),
		locals_max_items: z.number().optional(),
		string_truncate_length: z.number().optional(),
		collection_preview_items: z.number().optional(),
	}).optional().describe("Override default viewport rendering parameters"),
	stop_on_entry: z.boolean().optional()
		.describe("Pause on the first executable line. Default: false"),
}
```

**Tool 2: `debug_stop`**
```typescript
{
	session_id: z.string().describe("The session to terminate"),
}
```

**Tool 3: `debug_status`**
```typescript
{
	session_id: z.string().describe("The session to query"),
}
```

**Tool 4: `debug_continue`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	timeout_ms: z.number().optional()
		.describe("Max wait time for next stop in ms. Default: 30000"),
}
```

**Tool 5: `debug_step`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	direction: z.enum(["over", "into", "out"])
		.describe("Step granularity: 'over' skips function calls, " +
			"'into' enters them, 'out' runs to parent frame"),
	count: z.number().optional()
		.describe("Number of steps to take. Default: 1. " +
			"Useful for stepping through loops without setting breakpoints."),
}
```

**Tool 6: `debug_run_to`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	file: z.string().describe("Target file path"),
	line: z.number().describe("Target line number"),
	timeout_ms: z.number().optional()
		.describe("Max wait time in ms. Default: 30000"),
}
```

**Tool 7: `debug_set_breakpoints`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	file: z.string().describe("Source file path"),
	breakpoints: z.array(z.object({
		line: z.number().describe("Line number"),
		condition: z.string().optional()
			.describe("Expression that must be true to trigger"),
		hitCondition: z.string().optional()
			.describe("Break after N hits. E.g., '>=100'"),
		logMessage: z.string().optional()
			.describe("Log instead of breaking. Supports {expression} interpolation."),
	})).describe(
		"Breakpoint definitions. REPLACES all existing breakpoints in this file. " +
		"To add a breakpoint without removing existing ones, include them all."
	),
}
```

**Tool 8: `debug_set_exception_breakpoints`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	filters: z.array(z.string()).describe(
		"Exception filter IDs. Python: 'raised' (all exceptions), " +
		"'uncaught' (unhandled only), 'userUnhandled'. " +
		"Use debug_status to see available filters for the current adapter."
	),
}
```

**Tool 9: `debug_list_breakpoints`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
}
```

**Tool 10: `debug_evaluate`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	expression: z.string().describe(
		"Expression to evaluate in the debugee's context. " +
		"E.g., 'cart.items[0].__dict__', 'len(results)', 'discount < 0'. " +
		"Can call methods and access nested attributes."
	),
	frame_index: z.number().optional().describe(
		"Stack frame context: 0 = current frame (default), " +
		"1 = caller, 2 = caller's caller, etc."
	),
	max_depth: z.number().optional().describe(
		"Object expansion depth for the result. Default: 2"
	),
}
```

**Tool 11: `debug_variables`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	scope: z.enum(["local", "global", "closure", "all"]).optional()
		.describe("Variable scope to retrieve. Default: 'local'"),
	frame_index: z.number().optional()
		.describe("Stack frame context (0 = current). Default: 0"),
	filter: z.string().optional()
		.describe("Regex filter on variable names. E.g., '^user' to show only user-prefixed vars"),
	max_depth: z.number().optional()
		.describe("Object expansion depth. Default: 1"),
}
```

**Tool 12: `debug_stack_trace`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	max_frames: z.number().optional()
		.describe("Maximum frames to return. Default: 20"),
	include_source: z.boolean().optional()
		.describe("Include source context around each frame. Default: false"),
}
```

**Tool 13: `debug_source`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	file: z.string().describe("Source file path"),
	start_line: z.number().optional()
		.describe("Start of range. Default: 1"),
	end_line: z.number().optional()
		.describe("End of range. Default: start_line + 40"),
}
```

**Tool 14: `debug_watch`** (Stub for Phase 3 — stores expressions only)
```typescript
{
	session_id: z.string().describe("The active debug session"),
	expressions: z.array(z.string()).describe(
		"Expressions to add to the watch list. " +
		"Watched expressions are evaluated and shown in every viewport. " +
		"E.g., ['len(cart.items)', 'user.tier', 'total > 0']"
	),
}
```

**Tool 15: `debug_session_log`** (Stub for Phase 3 — returns raw action list)
```typescript
{
	session_id: z.string().describe("The active debug session"),
	format: z.enum(["summary", "detailed"]).optional()
		.describe("Level of detail. Default: 'summary'"),
}
```

**Tool 16: `debug_output`**
```typescript
{
	session_id: z.string().describe("The active debug session"),
	stream: z.enum(["stdout", "stderr", "both"]).optional()
		.describe("Which output stream. Default: 'both'"),
	since_action: z.number().optional()
		.describe("Only show output captured since action N. Default: 0 (all)"),
}
```

**Implementation Notes**:
- The `registerTools` function must accept the `SessionManager` instance as a parameter. This changes the MCP server entry point — the `src/mcp/index.ts` must create a `SessionManager` and pass it.
- Each tool handler wraps the `SessionManager` call in a try/catch. On error, return `{ content: [{ type: "text", text: errorMessage }], isError: true }`.
- The `debug_launch` tool's `viewport_config` uses snake_case keys in the MCP interface (agent-facing) but maps to camelCase internally (matching `ViewportConfig` type). Convert in the handler.
- The `breakpoints` field on `debug_launch` groups breakpoints by file. The agent provides `[{ file: "order.py", breakpoints: [{ line: 147 }] }]`.
- Tool descriptions are deliberately verbose and include agent guidance. This is intentional — agents use these descriptions to decide when and how to use each tool.

**Acceptance Criteria**:
- [ ] All 16 tools are registered with the MCP server
- [ ] Each tool validates input with Zod and returns a clear error on invalid input
- [ ] `debug_launch` creates a session and returns viewport or status
- [ ] `debug_stop` terminates session and returns summary
- [ ] `debug_continue` returns viewport at next stop
- [ ] `debug_step` returns viewport after stepping
- [ ] `debug_run_to` returns viewport at target location
- [ ] `debug_set_breakpoints` returns verified breakpoints
- [ ] `debug_evaluate` returns rendered expression result
- [ ] `debug_variables` returns filtered variables
- [ ] `debug_stack_trace` returns formatted stack trace
- [ ] `debug_source` returns numbered source lines
- [ ] `debug_output` returns captured output
- [ ] `debug_watch` stores expressions and returns confirmed list
- [ ] `debug_session_log` returns action history
- [ ] Errors include descriptive messages (session not found, invalid state, etc.)

---

### Unit 6: Output Capture

**File**: Integrated into `src/core/session-manager.ts` (OutputBuffer) and `src/mcp/tools/index.ts` (debug_output tool)

Output capture is not a separate module but a cross-cutting concern handled by the session manager and exposed via the `debug_output` tool. The types and logic are defined in Unit 3 (`OutputBuffer` interface) and Unit 5 (`debug_output` tool schema).

```typescript
// Already defined in Unit 3's DebugSession interface:
export interface OutputBuffer {
	stdout: Array<{ text: string; actionNumber: number }>;
	stderr: Array<{ text: string; actionNumber: number }>;
	totalBytes: number;
}
```

**Implementation Notes**:
- During `launch()`, register a handler on the DAPClient for the `output` event:
  ```typescript
  session.dapClient.on("output", (event) => {
  	const body = (event as DebugProtocol.OutputEvent).body;
  	const category = body.category ?? "console";
  	const text = body.output;
  	const entry = { text, actionNumber: session.actionCount };

  	if (category === "stdout") {
  		session.outputBuffer.stdout.push(entry);
  	} else if (category === "stderr") {
  		session.outputBuffer.stderr.push(entry);
  	}
  	// "console" category is debugger-internal output, skip

  	session.outputBuffer.totalBytes += Buffer.byteLength(text);

  	// Truncation: keep tail when over limit
  	while (session.outputBuffer.totalBytes > session.limits.maxOutputBytes) {
  		const target = session.outputBuffer.stdout.length >= session.outputBuffer.stderr.length
  			? session.outputBuffer.stdout
  			: session.outputBuffer.stderr;
  		if (target.length === 0) break;
  		const removed = target.shift()!;
  		session.outputBuffer.totalBytes -= Buffer.byteLength(removed.text);
  	}
  });
  ```
- `getOutput()` concatenates entries from the requested stream(s), filtering by `sinceAction`.
- Output is returned as plain text with stream labels if both streams are requested:
  ```
  [stdout] Hello world
  [stdout] Processing...
  [stderr] Warning: deprecated function
  ```

**Acceptance Criteria**:
- [ ] DAP `output` events with category `stdout` are captured
- [ ] DAP `output` events with category `stderr` are captured
- [ ] `debug_output` returns captured output filtered by stream
- [ ] `since_action` parameter filters to output captured after action N
- [ ] Truncation keeps the tail (most recent output) when buffer exceeds `maxOutputBytes`
- [ ] Output buffer tracks total bytes correctly

---

### Unit 7: MCP Server Entry + Wiring

**File**: `src/mcp/index.ts`

Update the MCP server entry to create and wire the `SessionManager`.

```typescript
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { ResourceLimitsSchema } from "../core/types.js";
import { SessionManager } from "../core/session-manager.js";
import { PythonAdapter } from "../adapters/python.js";
import { registerAdapter } from "../adapters/registry.js";
import { registerTools } from "./tools/index.js";

// Register adapters
registerAdapter(new PythonAdapter());

// Create session manager with default limits
const limits = ResourceLimitsSchema.parse({});
const sessionManager = new SessionManager(limits);

// Create and configure MCP server
const server = new McpServer({
	name: "agent-lens",
	version: "0.1.0",
});

registerTools(server, sessionManager);

// Handle graceful shutdown
process.on("SIGINT", async () => {
	await sessionManager.disposeAll();
	process.exit(0);
});
process.on("SIGTERM", async () => {
	await sessionManager.disposeAll();
	process.exit(0);
});

// Start
const transport = new StdioServerTransport();
await server.connect(transport);
```

**Implementation Notes**:
- The adapter registration happens at startup. For Phase 1, only the Python adapter is registered.
- Signal handlers ensure all debug sessions are cleaned up on server shutdown (SIGINT from Ctrl+C, SIGTERM from process managers).
- Resource limits use Zod defaults via `ResourceLimitsSchema.parse({})`.

**Acceptance Criteria**:
- [ ] MCP server starts and accepts connections over stdio
- [ ] Python adapter is registered on startup
- [ ] SessionManager is created with default resource limits
- [ ] All 16 tools are available via MCP tool listing
- [ ] SIGINT/SIGTERM trigger graceful session cleanup

---

### Unit 8: Error Types

**File**: `src/core/errors.ts` (new file)

Define structured error types used across the codebase.

```typescript
/**
 * Base error for all Agent Lens errors.
 */
export class AgentLensError extends Error {
	constructor(
		message: string,
		public readonly code: string,
	) {
		super(message);
		this.name = "AgentLensError";
	}
}

/**
 * DAP request timed out.
 */
export class DAPTimeoutError extends AgentLensError {
	constructor(
		public readonly command: string,
		public readonly timeoutMs: number,
	) {
		super(
			`DAP request '${command}' timed out after ${timeoutMs}ms`,
			"DAP_TIMEOUT",
		);
		this.name = "DAPTimeoutError";
	}
}

/**
 * DAP client has been disposed.
 */
export class DAPClientDisposedError extends AgentLensError {
	constructor() {
		super("DAP client has been disposed", "DAP_DISPOSED");
		this.name = "DAPClientDisposedError";
	}
}

/**
 * DAP connection failed.
 */
export class DAPConnectionError extends AgentLensError {
	constructor(
		public readonly host: string,
		public readonly port: number,
		public readonly cause?: Error,
	) {
		super(
			`Failed to connect to DAP server at ${host}:${port}: ${cause?.message ?? "unknown error"}`,
			"DAP_CONNECTION_FAILED",
		);
		this.name = "DAPConnectionError";
	}
}

/**
 * Session not found.
 */
export class SessionNotFoundError extends AgentLensError {
	constructor(public readonly sessionId: string) {
		super(`No debug session with id: ${sessionId}`, "SESSION_NOT_FOUND");
		this.name = "SessionNotFoundError";
	}
}

/**
 * Session is in an invalid state for the requested operation.
 */
export class SessionStateError extends AgentLensError {
	constructor(
		public readonly sessionId: string,
		public readonly currentState: string,
		public readonly expectedStates: string[],
	) {
		super(
			`Session ${sessionId} is '${currentState}', ` +
			`expected one of: ${expectedStates.join(", ")}`,
			"SESSION_INVALID_STATE",
		);
		this.name = "SessionStateError";
	}
}

/**
 * Session resource limit exceeded.
 */
export class SessionLimitError extends AgentLensError {
	constructor(
		public readonly limitName: string,
		public readonly currentValue: number,
		public readonly maxValue: number,
		public readonly suggestion?: string,
	) {
		super(
			`Session limit '${limitName}' exceeded: ${currentValue}/${maxValue}. ` +
			(suggestion ?? ""),
			"SESSION_LIMIT_EXCEEDED",
		);
		this.name = "SessionLimitError";
	}
}

/**
 * Adapter prerequisites not met.
 */
export class AdapterPrerequisiteError extends AgentLensError {
	constructor(
		public readonly adapterId: string,
		public readonly missing: string[],
		public readonly installHint?: string,
	) {
		super(
			`Adapter '${adapterId}' prerequisites not met: ${missing.join(", ")}. ` +
			(installHint ? `Install: ${installHint}` : ""),
			"ADAPTER_PREREQUISITES",
		);
		this.name = "AdapterPrerequisiteError";
	}
}

/**
 * No adapter found for the given language or file extension.
 */
export class AdapterNotFoundError extends AgentLensError {
	constructor(public readonly languageOrExt: string) {
		super(
			`No debug adapter found for '${languageOrExt}'. ` +
			"Available adapters can be checked with debug_status.",
			"ADAPTER_NOT_FOUND",
		);
		this.name = "AdapterNotFoundError";
	}
}

/**
 * Debugee process launch failed.
 */
export class LaunchError extends AgentLensError {
	constructor(
		message: string,
		public readonly stderr?: string,
	) {
		super(message, "LAUNCH_FAILED");
		this.name = "LaunchError";
	}
}
```

**Implementation Notes**:
- All errors extend `AgentLensError` which has a `code` field. This allows MCP tool handlers to pattern-match on error codes for structured responses.
- Error messages are written to be useful to agents — they explain what happened and suggest remediation.
- `SessionLimitError` includes a `suggestion` field (e.g., "Consider using conditional breakpoints to reduce step count").

**Acceptance Criteria**:
- [ ] All error types extend `AgentLensError`
- [ ] Each error type has a unique `code` string
- [ ] Error messages are descriptive and include remediation suggestions
- [ ] Errors serialize properly (no circular references, `message` is readable)

---

### Unit 9: Test Fixtures

**File**: `tests/fixtures/python/function-calls.py` (new), `tests/fixtures/python/exception-raising.py` (new), `tests/fixtures/python/class-state.py` (new)

Add the remaining Python test fixtures specified in TESTING.md.

```python
# tests/fixtures/python/function-calls.py
"""Call stack testing: step into/out, nested function calls."""

def add(a: int, b: int) -> int:
    result = a + b
    return result

def multiply(a: int, b: int) -> int:
    total = 0
    for _ in range(b):
        total = add(total, a)
    return total

def calculate(x: int, y: int, z: int) -> int:
    step1 = multiply(x, y)
    step2 = add(step1, z)
    return step2

if __name__ == "__main__":
    result = calculate(3, 4, 5)
    print(f"Result: {result}")
```

```python
# tests/fixtures/python/exception-raising.py
"""Exception breakpoint testing."""

class InsufficientFundsError(Exception):
    def __init__(self, balance: float, amount: float):
        self.balance = balance
        self.amount = amount
        super().__init__(f"Cannot withdraw {amount}, balance is {balance}")

def withdraw(balance: float, amount: float) -> float:
    if amount > balance:
        raise InsufficientFundsError(balance, amount)
    return balance - amount

def process_withdrawals(balance: float, amounts: list[float]) -> float:
    for amount in amounts:
        balance = withdraw(balance, amount)
    return balance

if __name__ == "__main__":
    try:
        final = process_withdrawals(100.0, [30.0, 50.0, 40.0])
        print(f"Final balance: {final}")
    except InsufficientFundsError as e:
        print(f"Error: {e}")
```

```python
# tests/fixtures/python/class-state.py
"""Object inspection and nested attribute testing."""

class Address:
    def __init__(self, street: str, city: str, zip_code: str):
        self.street = street
        self.city = city
        self.zip_code = zip_code

class User:
    def __init__(self, name: str, email: str, address: Address):
        self.name = name
        self.email = email
        self.address = address
        self.orders: list[dict] = []

    def add_order(self, item: str, price: float) -> None:
        self.orders.append({"item": item, "price": price})

def main():
    addr = Address("123 Main St", "Springfield", "62701")
    user = User("Alice", "alice@example.com", addr)
    user.add_order("Widget", 29.99)
    user.add_order("Gadget", 49.99)
    total = sum(o["price"] for o in user.orders)
    print(f"{user.name}'s total: {total}")

if __name__ == "__main__":
    main()
```

**Acceptance Criteria**:
- [ ] `function-calls.py` runs successfully and exercises nested function calls
- [ ] `exception-raising.py` raises `InsufficientFundsError` on the third withdrawal
- [ ] `class-state.py` creates nested objects (User -> Address, User -> orders list)
- [ ] All fixtures are deterministic — same values on every run

---

## Implementation Order

The implementation order resolves dependencies — each unit builds on the previous:

1. **Unit 8: Error Types** (`src/core/errors.ts`) — No dependencies. Used by all other units.
2. **Unit 1: DAP Client Hardening** (`src/core/dap-client.ts`) — Depends on error types. Required by session manager.
3. **Unit 4: Value Renderer** (`src/core/value-renderer.ts`) — No runtime dependencies. Required by session manager for viewport construction.
4. **Unit 2: Python Adapter** (`src/adapters/python.ts`) — Depends on error types. Required by session manager for launching debug sessions.
5. **Unit 9: Test Fixtures** (`tests/fixtures/python/`) — No code dependencies. Needed for integration tests of units 3+.
6. **Unit 3: Session Manager** (`src/core/session-manager.ts`) — Depends on DAP client, value renderer, python adapter, viewport renderer (existing), adapter registry (existing). This is the central orchestrator.
7. **Unit 5: MCP Tool Implementation** (`src/mcp/tools/index.ts`) — Depends on session manager. Thin wiring layer.
8. **Unit 6: Output Capture** — Integrated into units 3 and 5. Implemented as part of session manager's launch flow.
9. **Unit 7: MCP Server Entry** (`src/mcp/index.ts`) — Depends on all above. Wires everything together.

---

## Testing

### Unit Tests: `tests/unit/core/dap-client.test.ts`

Test the DAP client with an in-memory mock DAP server using Node.js `PassThrough` streams.

```typescript
// Mock DAP server that writes responses to a PassThrough stream
// Test cases:
// - send() resolves with correct response for matching request_seq
// - send() rejects after requestTimeoutMs with DAPTimeoutError
// - send() handles error responses (success: false)
// - initialize() completes handshake (send init, receive capabilities, receive initialized event)
// - waitForStop() resolves on stopped event
// - waitForStop() resolves on terminated event
// - waitForStop() rejects on timeout
// - Multiple concurrent send() calls resolve independently
// - Malformed DAP messages are skipped without crashing
// - dispose() rejects all pending requests
// - Event handlers are called for matching events
// - off() removes event handlers
```

### Unit Tests: `tests/unit/core/value-renderer.test.ts`

```typescript
// Test cases:
// - renderDAPVariable: int renders as-is ("42")
// - renderDAPVariable: float renders as-is ("3.14")
// - renderDAPVariable: bool renders as-is ("True")
// - renderDAPVariable: NoneType renders as "None"
// - renderDAPVariable: str renders quoted ('"hello"')
// - renderDAPVariable: str truncated at limit with "..."
// - renderDAPVariable: list with preview ('[1, 2, 3, ... (47 items)]')
// - renderDAPVariable: empty list ('[] (0 items)')
// - renderDAPVariable: dict with preview ("{'a': 1, ... (5 items)}")
// - renderDAPVariable: object at depth 0 ('<User: name="Alice">')
// - renderDAPVariable: object at maxDepth ('<User>')
// - convertDAPVariables: filters out __builtins__, __doc__, etc.
// - convertDAPVariables: keeps regular variables
// - isInternalVariable: matches __dunder__ names
// - isInternalVariable: does not match regular names
```

### Unit Tests: `tests/unit/core/session-manager.test.ts`

```typescript
// Test cases using mocked DAP client and adapter:
// - launch() creates session with correct state
// - launch() rejects when max concurrent sessions reached
// - getSession() throws SessionNotFoundError for bad ID
// - assertState() throws SessionStateError for wrong state
// - checkAndIncrementAction() throws SessionLimitError at max actions
// - mapStopReason() maps DAP reason strings correctly
// - generateSessionId() produces 8-char unique IDs
// - buildViewport() assembles ViewportSnapshot from DAP state (mocked)
```

### Unit Tests: `tests/unit/adapters/python.test.ts`

```typescript
// Test cases:
// - parseCommand("python app.py") => { script: "app.py", args: [] }
// - parseCommand("python3 app.py --verbose") => { script: "app.py", args: ["--verbose"] }
// - parseCommand("python -m pytest tests/") => { script: "-m", args: ["pytest", "tests/"] }
// - parseCommand("app.py") => { script: "app.py", args: [] }
// - allocatePort() returns a number > 0
```

### Integration Tests: `tests/integration/dap-client.test.ts`

Real DAP session against debugpy.

```typescript
// Prerequisites: python3, debugpy installed (skip if missing)
// Test cases:
// - Connect to debugpy, run initialize handshake
// - Set breakpoint, launch script, receive stopped event
// - Get stack trace at breakpoint
// - Get scopes and variables at breakpoint
// - Evaluate expression at breakpoint
// - Step over, receive stopped event
// - Continue to end, receive terminated event
// - Request timeout produces DAPTimeoutError
```

### Integration Tests: `tests/integration/adapters/python.test.ts`

Full adapter lifecycle.

```typescript
// Prerequisites: python3, debugpy installed (skip if missing)
// Test cases:
// - checkPrerequisites() returns satisfied: true
// - launch() spawns debugpy and returns working DAPConnection
// - DAPConnection can send/receive DAP messages
// - dispose() kills the child process
// - Launch with bad script path produces clear error
// - Launch with missing debugpy returns prerequisite error
```

### Integration Tests: `tests/integration/session-lifecycle.test.ts`

Full session manager lifecycle with real debugpy.

```typescript
// Prerequisites: python3, debugpy installed (skip if missing)
// Test cases:
// - launch → breakpoint → step → evaluate → stop sequence
// - launch with stopOnEntry returns viewport at first line
// - continue to breakpoint returns viewport with correct locals
// - step over changes current line
// - evaluate expression returns correct value
// - session timeout triggers clean termination
// - action limit triggers clean termination
// - concurrent session limit enforced
// - output capture captures debugee stdout
```

### E2E Tests: `tests/e2e/mcp/discount-bug.test.ts`

Full MCP path for the canonical discount bug scenario.

```typescript
// Prerequisites: python3, debugpy installed (skip if missing)
// Uses @modelcontextprotocol/sdk Client to call tools
// Test sequence:
// 1. debug_launch("python tests/fixtures/python/discount-bug.py",
//    breakpoints: [{ file: "discount-bug.py", breakpoints: [{ line: 22 }] }])
// 2. debug_continue() → viewport shows stopped at line 22, discount = -149.97
// 3. debug_evaluate("calculate_discount('gold', 149.97)") → returns 149.97
// 4. debug_evaluate("tier_multipliers") → shows gold: 1.0
// 5. debug_stop() → confirms session ended
// Assertions: viewport contains expected values at each step
```

### E2E Tests: `tests/e2e/mcp/step-and-inspect.test.ts`

```typescript
// Test stepping through simple-loop.py:
// 1. debug_launch with breakpoint at line 6 (inside loop)
// 2. debug_continue → stopped at line 6, total=0, i=0
// 3. debug_step(over) → stopped at line 7, total=0
// 4. debug_step(over) → stopped at line 6, total=0, i=1
// 5. debug_variables() → shows total and i
// 6. debug_stack_trace() → shows sum_range frame
// 7. debug_stop()
```

### E2E Tests: `tests/e2e/mcp/exception-tracing.test.ts`

```typescript
// Test exception breakpoints with exception-raising.py:
// 1. debug_launch with exception breakpoint filter "raised"
// 2. debug_continue → stopped on InsufficientFundsError
// 3. Viewport shows exception details
// 4. debug_variables() → shows balance=20.0, amount=40.0
// 5. debug_stop()
```

### E2E Tests: `tests/e2e/mcp/session-limits.test.ts`

```typescript
// Test resource limit enforcement:
// 1. debug_launch with small action limit (max_actions_per_session override)
// 2. Step repeatedly until limit hit
// 3. Verify session terminates with clear limit error
// 4. Verify session cleanup occurred
```

### E2E Tests: `tests/e2e/mcp/output-capture.test.ts`

```typescript
// Test output capture:
// 1. debug_launch("python tests/fixtures/python/simple-loop.py", stopOnEntry: true)
// 2. debug_continue → runs to end, prints output
// 3. debug_output(stream: "stdout") → contains "Sum: 45"
// 4. debug_stop()
```

### Test Helper: `tests/helpers/mcp-test-client.ts`

```typescript
import { Client } from "@modelcontextprotocol/sdk/client/index.js";

/**
 * Create an MCP client connected to the agent-lens server via stdio.
 * Spawns the server as a child process for e2e testing.
 */
export async function createTestClient(): Promise<{
	client: Client;
	cleanup: () => Promise<void>;
}>;

/**
 * Call an MCP tool and return the text content.
 * Throws if the tool returns an error.
 */
export async function callTool(
	client: Client,
	name: string,
	args: Record<string, unknown>,
): Promise<string>;
```

### Test Helper: `tests/helpers/debugpy-check.ts`

```typescript
/**
 * Check if debugpy is available. Used with vitest's describe.skipIf.
 */
export async function isDebugpyAvailable(): Promise<boolean>;

/**
 * Skip the current test suite if debugpy is not available.
 * Usage: describe.skipIf(!await isDebugpyAvailable())("...", () => { ... })
 */
export const SKIP_NO_DEBUGPY: boolean;
```

---

## Verification Checklist

```bash
# 1. Lint passes
bun run lint

# 2. Unit tests pass (fast, no debugger needed)
bun run test:unit

# 3. Integration tests pass (requires python3 + debugpy)
bun run test:integration

# 4. E2E tests pass (requires python3 + debugpy)
bun run test:e2e

# 5. All tests pass
bun run test

# 6. MCP server starts without errors
echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}},"id":1}' | bun run mcp

# 7. TypeScript compilation check
bunx tsc --noEmit
```
