# Design: Phase 5 — Advanced Debugging

## Overview

Phase 5 adds advanced debugging features that make the agent significantly more capable: conditional breakpoints with verification, exception breakpoints with viewport enrichment, logpoints, attach mode, and multi-threaded debugging. These build on the stable core from Phases 1–4.

**Key constraints:**
- Not all debuggers support all features. DAP capability negotiation determines what's available.
- The agent interface must remain simple — no DAP internals leak through.
- All features work across Python (debugpy), Node.js (js-debug), and Go (Delve).

---

## Implementation Units

### Unit 1: Breakpoint Verification & Capability Reporting

**File**: `src/core/session-manager.ts` (modify `setBreakpoints`)

The `setBreakpoints` method already passes `condition`, `hitCondition`, and `logMessage` to DAP via `toSourceBreakpoints()`. However, the response handling doesn't report DAP's verification feedback to the caller in a structured way.

```typescript
// New return type for setBreakpoints (replaces raw DebugProtocol.Breakpoint[])
export interface VerifiedBreakpoint {
	/** Requested line */
	requestedLine: number;
	/** Actual line the debugger set the breakpoint on (may differ) */
	verifiedLine: number | null;
	/** Whether the debugger accepted the breakpoint */
	verified: boolean;
	/** Debugger message (e.g., "adjusted to nearest executable line") */
	message?: string;
	/** Whether the condition was accepted (null if no condition) */
	conditionAccepted?: boolean;
}
```

**Changes to `SessionManager.setBreakpoints`:**

```typescript
async setBreakpoints(
	sessionId: string,
	file: string,
	breakpoints: Breakpoint[],
): Promise<VerifiedBreakpoint[]> {
	const session = this.getSession(sessionId);
	const absFile = resolve(process.cwd(), file);

	const response = await session.dapClient.setBreakpoints(
		{ path: absFile, name: file },
		toSourceBreakpoints(breakpoints),
	);

	session.breakpointMap.set(absFile, breakpoints);
	this.logAction(session, "debug_set_breakpoints", `Set ${breakpoints.length} breakpoints in ${file}`);

	const verified = response.body?.breakpoints ?? [];
	return breakpoints.map((bp, i) => {
		const v = verified[i];
		return {
			requestedLine: bp.line,
			verifiedLine: v?.line ?? null,
			verified: v?.verified ?? false,
			message: v?.message,
			conditionAccepted: bp.condition ? v?.verified ?? false : undefined,
		};
	});
}
```

**File**: `src/mcp/tools/index.ts` (update `debug_set_breakpoints` tool response)

Update the response to include richer verification info:

```typescript
const lines = verified.map((bp) => {
	const parts = [`Line ${bp.requestedLine}`];
	if (bp.verifiedLine !== null && bp.verifiedLine !== bp.requestedLine) {
		parts.push(`→ adjusted to line ${bp.verifiedLine}`);
	}
	parts.push(bp.verified ? "verified" : "UNVERIFIED");
	if (bp.message) parts.push(`(${bp.message})`);
	if (bp.conditionAccepted === false) parts.push("WARNING: condition may not be supported");
	return `  ${parts.join(" ")}`;
}).join("\n");
```

**File**: `src/daemon/protocol.ts` (update `BreakpointsResultPayload`)

```typescript
export interface BreakpointsResultPayload {
	breakpoints: VerifiedBreakpoint[];
}
```

**Implementation Notes:**
- debugpy, js-debug, and Delve all support `condition` and `hitCondition` on `SourceBreakpoint`
- debugpy supports `logMessage` natively; js-debug supports it; Delve does NOT support logpoints (DAP `setBreakpoints` will accept the field but ignore it)
- When a condition is syntactically invalid, debugpy sets `verified: false` with a `message` explaining why
- js-debug sets `verified: true` even for bad conditions (error surfaces at runtime)
- The `conditionAccepted` field helps agents understand when to distrust a "verified" response

**Acceptance Criteria:**
- [ ] `setBreakpoints` returns `VerifiedBreakpoint[]` with `requestedLine`, `verifiedLine`, `verified`, `message`, `conditionAccepted`
- [ ] MCP tool response includes adjusted line info and condition warnings
- [ ] Daemon protocol updated to match
- [ ] Existing tests still pass

---

### Unit 2: Exception Breakpoints with Viewport Enrichment

**File**: `src/core/types.ts` (add exception info to `ViewportSnapshot`)

```typescript
export interface ExceptionInfo {
	/** Exception type name, e.g. "ValueError", "TypeError" */
	type: string;
	/** Exception message */
	message: string;
	/** Exception ID from DAP (for drill-down) */
	exceptionId?: string;
}

// Add to ViewportSnapshot:
export interface ViewportSnapshot {
	// ... existing fields ...
	/** Exception info when stopped on exception */
	exception?: ExceptionInfo;
}
```

**File**: `src/core/session-manager.ts` (modify `handleStopResult` and `buildViewport`)

When the stop reason is `"exception"`, query DAP for exception details:

```typescript
// In handleStopResult, after setting session.state = "stopped":
if (reason === "exception") {
	try {
		const threadId = this.getThreadId(session);
		const exInfo = await session.dapClient.exceptionInfo(threadId);
		session.lastExceptionInfo = {
			type: exInfo.body.exceptionId ?? "Unknown",
			message: exInfo.body.description ?? exInfo.body.exceptionId ?? "",
			exceptionId: exInfo.body.exceptionId,
		};
	} catch {
		session.lastExceptionInfo = null;
	}
}

// In buildViewport, add exception info to snapshot:
if (session.lastExceptionInfo && snapshot.reason === "exception") {
	snapshot.exception = session.lastExceptionInfo;
}
```

**File**: `src/core/dap-client.ts` (add `exceptionInfo` method)

```typescript
/** DAP exceptionInfo — get details about the exception that caused a stop. */
exceptionInfo(threadId: number): Promise<DebugProtocol.ExceptionInfoResponse> {
	return this.send("exceptionInfo", { threadId });
}
```

**File**: `src/core/viewport.ts` (render exception in viewport header)

```typescript
// In renderViewport, after the Reason line:
if (snapshot.exception) {
	lines.push(`Exception: ${snapshot.exception.type}: ${snapshot.exception.message}`);
}
```

**File**: `src/core/session-manager.ts` (add `lastExceptionInfo` to `DebugSession`)

```typescript
export interface DebugSession {
	// ... existing fields ...
	/** Last exception info (populated when stopped on exception) */
	lastExceptionInfo: ExceptionInfo | null;
}
```

Initialize as `null` in `launch()`.

**File**: `src/core/session-manager.ts` (add `getExceptionBreakpointFilters`)

Expose the available exception filters from DAP capabilities so agents know what's available:

```typescript
/**
 * Get the available exception breakpoint filters for a session.
 * Reads from DAP capabilities negotiated during initialize.
 */
getExceptionBreakpointFilters(sessionId: string): Array<{ filter: string; label: string; default?: boolean }> {
	const session = this.getSession(sessionId);
	const filters = session.dapClient.capabilities.exceptionBreakpointFilters ?? [];
	return filters.map((f) => ({
		filter: f.filter,
		label: f.label,
		default: f.default,
	}));
}
```

**File**: `src/mcp/tools/index.ts` (update `debug_set_exception_breakpoints` response)

```typescript
async ({ session_id, filters }) => {
	try {
		await sessionManager.setExceptionBreakpoints(session_id, filters);
		const available = sessionManager.getExceptionBreakpointFilters(session_id);
		const filterList = available.map((f) => `  ${f.filter}: ${f.label}`).join("\n");
		return {
			content: [{
				type: "text" as const,
				text: `Exception breakpoints set: ${filters.join(", ")}\n\nAvailable filters:\n${filterList}`,
			}],
		};
	} catch (err) {
		return errorResponse(err);
	}
},
```

Also update the tool description to include per-adapter filter info:

```typescript
"Configure exception breakpoint filters. Controls which exceptions pause execution. " +
"Python filters: 'raised' (all exceptions), 'uncaught' (unhandled only), 'userUnhandled'. " +
"Node.js filters: 'all' (all exceptions), 'uncaught' (unhandled only). " +
"Go/Delve: 'panic' (runtime panics). " +
"Use debug_status after launch to see exact available filters for the current adapter."
```

**Implementation Notes:**
- `exceptionInfo` request is gated by `supportsExceptionInfoRequest` capability — check before calling
- debugpy returns rich exception info with `exceptionId` = exception class name, `description` = message
- js-debug returns similar structure
- Delve has limited exception info support — panics show up as "runtime error" with a description
- If `exceptionInfo` is not supported, fall back to showing just "Exception" in the viewport header

**Acceptance Criteria:**
- [ ] When stopped on exception, viewport header includes `Exception: <type>: <message>`
- [ ] `ExceptionInfo` type added to `ViewportSnapshot`
- [ ] `DAPClient.exceptionInfo()` method added
- [ ] `getExceptionBreakpointFilters()` exposes available filters from capabilities
- [ ] Tool description lists per-adapter filter IDs
- [ ] Falls back gracefully when `supportsExceptionInfoRequest` is false

---

### Unit 3: Logpoint Handling

Logpoints (`logMessage` on breakpoints) are already passed to DAP by `toSourceBreakpoints()`. However, the agent doesn't know whether the adapter actually supports logpoints, and logpoint output needs to be correctly captured.

**File**: `src/core/session-manager.ts` (modify `setBreakpoints` verification)

The `VerifiedBreakpoint` from Unit 1 already reports whether the breakpoint was accepted. For logpoints specifically, add a note when the adapter doesn't support them.

**File**: `src/mcp/tools/index.ts` (update `debug_set_breakpoints` tool description)

```typescript
"Set breakpoints in a source file. REPLACES all existing breakpoints in that file. " +
"Supports conditions ('discount < 0'), hit counts ('>=100'), and logpoints ('discount={discount}'). " +
"Logpoints log a message when hit instead of breaking. Not all debuggers support logpoints — " +
"if unsupported, the breakpoint will be set as a regular breakpoint. " +
"Note: breakpoints on non-executable lines may be adjusted by the debugger."
```

**Implementation Notes:**
- Logpoints in debugpy: fully supported, output appears as `stdout` DAP output events
- Logpoints in js-debug: fully supported via `logMessage`
- Logpoints in Delve: NOT supported. DAP accepts the field but ignores `logMessage`. The breakpoint acts as a regular breakpoint.
- No code change needed for capture — logpoint output flows through DAP `output` events which are already captured by the session's output event handler in `launch()`
- The key enhancement is agent-facing: clear tool descriptions about logpoint support limitations

**Acceptance Criteria:**
- [ ] Tool description documents logpoint syntax and adapter support limitations
- [ ] Logpoint output captured via existing `output` event handler (no code change needed)
- [ ] `VerifiedBreakpoint` response from Unit 1 flags unsupported logpoints

---

### Unit 4: Attach Mode

**File**: `src/core/session-manager.ts` (add `attach` method)

```typescript
export interface AttachOptions {
	/** Attach by process ID */
	pid?: number;
	/** Attach by port (connect to debug server) */
	port?: number;
	/** Host for port-based attach. Default: "127.0.0.1" */
	host?: string;
	/** Override language detection */
	language: string;
	/** Working directory */
	cwd?: string;
	/** Initial breakpoints */
	breakpoints?: Array<{ file: string; breakpoints: Breakpoint[] }>;
	/** Viewport configuration */
	viewportConfig?: Partial<ViewportConfig>;
}

async attach(options: AttachOptions): Promise<LaunchResult> {
	// 1. Check concurrent session limit
	if (this.sessions.size >= this.limits.maxConcurrentSessions) {
		throw new SessionLimitError(
			"maxConcurrentSessions",
			this.sessions.size,
			this.limits.maxConcurrentSessions,
			"Stop an existing session before attaching a new one.",
		);
	}

	// 2. Resolve adapter — language is required for attach (no command to infer from)
	const adapter = getAdapter(options.language);
	if (!adapter) throw new AdapterNotFoundError(options.language);

	const prereqs = await adapter.checkPrerequisites();
	if (!prereqs.satisfied) {
		throw new AdapterPrerequisiteError(adapter.id, prereqs.missing ?? [], prereqs.installHint);
	}

	// 3. Call adapter.attach()
	const connection = await adapter.attach({
		pid: options.pid,
		port: options.port,
		host: options.host,
	});

	// 4. Create DAPClient, attach streams
	const dapClient = new DAPClient({
		requestTimeoutMs: 10_000,
		stopTimeoutMs: this.limits.stepTimeoutMs,
	});
	dapClient.attachStreams(connection.reader, connection.writer);

	// 5. Build DAP attach arguments from adapter's launchArgs
	const dapFlow = (connection.launchArgs?._dapFlow as string | undefined) ?? "standard";
	const { _dapFlow: _ignored, ...adapterAttachArgs } = connection.launchArgs ?? {};
	const dapAttachArgs: Record<string, unknown> = {
		...adapterAttachArgs,
	};

	// Register `initialized` listener before initialize
	const initializedPromise = new Promise<void>((resolve) => {
		const handler = () => {
			dapClient.off("initialized", handler);
			resolve();
		};
		dapClient.on("initialized", handler);
	});

	// 6. Initialize
	await dapClient.initialize();

	const viewportConfig = ViewportConfigSchema.parse(options.viewportConfig ?? {});
	const explicitViewportFields = new Set<string>(Object.keys(options.viewportConfig ?? {}));
	const breakpointMap = new Map<string, Breakpoint[]>();

	if (dapFlow === "launch-first") {
		// Send attach first, wait for initialized
		const attachPromise = dapClient.send("attach", dapAttachArgs);
		await initializedPromise;

		if (options.breakpoints) {
			for (const { file, breakpoints } of options.breakpoints) {
				const absFile = resolve(options.cwd ?? process.cwd(), file);
				await dapClient.setBreakpoints(
					{ path: absFile, name: file },
					toSourceBreakpoints(breakpoints),
				);
				breakpointMap.set(absFile, breakpoints);
			}
		}

		await dapClient.configurationDone();
		await attachPromise;
	} else {
		await initializedPromise;

		if (options.breakpoints) {
			for (const { file, breakpoints } of options.breakpoints) {
				const absFile = resolve(options.cwd ?? process.cwd(), file);
				await dapClient.setBreakpoints(
					{ path: absFile, name: file },
					toSourceBreakpoints(breakpoints),
				);
				breakpointMap.set(absFile, breakpoints);
			}
		}

		await dapClient.configurationDone();
		await dapClient.send("attach", dapAttachArgs);
	}

	// 7. Create session
	const sessionId = this.generateSessionId();
	const now = Date.now();

	const session: DebugSession = {
		id: sessionId,
		state: "running",
		language: adapter.id,
		adapter,
		dapClient,
		connection,
		viewportConfig,
		limits: this.limits,
		startedAt: now,
		actionCount: 0,
		lastStoppedThreadId: null,
		lastStoppedFrameId: null,
		breakpointMap,
		watchExpressions: [],
		outputBuffer: { stdout: [], stderr: [], totalBytes: 0 },
		actionLog: [],
		sourceCache: new Map(),
		timeoutTimer: setTimeout(() => {}, 0),
		previousSnapshot: null,
		diffMode: false,
		tokenStats: { viewportTokensConsumed: 0, viewportCount: 0 },
		explicitViewportFields,
		pendingStopPromise: null,
		lastExceptionInfo: null,
	};

	// Session timeout
	session.timeoutTimer = setTimeout(async () => {
		try {
			session.state = "error";
			await this.stop(sessionId);
		} catch { /* ignore */ }
	}, this.limits.sessionTimeoutMs);

	// Register output event handler
	dapClient.on("output", (event) => {
		const body = (event as DebugProtocol.OutputEvent).body;
		const category = body.category ?? "console";
		const text = body.output;
		const entry = { text, actionNumber: session.actionCount };
		if (category === "stdout") session.outputBuffer.stdout.push(entry);
		else if (category === "stderr") session.outputBuffer.stderr.push(entry);
		session.outputBuffer.totalBytes += Buffer.byteLength(text);
		while (session.outputBuffer.totalBytes > session.limits.maxOutputBytes) {
			const target = session.outputBuffer.stdout.length >= session.outputBuffer.stderr.length
				? session.outputBuffer.stdout : session.outputBuffer.stderr;
			if (target.length === 0) break;
			const removed = target.shift();
			if (removed) session.outputBuffer.totalBytes -= Buffer.byteLength(removed.text);
		}
	});

	this.sessions.set(sessionId, session);
	this.logAction(session, "debug_attach", `Attached to ${options.language} process`);

	return {
		sessionId,
		status: session.state as SessionStatus,
	};
}
```

**File**: `src/core/dap-client.ts` (add `attach` method)

```typescript
/** DAP attach — attach to a running debugee. */
attach(args: DebugProtocol.AttachRequestArguments): Promise<DebugProtocol.AttachResponse> {
	return this.send("attach", args as Record<string, unknown>);
}
```

**File**: `src/mcp/tools/index.ts` (add `debug_attach` tool)

```typescript
// Tool 17: debug_attach
server.tool(
	"debug_attach",
	"Attach to an already-running process for debugging. " +
		"Use when the target is a long-running service (web server, daemon) " +
		"or when you need to debug a process you didn't launch. " +
		"Python: the process must be running with debugpy listening (python -m debugpy --listen 5678 app.py). " +
		"Node.js: the process must be running with --inspect (node --inspect app.js). " +
		"Go: Delve will attach to the process by PID.",
	{
		language: z.enum(["python", "javascript", "typescript", "go"])
			.describe("Language of the target process. Required for attach (no command to infer from)."),
		pid: z.number().optional()
			.describe("Process ID to attach to. Use for Go (Delve attaches by PID)."),
		port: z.number().optional()
			.describe("Debug server port. Python debugpy default: 5678. Node.js inspector default: 9229."),
		host: z.string().optional()
			.describe("Debug server host. Default: '127.0.0.1'"),
		cwd: z.string().optional()
			.describe("Working directory for source file resolution"),
		breakpoints: z.array(
			z.object({
				file: z.string().describe("Source file path"),
				breakpoints: z.array(
					z.object({
						line: z.number().describe("Line number"),
						condition: z.string().optional(),
						hitCondition: z.string().optional(),
						logMessage: z.string().optional(),
					}),
				),
			}),
		).optional().describe("Breakpoints to set after attaching"),
		viewport_config: z.object({
			source_context_lines: z.number().optional(),
			stack_depth: z.number().optional(),
			locals_max_depth: z.number().optional(),
			locals_max_items: z.number().optional(),
			string_truncate_length: z.number().optional(),
			collection_preview_items: z.number().optional(),
		}).optional().describe("Override default viewport rendering parameters"),
	},
	async ({ language, pid, port, host, cwd, breakpoints, viewport_config }) => {
		try {
			const viewportConfig = viewport_config
				? {
					sourceContextLines: viewport_config.source_context_lines,
					stackDepth: viewport_config.stack_depth,
					localsMaxDepth: viewport_config.locals_max_depth,
					localsMaxItems: viewport_config.locals_max_items,
					stringTruncateLength: viewport_config.string_truncate_length,
					collectionPreviewItems: viewport_config.collection_preview_items,
				}
				: undefined;

			const result = await sessionManager.attach({
				language,
				pid,
				port,
				host,
				cwd,
				breakpoints,
				viewportConfig,
			});

			return {
				content: [{
					type: "text" as const,
					text: `Session: ${result.sessionId}\nStatus: ${result.status}\nAttached to ${language} process.`,
				}],
			};
		} catch (err) {
			return errorResponse(err);
		}
	},
);
```

**File**: `src/daemon/protocol.ts` (add `session.attach` method)

```typescript
// Add to RpcMethods:
"session.attach": { params: AttachParams; result: LaunchResultPayload };

// Add schema:
export const AttachParamsSchema = z.object({
	language: z.string(),
	pid: z.number().optional(),
	port: z.number().optional(),
	host: z.string().optional(),
	cwd: z.string().optional(),
	breakpoints: z.array(
		z.object({
			file: z.string(),
			breakpoints: z.array(
				z.object({
					line: z.number(),
					condition: z.string().optional(),
					hitCondition: z.string().optional(),
					logMessage: z.string().optional(),
				}),
			),
		}),
	).optional(),
	viewportConfig: z.object({
		sourceContextLines: z.number().optional(),
		stackDepth: z.number().optional(),
		localsMaxDepth: z.number().optional(),
		localsMaxItems: z.number().optional(),
		stringTruncateLength: z.number().optional(),
		collectionPreviewItems: z.number().optional(),
	}).optional(),
});
export type AttachParams = z.infer<typeof AttachParamsSchema>;
```

**File**: `src/daemon/server.ts` (add `session.attach` handler)

Wire `session.attach` to `sessionManager.attach()`, following the same pattern as `session.launch`.

**File**: `src/cli/commands/index.ts` (add `attachCommand`)

```typescript
// CLI: agent-lens attach --language python --port 5678
// CLI: agent-lens attach --language go --pid 12345
export const attachCommand = defineCommand({
	meta: { name: "attach", description: "Attach to a running process" },
	args: {
		language: { type: "string", required: true, description: "Language: python, javascript, go" },
		pid: { type: "string", description: "Process ID" },
		port: { type: "string", description: "Debug server port" },
		host: { type: "string", description: "Debug server host" },
		break: { type: "string", description: "Breakpoints (file:line)" },
		json: { type: "boolean", description: "JSON output" },
		quiet: { type: "boolean", description: "Quiet mode" },
		session: { type: "string", description: "Session ID" },
	},
	async run({ args }) {
		// Parse args, call daemon client session.attach, format output
	},
});
```

**Implementation Notes:**
- Python attach: The process must already be running with `python -m debugpy --listen 0.0.0.0:5678 --wait-for-client app.py`. The adapter connects to the debugpy listener and sends an `attach` DAP request.
- Node.js attach: The process must be running with `node --inspect=9229 app.js`. The js-debug adapter spawns, connects to the inspector, and sends an `attach` DAP request.
- Go attach: Delve spawns in DAP mode, then the `attach` DAP request includes `processId` for the target PID. Requires `SYS_PTRACE` capability or running as root.
- The `stop` method should use `disconnect` with `terminateDebuggee: false` for attached sessions (don't kill the process we attached to). This requires tracking whether the session was launched or attached.

**File**: `src/core/session-manager.ts` (modify `DebugSession` and `stop`)

```typescript
export interface DebugSession {
	// ... existing fields ...
	/** Whether this session was created via attach (vs launch) */
	isAttached: boolean;
}
```

In `stop()`:
```typescript
try {
	// Don't terminate the debuggee if we attached to it
	await session.dapClient.sendDisconnect(!session.isAttached);
} catch { /* Ignore errors during disconnect */ }
```

**Acceptance Criteria:**
- [ ] `SessionManager.attach()` creates a session by attaching to a running process
- [ ] `debug_attach` MCP tool registered with clear description
- [ ] `session.attach` RPC method in daemon protocol
- [ ] `attach` CLI command wired
- [ ] Python attach works with debugpy `--listen` process
- [ ] Node.js attach works with `--inspect` process
- [ ] Go attach works with Delve by PID
- [ ] `stop()` does NOT kill the process for attached sessions
- [ ] Session limit enforcement applies to attached sessions

---

### Unit 5: Multi-Threaded Debugging

**File**: `src/core/types.ts` (add thread types)

```typescript
export interface ThreadInfo {
	/** DAP thread ID */
	id: number;
	/** Thread name (e.g., "MainThread", "goroutine 1") */
	name: string;
	/** Whether this thread is currently stopped */
	stopped: boolean;
}
```

Add thread indicator to `ViewportSnapshot`:

```typescript
export interface ViewportSnapshot {
	// ... existing fields ...
	/** Thread info when multiple threads exist */
	thread?: {
		id: number;
		name: string;
		totalThreads: number;
	};
}
```

**File**: `src/core/session-manager.ts` (add `getThreads` method)

```typescript
/**
 * List all threads in the debug session.
 */
async getThreads(sessionId: string): Promise<ThreadInfo[]> {
	const session = this.getSession(sessionId);
	this.assertState(session, "stopped");

	const response = await session.dapClient.threads();
	const threads = response.body?.threads ?? [];

	return threads.map((t) => ({
		id: t.id,
		name: t.name,
		stopped: t.id === session.lastStoppedThreadId,
	}));
}
```

**File**: `src/core/session-manager.ts` (add `thread_id` parameter to execution control)

Modify `continue`, `step`, `evaluate`, `getVariables`, `getStackTrace` to accept optional `threadId`:

```typescript
async continue(
	sessionId: string,
	timeoutMs?: number,
	threadId?: number,
): Promise<string> {
	const session = this.getSession(sessionId);
	this.assertState(session, "stopped", "running");
	this.checkAndIncrementAction(session, "debug_continue");

	let stopResultPromise: Promise<StopResult>;

	if (session.state === "stopped") {
		const tid = threadId ?? this.getThreadId(session);
		await session.dapClient.continue(tid);
		session.state = "running";
		stopResultPromise = session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
	} else {
		stopResultPromise = session.pendingStopPromise
			?? session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
		session.pendingStopPromise = null;
	}

	const stopResult = await stopResultPromise;
	return this.handleStopResult(session, stopResult);
}

async step(
	sessionId: string,
	direction: "over" | "into" | "out",
	count = 1,
	threadId?: number,
): Promise<string> {
	const session = this.getSession(sessionId);
	this.assertState(session, "stopped");

	let viewport = "";
	for (let i = 0; i < count; i++) {
		this.checkAndIncrementAction(session, "debug_step");
		const tid = threadId ?? this.getThreadId(session);

		if (direction === "over") await session.dapClient.next(tid);
		else if (direction === "into") await session.dapClient.stepIn(tid);
		else await session.dapClient.stepOut(tid);

		session.state = "running";
		const stopResult = await session.dapClient.waitForStop(this.limits.stepTimeoutMs);
		viewport = await this.handleStopResult(session, stopResult);

		if ((session.state as string) === "terminated") break;
	}

	return viewport;
}
```

Similarly for `evaluate` and `getVariables` — add optional `threadId` parameter, use it to resolve the correct frame.

**File**: `src/core/session-manager.ts` (modify `buildViewport` for thread indicator)

```typescript
// In buildViewport, after building the snapshot:
try {
	const threadsResponse = await session.dapClient.threads();
	const threads = threadsResponse.body?.threads ?? [];
	if (threads.length > 1) {
		const currentThread = threads.find((t) => t.id === (session.lastStoppedThreadId ?? 1));
		snapshot.thread = {
			id: currentThread?.id ?? 1,
			name: currentThread?.name ?? "Thread 1",
			totalThreads: threads.length,
		};
	}
} catch {
	// Thread listing not supported or failed — skip
}
```

**File**: `src/core/viewport.ts` (render thread indicator in header)

```typescript
// In renderViewport, modify header:
let header = `── STOPPED at ${snapshot.file}:${snapshot.line} (${snapshot.function})`;
if (snapshot.thread) {
	header += ` [${snapshot.thread.name} (${snapshot.thread.id}/${snapshot.thread.totalThreads})]`;
}
header += " ──";
lines.push(header);
```

**File**: `src/mcp/tools/index.ts` (add `debug_threads` tool, update execution tools)

```typescript
// Tool 18: debug_threads
server.tool(
	"debug_threads",
	"List all threads in the debug session. " +
		"Useful for multi-threaded programs (Go goroutines, Python threads). " +
		"Shows thread IDs and names. Use thread_id on step/continue/evaluate " +
		"to operate on a specific thread.",
	{
		session_id: z.string().describe("The active debug session"),
	},
	async ({ session_id }) => {
		try {
			const threads = await sessionManager.getThreads(session_id);
			const lines = threads.map((t) =>
				`  ${t.stopped ? "→" : " "} Thread ${t.id}: ${t.name}${t.stopped ? " (stopped)" : " (running)"}`
			);
			return {
				content: [{
					type: "text" as const,
					text: `Threads (${threads.length}):\n${lines.join("\n")}`,
				}],
			};
		} catch (err) {
			return errorResponse(err);
		}
	},
);
```

Update `debug_continue`, `debug_step`, `debug_evaluate` tools to add optional `thread_id` parameter:

```typescript
// Add to debug_continue, debug_step, debug_evaluate schemas:
thread_id: z.number().optional().describe(
	"Thread ID to operate on. Default: the thread that last stopped. " +
	"Use debug_threads to list available threads."
),
```

**File**: `src/daemon/protocol.ts` (add `session.threads`, update schemas)

```typescript
// Add to RpcMethods:
"session.threads": { params: SessionIdParams; result: ThreadInfo[] };

// Update ContinueParams, StepParams, EvaluateParams:
export const ContinueParamsSchema = z.object({
	sessionId: z.string(),
	timeoutMs: z.number().optional(),
	threadId: z.number().optional(),
});

export const StepParamsSchema = z.object({
	sessionId: z.string(),
	direction: z.enum(["over", "into", "out"]),
	count: z.number().optional(),
	threadId: z.number().optional(),
});

export const EvaluateParamsSchema = z.object({
	sessionId: z.string(),
	expression: z.string(),
	frameIndex: z.number().optional(),
	maxDepth: z.number().optional(),
	threadId: z.number().optional(),
});
```

**File**: `src/cli/commands/index.ts` (add `threadsCommand`, update `--thread` flag)

```typescript
// CLI: agent-lens threads
export const threadsCommand = defineCommand({
	meta: { name: "threads", description: "List all threads" },
	// ... standard session/json/quiet flags
});

// Add --thread flag to: continue, step, eval
```

**Implementation Notes:**
- Most single-threaded programs report exactly 1 thread. The thread indicator only appears in the viewport when `threads.length > 1`.
- Go goroutines appear as threads in Delve. `threadId` in DAP maps to goroutine ID.
- Python's `threading` module is supported by debugpy — each Python thread appears as a DAP thread.
- Node.js is typically single-threaded; worker threads appear as separate threads in js-debug.
- The `allThreadsStopped` property on the `StoppedEvent` indicates whether all threads stopped or just one. Currently we don't use this, but it's available for future enhancement.
- Thread-aware `getFrameId` needs the threadId to get the correct stack trace:

```typescript
private async getFrameId(
	session: DebugSession,
	frameIndex: number,
	threadId?: number,
): Promise<number> {
	if (frameIndex === 0 && !threadId && session.lastStoppedFrameId !== null) {
		return session.lastStoppedFrameId;
	}
	const tid = threadId ?? this.getThreadId(session);
	const response = await session.dapClient.stackTrace(tid, 0, frameIndex + 1);
	const frames = response.body?.stackFrames ?? [];
	if (!frames[frameIndex]) throw new Error(`No frame at index ${frameIndex}`);
	return frames[frameIndex].id;
}
```

**Acceptance Criteria:**
- [ ] `debug_threads` tool lists all threads with IDs and names
- [ ] `thread_id` parameter on `debug_continue`, `debug_step`, `debug_evaluate`
- [ ] Viewport header includes thread indicator when multiple threads exist
- [ ] `ThreadInfo` type exported
- [ ] Go goroutines correctly listed as threads
- [ ] Single-threaded programs show no thread indicator (clean output)

---

### Unit 6: Capability Gating

Not all debuggers support all Phase 5 features. The system should expose what's available.

**File**: `src/core/session-manager.ts` (add `getCapabilities`)

```typescript
export interface SessionCapabilities {
	/** Whether conditional breakpoints are supported */
	supportsConditionalBreakpoints: boolean;
	/** Whether hit count breakpoints are supported */
	supportsHitConditionalBreakpoints: boolean;
	/** Whether logpoints are supported */
	supportsLogPoints: boolean;
	/** Whether exception info can be queried */
	supportsExceptionInfo: boolean;
	/** Available exception breakpoint filters */
	exceptionFilters: Array<{ filter: string; label: string }>;
	/** Whether the debugger supports restart */
	supportsRestart: boolean;
	/** Whether set-variable is supported */
	supportsSetVariable: boolean;
}

getCapabilities(sessionId: string): SessionCapabilities {
	const session = this.getSession(sessionId);
	const caps = session.dapClient.capabilities;
	return {
		supportsConditionalBreakpoints: caps.supportsConditionalBreakpoints ?? false,
		supportsHitConditionalBreakpoints: caps.supportsHitConditionalBreakpoints ?? false,
		supportsLogPoints: caps.supportsLogPoints ?? false,
		supportsExceptionInfo: caps.supportsExceptionInfoRequest ?? false,
		exceptionFilters: (caps.exceptionBreakpointFilters ?? []).map((f) => ({
			filter: f.filter,
			label: f.label,
		})),
		supportsRestart: caps.supportsRestartRequest ?? false,
		supportsSetVariable: caps.supportsSetVariable ?? false,
	};
}
```

**File**: `src/mcp/tools/index.ts` (update `debug_status` to include capabilities)

```typescript
async ({ session_id }) => {
	try {
		const result = await sessionManager.getStatus(session_id);
		const caps = sessionManager.getCapabilities(session_id);

		let text = result.viewport
			? `Status: ${result.status}\nActions: ${result.actionCount ?? 0}, Elapsed: ${result.elapsedMs ?? 0}ms, Viewport tokens: ${result.tokenStats?.viewportTokensConsumed ?? 0}\n\n${result.viewport}`
			: `Status: ${result.status}\nActions: ${result.actionCount ?? 0}, Elapsed: ${result.elapsedMs ?? 0}ms, Viewport tokens: ${result.tokenStats?.viewportTokensConsumed ?? 0}`;

		// Append capabilities summary
		const capLines = [
			`Conditional breakpoints: ${caps.supportsConditionalBreakpoints ? "yes" : "no"}`,
			`Hit count breakpoints: ${caps.supportsHitConditionalBreakpoints ? "yes" : "no"}`,
			`Logpoints: ${caps.supportsLogPoints ? "yes" : "no"}`,
			`Exception info: ${caps.supportsExceptionInfo ? "yes" : "no"}`,
		];
		if (caps.exceptionFilters.length > 0) {
			capLines.push(`Exception filters: ${caps.exceptionFilters.map((f) => f.filter).join(", ")}`);
		}
		text += `\n\nCapabilities:\n${capLines.map((l) => `  ${l}`).join("\n")}`;

		return { content: [{ type: "text" as const, text }] };
	} catch (err) {
		return errorResponse(err);
	}
},
```

**Implementation Notes:**
- Capabilities come from the DAP `initialize` response and are already stored in `DAPClient._capabilities`
- Known capability matrix:

| Feature | debugpy | js-debug | Delve |
|---------|---------|----------|-------|
| `supportsConditionalBreakpoints` | yes | yes | yes |
| `supportsHitConditionalBreakpoints` | yes | yes | yes |
| `supportsLogPoints` | yes | yes | **no** |
| `supportsExceptionInfoRequest` | yes | yes | partial |
| Exception filters | raised, uncaught, userUnhandled | all, uncaught | — |
| `supportsRestartRequest` | no | yes | no |
| `supportsSetVariable` | yes | yes | yes |

**Acceptance Criteria:**
- [ ] `getCapabilities()` returns structured capability info from DAP
- [ ] `debug_status` response includes capabilities summary
- [ ] Agent can query capabilities to decide which features to use

---

## Implementation Order

1. **Unit 1: Breakpoint Verification** — Foundation. Improves existing `setBreakpoints` return type. No new APIs.
2. **Unit 2: Exception Breakpoints** — Adds `exceptionInfo` DAP method, `ExceptionInfo` type, viewport enrichment. Builds on existing exception breakpoint flow.
3. **Unit 3: Logpoint Handling** — Mostly documentation/description changes. Depends on Unit 1 for verification feedback.
4. **Unit 6: Capability Gating** — Exposes what the debugger supports. Agents need this to make informed decisions about Units 1–3.
5. **Unit 4: Attach Mode** — New `attach()` method, MCP tool, CLI command. Independent from breakpoint work.
6. **Unit 5: Multi-Threaded Debugging** — New `debug_threads` tool, `thread_id` parameters. Most complex unit.

---

## Testing

### Unit Tests: `tests/unit/core/session-manager.test.ts`

Update existing mock-based tests:

- **Breakpoint verification**: Mock DAP response with adjusted lines, unverified conditions. Assert `VerifiedBreakpoint[]` structure.
- **Exception info**: Mock `exceptionInfo` response. Assert viewport includes `Exception:` header.
- **Capability gating**: Mock capabilities. Assert `getCapabilities()` returns correct structure.
- **Thread listing**: Mock `threads` response. Assert `getThreads()` returns `ThreadInfo[]`.
- **Attach mode**: Mock adapter.attach + DAP handshake. Assert session created with `isAttached: true`.

### Unit Tests: `tests/unit/core/viewport.test.ts`

- **Thread indicator**: Snapshot with `thread` field renders `[Thread name (id/total)]` in header.
- **Thread indicator absent**: Single-threaded snapshot renders clean header (no brackets).
- **Exception in header**: Snapshot with `exception` field renders `Exception: type: message`.

### Integration Tests: `tests/integration/adapters/`

#### `tests/integration/adapters/python-advanced.test.ts`

```typescript
describe.skipIf(SKIP_NO_DEBUGPY)("Python advanced debugging", () => {
	// Conditional breakpoint test
	it("conditional breakpoint stops only when condition is true");

	// Hit count breakpoint test
	it("hit count breakpoint stops after N iterations");

	// Exception breakpoint test
	it("exception breakpoint stops on raised exception with exception info");

	// Logpoint test
	it("logpoint logs message without breaking");

	// Attach test (requires separate debugpy process)
	it("attach to debugpy --listen process");
});
```

#### `tests/integration/adapters/node-advanced.test.ts`

```typescript
describe.skipIf(SKIP_NO_NODE_DEBUG)("Node.js advanced debugging", () => {
	it("conditional breakpoint works");
	it("exception breakpoint stops on throw");
	it("attach to node --inspect process");
});
```

#### `tests/integration/adapters/go-advanced.test.ts`

```typescript
describe.skipIf(SKIP_NO_DLV)("Go advanced debugging", () => {
	it("conditional breakpoint works");
	it("goroutine listing via debug_threads");
	it("step specific goroutine");
});
```

### E2E Tests: `tests/e2e/mcp/`

#### `tests/e2e/mcp/conditional-breakpoints.test.ts`

Full MCP tool flow:
1. `debug_launch` with a loop fixture
2. `debug_set_breakpoints` with condition `i == 5`
3. `debug_continue` — verify stopped with `i == 5`
4. `debug_stop`

#### `tests/e2e/mcp/exception-breakpoints.test.ts` (enhance existing)

1. `debug_launch` with exception-raising fixture
2. `debug_set_exception_breakpoints` with `["raised"]`
3. `debug_continue` — verify viewport includes `Exception: InsufficientFundsError: ...`
4. `debug_stop`

#### `tests/e2e/mcp/attach-mode.test.ts`

1. Spawn a Python process with `debugpy --listen --wait-for-client`
2. `debug_attach` with language=python, port
3. `debug_set_breakpoints`
4. `debug_continue` — verify breakpoint hit
5. `debug_stop` — verify process NOT killed

### Test Fixtures

#### `tests/fixtures/python/threaded.py` (new)

```python
"""Multi-threaded program for thread debugging tests."""
import threading

def worker(name: str, count: int):
    total = 0
    for i in range(count):
        total += i
    print(f"{name}: {total}")

if __name__ == "__main__":
    t1 = threading.Thread(target=worker, args=("worker-1", 5), name="worker-1")
    t2 = threading.Thread(target=worker, args=("worker-2", 5), name="worker-2")
    t1.start()
    t2.start()
    t1.join()
    t2.join()
```

#### `tests/fixtures/go/goroutine.go` (new)

```go
// Multi-goroutine program for thread debugging tests.
package main

import (
	"fmt"
	"sync"
)

func worker(name string, count int, wg *sync.WaitGroup) {
	defer wg.Done()
	total := 0
	for i := 0; i < count; i++ {
		total += i
	}
	fmt.Printf("%s: %d\n", name, total)
}

func main() {
	var wg sync.WaitGroup
	wg.Add(2)
	go worker("worker-1", 5, &wg)
	go worker("worker-2", 5, &wg)
	wg.Wait()
}
```

#### `tests/fixtures/python/attach-target.py` (new)

```python
"""Target for attach mode tests. Runs with debugpy --listen."""
import time

def process():
    count = 0
    while count < 10:
        count += 1
        time.sleep(0.1)
    return count

if __name__ == "__main__":
    result = process()
    print(f"Done: {result}")
```

---

## Verification Checklist

```bash
# Lint
bun run lint

# Unit tests
bun run test:unit

# Integration tests (needs debuggers installed)
bun run test:integration

# E2E tests
bun run test:e2e

# Specific Phase 5 tests
bun run test tests/unit/core/viewport.test.ts
bun run test tests/integration/adapters/python-advanced.test.ts
bun run test tests/integration/adapters/node-advanced.test.ts
bun run test tests/integration/adapters/go-advanced.test.ts
bun run test tests/e2e/mcp/conditional-breakpoints.test.ts
bun run test tests/e2e/mcp/attach-mode.test.ts

# Full suite
bun run test
```
