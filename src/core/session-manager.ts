import { resolve } from "node:path";
import type { DebugProtocol } from "@vscode/debugprotocol";
import type { DAPConnection, DebugAdapter } from "../adapters/base.js";
import { getAdapter, getAdapterForFile } from "../adapters/registry.js";
import { detectFramework } from "../frameworks/index.js";
import { compressionNote, computeEffectiveConfig, estimateTokens, resolveCompressionTier, shouldUseDiffMode } from "./compression.js";
import type { StopResult } from "./dap-client.js";
import { DAPClient } from "./dap-client.js";
import type { SessionState, StepDirection } from "./enums.js";
import { AdapterNotFoundError, AdapterPrerequisiteError, SessionLimitError, SessionNotFoundError, SessionStateError } from "./errors.js";
import { extractObservations } from "./session-logger.js";
import type { Breakpoint, EnrichedActionLogEntry, ExceptionInfo, ResourceLimits, SessionStatus, StopReason, ThreadInfo, TokenStats, Variable, ViewportConfig, ViewportSnapshot } from "./types.js";
import { ResourceLimitsSchema, ViewportConfigSchema } from "./types.js";

export type { SessionState };

import { convertDAPVariables } from "./value-renderer.js";
import { computeViewportDiff, isDiffEligible, renderViewport, renderViewportDiff } from "./viewport.js";

// Re-export types that live in extracted modules
export type { SessionCapabilities, VerifiedBreakpoint } from "./breakpoint-manager.js";

// Import from extracted modules
import {
	getSessionCapabilities,
	getSessionExceptionBreakpointFilters,
	type SessionCapabilities,
	setSessionBreakpoints,
	setSessionExceptionBreakpoints,
	toSourceBreakpoints,
	type VerifiedBreakpoint,
} from "./breakpoint-manager.js";
import { executeContinue, executeRunTo, executeStep, getThreadId } from "./execution-controller.js";
import { addSessionWatchExpressions, getSessionLog, getSessionOutput, registerOutputHandler, removeSessionWatchExpressions } from "./session-output.js";
import { evaluateExpression, getSessionSource, getSessionStackTrace, getSessionVariables, readSourceFile } from "./state-inspector.js";

// --- Launch Options ---

export interface LaunchOptions {
	command: string;
	language?: string;
	/** Explicit framework override. "none" disables auto-detection. */
	framework?: string;
	breakpoints?: Array<{ file: string; breakpoints: Breakpoint[] }>;
	cwd?: string;
	env?: Record<string, string>;
	viewportConfig?: Partial<ViewportConfig>;
	stopOnEntry?: boolean;
}

// --- Attach Options ---

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
	/** Environment variables for the debug target */
	env?: Record<string, string>;
	/** Initial breakpoints */
	breakpoints?: Array<{ file: string; breakpoints: Breakpoint[] }>;
	/** Viewport configuration */
	viewportConfig?: Partial<ViewportConfig>;
}

// --- Session State Machine ---

// --- Output Buffer ---

export interface OutputBuffer {
	stdout: Array<{ text: string; actionNumber: number }>;
	stderr: Array<{ text: string; actionNumber: number }>;
	totalBytes: number;
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
	/** Enriched action log */
	actionLog: EnrichedActionLogEntry[];
	/** Source file cache: path -> lines */
	sourceCache: Map<string, string[]>;
	/** Session timeout timer */
	timeoutTimer: ReturnType<typeof setTimeout>;
	/** Previous viewport snapshot for diff computation */
	previousSnapshot: ViewportSnapshot | null;
	/** Whether diff mode is enabled for this session */
	diffMode: boolean;
	/** Cumulative token stats */
	tokenStats: TokenStats;
	/** Fields explicitly set by user in viewportConfig (not auto-compressed) */
	explicitViewportFields: Set<string>;
	/**
	 * Pending stop event registered before configurationDone during launch.
	 * Used by continue() to avoid the race where stopped fires before waitForStop is registered.
	 */
	pendingStopPromise: Promise<StopResult> | null;
	/** Last exception info (populated when stopped on exception) */
	lastExceptionInfo: ExceptionInfo | null;
	/** Whether this session was created via attach (vs launch) */
	isAttached: boolean;
	/** Detected framework identifier, or null */
	framework: string | null;
	/** Framework-related warnings surfaced at launch */
	frameworkWarnings: string[];
}

// --- Launch Result ---

export interface LaunchResult {
	sessionId: string;
	viewport?: string;
	status: SessionStatus;
	/** Detected framework identifier (e.g., "pytest", "jest") */
	framework?: string;
	/** Warnings explaining framework-specific modifications */
	frameworkWarnings?: string[];
}

// --- Stop Result ---

export interface StopSessionResult {
	duration: number;
	actionCount: number;
}

// --- Session Manager ---

export class SessionManager {
	private sessions = new Map<string, DebugSession>();
	private limits: ResourceLimits;

	constructor(limits: ResourceLimits) {
		this.limits = limits;
	}

	/**
	 * Launch a new debug session.
	 */
	async launch(options: LaunchOptions): Promise<LaunchResult> {
		// 1. Check concurrent session limit
		if (this.sessions.size >= this.limits.maxConcurrentSessions) {
			throw new SessionLimitError("maxConcurrentSessions", this.sessions.size, this.limits.maxConcurrentSessions, "Stop an existing session before launching a new one.");
		}

		// 2. Resolve adapter
		const adapter = this.resolveAdapter(options.command, options.language);

		// Check prerequisites
		const prereqs = await adapter.checkPrerequisites();
		if (!prereqs.satisfied) {
			throw new AdapterPrerequisiteError(adapter.id, prereqs.missing ?? [], prereqs.installHint, prereqs.fixCommand);
		}

		// 2.5. Framework detection — may modify command and env
		const cwd = options.cwd ?? process.cwd();
		const frameworkOverrides = detectFramework(options.command, adapter.id, cwd, options.framework);

		// Apply framework overrides (framework env goes under user env so user wins)
		const effectiveCommand = frameworkOverrides?.command ?? options.command;
		const effectiveEnv = frameworkOverrides?.env ? { ...frameworkOverrides.env, ...options.env } : options.env;

		// 3. Launch adapter to get DAPConnection
		const connection = await adapter.launch({
			command: effectiveCommand,
			cwd,
			env: effectiveEnv,
		});

		// 4. Create DAPClient, attach streams
		const dapClient = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: this.limits.stepTimeoutMs });
		dapClient.attachStreams(connection.reader, connection.writer);

		// Build DAP launch arguments — merge adapter-specific launchArgs over defaults.
		// Strip internal protocol flags (prefixed with _) before sending to the adapter.
		// Framework launchArgs go last so they take precedence over adapter defaults.
		const dapFlow = (connection.launchArgs?._dapFlow as string | undefined) ?? "standard";
		const fireConfigDone = !!connection.launchArgs?._fireConfigDone;
		const { _dapFlow: _ignored, _fireConfigDone: _ignored2, ...adapterLaunchArgs } = connection.launchArgs ?? {};
		const dapLaunchArgs: Record<string, unknown> = {
			noDebug: false,
			program: effectiveCommand,
			stopOnEntry: options.stopOnEntry ?? false,
			cwd,
			env: effectiveEnv ?? {},
			...adapterLaunchArgs,
			...(frameworkOverrides?.launchArgs ?? {}),
		};
		// debugpy (and others) treat program/module/code as mutually exclusive.
		// If the adapter supplied module or code, remove the default program field.
		if (dapLaunchArgs.module !== undefined || dapLaunchArgs.code !== undefined) {
			delete dapLaunchArgs.program;
		}

		// Register `initialized` event listener before initialize() so we never miss it.
		const initializedPromise = this.waitForInitialized(dapClient);

		// 5. Initialize — gets capabilities, does NOT wait for `initialized` event.
		await dapClient.initialize();

		const viewportConfig = ViewportConfigSchema.parse(options.viewportConfig ?? {});
		const explicitViewportFields = new Set<string>(Object.keys(options.viewportConfig ?? {}));
		const breakpointMap = new Map<string, Breakpoint[]>();

		// Register stop listener before configurationDone to avoid race where
		// the stopped event fires before the caller can register waitForStop.
		// Only needed when breakpoints are set (or stopOnEntry) — i.e., a stop is expected.
		const expectStop = !!(options.breakpoints?.length || options.stopOnEntry);
		const pendingStopPromise: Promise<StopResult> | null = expectStop ? dapClient.waitForStop(this.limits.stepTimeoutMs) : null;

		if (dapFlow === "launch-first") {
			// debugpy.adapter protocol: `launch` triggers the server which sends `initialized`.
			// Must send launch first, wait for initialized, then setBreakpoints/configurationDone.
			const launchPromise = dapClient.launch(dapLaunchArgs as DebugProtocol.LaunchRequestArguments);

			// Wait for `initialized` event (arrives after server starts, triggered by launch).
			await initializedPromise;

			// Set breakpoints now that the server is ready.
			if (options.breakpoints) {
				await this.setInitialBreakpoints(dapClient, options.breakpoints, cwd, breakpointMap);
			}

			// Some adapters (e.g. kotlin-debug-adapter) never send a configurationDone response
			// but DO need the request to be sent to trigger JVM startup. Fire without awaiting.
			if (fireConfigDone) {
				dapClient.configurationDone().catch(() => {});
			} else {
				await dapClient.configurationDone();
			}
			await launchPromise;
		} else {
			// Standard DAP protocol: initialized arrives quickly after initialize response.
			await initializedPromise;

			if (options.breakpoints) {
				await this.setInitialBreakpoints(dapClient, options.breakpoints, cwd, breakpointMap);
			}

			// Send configurationDone and launch concurrently: some adapters (e.g. js-debug)
			// only respond to configurationDone after receiving launch, so we must not
			// await configurationDone before sending launch.
			const configDonePromise = dapClient.configurationDone();
			if (dapFlow === "standard-attach") {
				// js-debug child session: send "attach" with __pendingTargetId from startDebugging.
				await dapClient.send("attach", dapLaunchArgs);
			} else {
				await dapClient.launch(dapLaunchArgs as DebugProtocol.LaunchRequestArguments);
			}
			await configDonePromise;
		}

		// Create session object
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
			timeoutTimer: setTimeout(() => {}, 0), // placeholder
			previousSnapshot: null,
			diffMode: false,
			tokenStats: { viewportTokensConsumed: 0, viewportCount: 0 },
			explicitViewportFields,
			pendingStopPromise,
			lastExceptionInfo: null,
			isAttached: false,
			framework: frameworkOverrides?.framework ?? null,
			frameworkWarnings: frameworkOverrides?.warnings ?? [],
		};

		// 9. Start session timeout
		session.timeoutTimer = this.startSessionTimeout(session, sessionId);

		// 10. Register output event handler
		registerOutputHandler(session, dapClient);

		this.sessions.set(sessionId, session);

		// 8. If stopOnEntry, wait for stop and build viewport
		let viewport: string | undefined;
		if (options.stopOnEntry) {
			// Use pendingStopPromise (registered before configurationDone) to avoid race conditions.
			const stopResult = await (session.pendingStopPromise ?? dapClient.waitForStop(this.limits.stepTimeoutMs));
			session.pendingStopPromise = null;
			if (stopResult.type === "stopped") {
				session.state = "stopped";
				session.lastStoppedThreadId = stopResult.event.body.threadId ?? null;
				const snapshot = await this.buildViewport(session);
				viewport = renderViewport(snapshot, viewportConfig);
				session.lastStoppedFrameId = snapshot.stack[0] ? (session.lastStoppedFrameId ?? 0) : null;
			} else {
				session.state = "terminated";
			}
		}

		this.logAction(session, "debug_launch", `Launched ${effectiveCommand}`);

		return {
			sessionId,
			viewport,
			status: session.state as SessionStatus,
			framework: frameworkOverrides?.framework,
			frameworkWarnings: frameworkOverrides?.warnings?.length ? frameworkOverrides.warnings : undefined,
		};
	}

	/**
	 * Terminate a debug session.
	 */
	async stop(sessionId: string): Promise<StopSessionResult> {
		const session = this.getSession(sessionId);
		clearTimeout(session.timeoutTimer);

		try {
			// Don't terminate the debuggee if we attached to it
			await session.dapClient.sendDisconnect(!session.isAttached);
		} catch {
			// Ignore errors during disconnect
		}

		session.dapClient.dispose();
		await session.adapter.dispose();
		this.sessions.delete(sessionId);

		return {
			duration: Date.now() - session.startedAt,
			actionCount: session.actionCount,
		};
	}

	/**
	 * Attach to an already-running process for debugging.
	 */
	async attach(options: AttachOptions): Promise<LaunchResult> {
		// 1. Check concurrent session limit
		if (this.sessions.size >= this.limits.maxConcurrentSessions) {
			throw new SessionLimitError("maxConcurrentSessions", this.sessions.size, this.limits.maxConcurrentSessions, "Stop an existing session before attaching a new one.");
		}

		// 2. Resolve adapter — language is required for attach (no command to infer from)
		const adapter = getAdapter(options.language);
		if (!adapter) throw new AdapterNotFoundError(options.language);

		const prereqs = await adapter.checkPrerequisites();
		if (!prereqs.satisfied) {
			throw new AdapterPrerequisiteError(adapter.id, prereqs.missing ?? [], prereqs.installHint, prereqs.fixCommand);
		}

		// 3. Call adapter.attach()
		const connection = await adapter.attach({
			pid: options.pid,
			port: options.port,
			host: options.host,
		});

		// 4. Create DAPClient, attach streams
		const dapClient = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: this.limits.stepTimeoutMs });
		dapClient.attachStreams(connection.reader, connection.writer);

		// Build DAP attach arguments from adapter's launchArgs
		const dapFlow = (connection.launchArgs?._dapFlow as string | undefined) ?? "standard";
		const { _dapFlow: _ignored, ...adapterAttachArgs } = connection.launchArgs ?? {};
		const dapAttachArgs: Record<string, unknown> = { ...adapterAttachArgs };

		// Register `initialized` listener before initialize
		const initializedPromise = this.waitForInitialized(dapClient);

		// 5. Initialize
		await dapClient.initialize();

		const viewportConfig = ViewportConfigSchema.parse(options.viewportConfig ?? {});
		const explicitViewportFields = new Set<string>(Object.keys(options.viewportConfig ?? {}));
		const breakpointMap = new Map<string, Breakpoint[]>();
		const attachCwd = options.cwd ?? process.cwd();

		if (dapFlow === "launch-first") {
			// Send attach first, wait for initialized
			const attachPromise = dapClient.send("attach", dapAttachArgs);
			await initializedPromise;

			if (options.breakpoints) {
				await this.setInitialBreakpoints(dapClient, options.breakpoints, attachCwd, breakpointMap);
			}

			await dapClient.configurationDone();
			await attachPromise;
		} else {
			await initializedPromise;

			if (options.breakpoints) {
				await this.setInitialBreakpoints(dapClient, options.breakpoints, attachCwd, breakpointMap);
			}

			// Send configurationDone and attach concurrently (same reasoning as launch path above).
			const configDonePromise = dapClient.configurationDone();
			await dapClient.send("attach", dapAttachArgs);
			await configDonePromise;
		}

		// 6. Create session
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
			isAttached: true,
			framework: null,
			frameworkWarnings: [],
		};

		// Session timeout
		session.timeoutTimer = this.startSessionTimeout(session, sessionId);

		// Register output event handler
		registerOutputHandler(session, dapClient);

		this.sessions.set(sessionId, session);
		this.logAction(session, "debug_attach", `Attached to ${options.language} process`);

		return {
			sessionId,
			status: session.state as SessionStatus,
		};
	}

	/**
	 * Continue execution until next stop.
	 * Accepts both `stopped` and `running` states:
	 * - `stopped`: sends DAP continue, then waits for next stop event.
	 * - `running`: the debuggee is already running (e.g., just launched with breakpoints),
	 *   so just waits for the next stop event without resending continue.
	 *   Uses pendingStopPromise if available to avoid race conditions.
	 */
	async continue(sessionId: string, timeoutMs?: number, threadId?: number): Promise<string> {
		const session = this.getSession(sessionId);
		this.assertState(session, "stopped", "running");
		this.checkAndIncrementAction(session, "debug_continue");
		const stopResult = await executeContinue(session, timeoutMs ?? this.limits.stepTimeoutMs, threadId);
		return this.handleStopResult(session, stopResult);
	}

	/**
	 * Step in the given direction.
	 */
	async step(sessionId: string, direction: StepDirection, count = 1, threadId?: number): Promise<string> {
		const session = this.getSession(sessionId);
		this.assertState(session, "stopped");
		return executeStep(session, direction, count, this.limits.stepTimeoutMs, threadId, this.handleStopResult.bind(this), this.checkAndIncrementAction.bind(this));
	}

	/**
	 * Run to a specific location by setting a temp breakpoint, continuing,
	 * then removing the temp breakpoint.
	 */
	async runTo(sessionId: string, file: string, line: number, timeoutMs?: number): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_run_to", async (session) => {
			return executeRunTo(session, file, line, timeoutMs ?? this.limits.stepTimeoutMs, this.handleStopResult.bind(this));
		});
	}

	/**
	 * Set breakpoints in a file (DAP semantics: replaces all in that file).
	 * Returns VerifiedBreakpoint[] with verification info from the debugger.
	 */
	async setBreakpoints(sessionId: string, file: string, breakpoints: Breakpoint[]): Promise<VerifiedBreakpoint[]> {
		const session = this.getSession(sessionId);
		return setSessionBreakpoints(session, file, breakpoints, this.logAction.bind(this));
	}

	/**
	 * Set exception breakpoint filters.
	 */
	async setExceptionBreakpoints(sessionId: string, filters: string[]): Promise<void> {
		const session = this.getSession(sessionId);
		return setSessionExceptionBreakpoints(session, filters, this.logAction.bind(this));
	}

	/**
	 * Get the available exception breakpoint filters for a session.
	 * Reads from DAP capabilities negotiated during initialize.
	 */
	getExceptionBreakpointFilters(sessionId: string): Array<{ filter: string; label: string; default?: boolean }> {
		const session = this.getSession(sessionId);
		return getSessionExceptionBreakpointFilters(session);
	}

	/**
	 * Get structured capability info from DAP for this session.
	 */
	getCapabilities(sessionId: string): SessionCapabilities {
		const session = this.getSession(sessionId);
		return getSessionCapabilities(session);
	}

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

	/**
	 * List all breakpoints across all files for a session.
	 */
	listBreakpoints(sessionId: string): Map<string, Breakpoint[]> {
		const session = this.getSession(sessionId);
		return new Map(session.breakpointMap);
	}

	/**
	 * Evaluate an expression in a stack frame.
	 */
	async evaluate(sessionId: string, expression: string, frameIndex = 0, maxDepth = 2): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_evaluate", async (session) => {
			return evaluateExpression(session, expression, frameIndex, maxDepth, getThreadId, this.logAction.bind(this));
		});
	}

	/**
	 * Get variables for a scope.
	 */
	async getVariables(sessionId: string, scope: "local" | "global" | "closure" | "all" = "local", frameIndex = 0, filter?: string, maxDepth = 1): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_variables", async (session) => {
			return getSessionVariables(session, scope, frameIndex, filter, maxDepth, getThreadId);
		});
	}

	/**
	 * Get the full stack trace.
	 */
	async getStackTrace(sessionId: string, maxFrames = 20, includeSource = false): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_stack_trace", async (session) => {
			return getSessionStackTrace(session, maxFrames, includeSource, getThreadId);
		});
	}

	/**
	 * Read source file, return numbered lines for the requested range.
	 */
	async getSource(sessionId: string, file: string, startLine = 1, endLine?: number): Promise<string> {
		const session = this.getSession(sessionId);
		this.checkAndIncrementAction(session, "debug_source");
		return getSessionSource(session, file, startLine, endLine);
	}

	/**
	 * Get current session status with viewport if stopped.
	 */
	async getStatus(sessionId: string): Promise<{ status: SessionStatus; viewport?: string; tokenStats?: TokenStats; actionCount?: number; elapsedMs?: number }> {
		const session = this.getSession(sessionId);
		let viewport: string | undefined;

		if (session.state === "stopped") {
			const snapshot = await this.buildViewport(session);
			viewport = renderViewport(snapshot, session.viewportConfig);
		}

		return {
			status: session.state as SessionStatus,
			viewport,
			tokenStats: { ...session.tokenStats },
			actionCount: session.actionCount,
			elapsedMs: Date.now() - session.startedAt,
		};
	}

	/**
	 * List all active sessions with their status.
	 */
	listSessions(): Array<{ id: string; status: string; language: string; actionCount: number }> {
		return Array.from(this.sessions.values()).map((session) => ({
			id: session.id,
			status: session.state,
			language: session.language,
			actionCount: session.actionCount,
		}));
	}

	/**
	 * Add watch expressions to the session.
	 */
	addWatchExpressions(sessionId: string, expressions: string[]): string[] {
		const session = this.getSession(sessionId);
		return addSessionWatchExpressions(session, expressions, this.logAction.bind(this));
	}

	/**
	 * Remove watch expressions from the session.
	 * Accepts expressions to remove. Returns remaining watch list.
	 */
	removeWatchExpressions(sessionId: string, expressions: string[]): string[] {
		const session = this.getSession(sessionId);
		return removeSessionWatchExpressions(session, expressions, this.logAction.bind(this));
	}

	/**
	 * Get the enriched session log with observations, compression, and token stats.
	 */
	getSessionLog(sessionId: string, format: "summary" | "detailed" = "summary"): string {
		const session = this.getSession(sessionId);
		return getSessionLog(session, format);
	}

	/**
	 * Get captured output from the debugee.
	 */
	getOutput(sessionId: string, stream: "stdout" | "stderr" | "both" = "both", sinceAction = 0): string {
		const session = this.getSession(sessionId);
		return getSessionOutput(session, stream, sinceAction);
	}

	/**
	 * Build a ViewportSnapshot from the current DAP state.
	 * @param configOverride Optional viewport config to use instead of session.viewportConfig.
	 */
	private async buildViewport(session: DebugSession, configOverride?: ViewportConfig): Promise<ViewportSnapshot> {
		const threadId = getThreadId(session);
		const config = configOverride ?? session.viewportConfig;

		// Get stack trace
		const stackResponse = await session.dapClient.stackTrace(threadId, 0, config.stackDepth);
		const frames = stackResponse.body?.stackFrames ?? [];

		if (frames.length === 0) {
			throw new SessionStateError(session.id, "no-frames", ["stopped"]);
		}

		const topFrame = frames[0];
		const frameId = topFrame.id;
		session.lastStoppedFrameId = frameId;

		// Get scopes and locals for top frame
		const scopesResponse = await session.dapClient.scopes(frameId);
		const scopes = scopesResponse.body?.scopes ?? [];
		const localsScope = scopes.find((s) => s.name.toLowerCase() === "locals" || s.name.toLowerCase() === "local");

		let locals: Variable[] = [];
		if (localsScope) {
			const varsResponse = await session.dapClient.variables(localsScope.variablesReference);
			locals = convertDAPVariables(varsResponse.body?.variables ?? [], config);
		}

		// Read source file
		const sourceFile = topFrame.source?.path ?? topFrame.source?.name ?? "";
		let sourceLines: string[] = [];
		if (sourceFile) {
			try {
				sourceLines = await readSourceFile(session, sourceFile);
			} catch {
				// Source unavailable
			}
		}

		const currentLine = topFrame.line;
		const halfContext = Math.floor(config.sourceContextLines / 2);
		const startLine = Math.max(1, currentLine - halfContext);
		const endLine = Math.min(sourceLines.length, currentLine + Math.ceil(config.sourceContextLines / 2));

		const source = sourceLines.slice(startLine - 1, endLine).map((text, i) => ({ line: startLine + i, text }));

		// Evaluate watch expressions
		let watches: Variable[] | undefined;
		if (session.watchExpressions.length > 0) {
			watches = [];
			for (const expr of session.watchExpressions) {
				try {
					const evalResult = await session.dapClient.evaluate(expr, frameId, "watch");
					watches.push({ name: expr, value: evalResult.body.result, type: evalResult.body.type });
				} catch {
					watches.push({ name: expr, value: "<error>", type: undefined });
				}
			}
		}

		// Build stack frames
		const stackFrames = frames.map((f) => {
			const file = f.source?.path ?? f.source?.name ?? "<unknown>";
			return {
				file,
				shortFile: file.split("/").pop() ?? file,
				line: f.line,
				function: f.name,
				arguments: "",
			};
		});

		// Determine stop reason (use the last stored one or default)
		const reason: StopReason = "breakpoint";

		const shortFile = sourceFile.split("/").pop() ?? sourceFile;

		const snapshot: ViewportSnapshot = {
			file: shortFile,
			line: currentLine,
			function: topFrame.name,
			reason,
			stack: stackFrames,
			totalFrames: stackResponse.body?.totalFrames ?? frames.length,
			source,
			locals,
			watches,
		};

		// Add exception info if available (only set when stopped on exception)
		if (session.lastExceptionInfo) {
			snapshot.exception = session.lastExceptionInfo;
		}

		// Add thread indicator when multiple threads exist
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

		return snapshot;
	}

	/**
	 * Map a DAP StoppedEvent reason string to our StopReason type.
	 */
	private mapStopReason(dapReason: string): StopReason {
		switch (dapReason) {
			case "breakpoint":
				return "breakpoint";
			case "step":
				return "step";
			case "exception":
				return "exception";
			case "pause":
				return "pause";
			case "entry":
				return "entry";
			default:
				return "breakpoint";
		}
	}

	/**
	 * Increment action count, check limits.
	 */
	private checkAndIncrementAction(session: DebugSession, toolName: string): void {
		session.actionCount++;
		if (session.actionCount > session.limits.maxActionsPerSession) {
			throw new SessionLimitError("maxActionsPerSession", session.actionCount, session.limits.maxActionsPerSession, "Consider using conditional breakpoints to reduce step count.");
		}
		this.logAction(session, toolName, `Action ${session.actionCount}`);
	}

	/**
	 * Log an action to the session log.
	 */
	private logAction(
		session: DebugSession,
		tool: string,
		summary: string,
		keyParams?: Record<string, unknown>,
		snapshot?: ViewportSnapshot | null,
		observations?: import("./types.js").ActionObservation[],
	): void {
		const location = snapshot ? `${snapshot.file}:${snapshot.line} ${snapshot.function}` : undefined;
		session.actionLog.push({
			actionNumber: session.actionCount,
			tool,
			summary,
			timestamp: Date.now(),
			keyParams: keyParams ?? {},
			observations: observations ?? [],
			location,
		});
	}

	/**
	 * Get the session or throw a clear error.
	 */
	private getSession(sessionId: string): DebugSession {
		const session = this.sessions.get(sessionId);
		if (!session) throw new SessionNotFoundError(sessionId);
		return session;
	}

	/**
	 * Assert the session is in one of the allowed states.
	 */
	private assertState(session: DebugSession, ...allowedStates: SessionState[]): void {
		if (!allowedStates.includes(session.state)) {
			throw new SessionStateError(session.id, session.state, allowedStates);
		}
	}

	/**
	 * Retrieve session, assert it is stopped, increment action count, then call fn.
	 * Use this for all actions that require a stopped session.
	 */
	private async withStoppedSession<T>(sessionId: string, toolName: string, fn: (session: DebugSession) => Promise<T>): Promise<T> {
		const session = this.getSession(sessionId);
		this.assertState(session, "stopped");
		this.checkAndIncrementAction(session, toolName);
		return fn(session);
	}

	/**
	 * Generate a unique session ID.
	 */
	private generateSessionId(): string {
		return crypto.randomUUID().replace(/-/g, "").slice(0, 8);
	}

	/**
	 * Resolve adapter from registry by language or file extension.
	 */
	private resolveAdapter(command: string, language?: string): DebugAdapter {
		if (language) {
			const adapter = getAdapter(language);
			if (adapter) return adapter;
			throw new AdapterNotFoundError(language);
		}

		// Try to detect from file extension
		const parts = command.trim().split(/\s+/);
		for (const part of parts) {
			if (part.includes(".")) {
				const adapter = getAdapterForFile(part);
				if (adapter) return adapter;
			}
		}

		// Default to python for python commands
		const firstWord = parts[0] ?? "";
		if (firstWord === "python" || firstWord === "python3") {
			const adapter = getAdapter("python");
			if (adapter) return adapter;
		}

		throw new AdapterNotFoundError(command);
	}

	/**
	 * Handle a stop result and return the rendered viewport.
	 * Applies compression tier, diff mode, observation extraction, and token tracking.
	 */
	private async handleStopResult(session: DebugSession, stopResult: StopResult): Promise<string> {
		if (stopResult.type === "stopped") {
			session.state = "stopped";
			session.lastStoppedThreadId = stopResult.event.body.threadId ?? null;
			const reason = this.mapStopReason(stopResult.event.body.reason);

			// Query exception info when stopped on exception
			if (reason === "exception") {
				try {
					const caps = session.dapClient.capabilities;
					if (caps.supportsExceptionInfoRequest) {
						const threadId = getThreadId(session);
						const exInfo = await session.dapClient.exceptionInfo(threadId);
						session.lastExceptionInfo = {
							type: exInfo.body.exceptionId ?? "Unknown",
							message: exInfo.body.description ?? exInfo.body.exceptionId ?? "",
							exceptionId: exInfo.body.exceptionId,
						};
					} else {
						session.lastExceptionInfo = null;
					}
				} catch {
					session.lastExceptionInfo = null;
				}
			} else {
				session.lastExceptionInfo = null;
			}

			// 1. Resolve compression tier from action count
			const tier = resolveCompressionTier(session.actionCount);

			// 2. Compute effective viewport config (base + compression overrides)
			const effectiveConfig = computeEffectiveConfig(session.viewportConfig, tier, session.explicitViewportFields);

			// 3. Build viewport snapshot using effective config (passed as override to avoid mutation)
			const snapshot = await this.buildViewport(session, effectiveConfig);

			// Update reason from the actual stop event
			snapshot.reason = reason;

			// 4. Determine compression note
			const note = compressionNote(session.actionCount, session.limits.maxActionsPerSession, tier);

			// 5. Render viewport: diff mode if eligible, otherwise full
			let renderedViewport: string;
			const useDiff = shouldUseDiffMode(tier, session.diffMode);

			if (useDiff && session.previousSnapshot && isDiffEligible(snapshot, session.previousSnapshot)) {
				const diff = computeViewportDiff(snapshot, session.previousSnapshot, note);
				renderedViewport = renderViewportDiff(diff, effectiveConfig);
			} else {
				if (note) snapshot.compressionNote = note;
				renderedViewport = renderViewport(snapshot, effectiveConfig);
			}

			// 6. Extract observations and log enriched action entry
			const observations = extractObservations(snapshot, session.previousSnapshot);
			this.logAction(session, "debug_stop", `Stopped at ${snapshot.file}:${snapshot.line} (${reason})`, { reason }, snapshot, observations);

			// 7. Track token consumption
			const tokens = estimateTokens(renderedViewport);
			session.tokenStats.viewportTokensConsumed += tokens;
			session.tokenStats.viewportCount += 1;

			// 8. Store snapshot as previousSnapshot for next diff
			session.previousSnapshot = snapshot;

			// Auto-activate diff mode when tier requires it
			if (tier.diffMode) {
				session.diffMode = true;
			}

			return renderedViewport;
		}

		// Log termination
		this.logAction(session, "debug_stop", "Session terminated", {}, null, [{ kind: "terminated", description: "Session terminated" }]);
		session.state = "terminated";
		return "Session terminated.";
	}

	// --- Session Initialization Helpers ---

	/**
	 * Return a Promise that resolves when the DAP client emits the 'initialized' event.
	 * Register this BEFORE calling dapClient.initialize() to avoid missing the event.
	 */
	private waitForInitialized(dapClient: DAPClient): Promise<void> {
		return new Promise<void>((resolve) => {
			const handler = () => {
				dapClient.off("initialized", handler);
				resolve();
			};
			dapClient.on("initialized", handler);
		});
	}

	/**
	 * Set breakpoints from the provided list and record them in breakpointMap.
	 */
	private async setInitialBreakpoints(dapClient: DAPClient, breakpoints: Array<{ file: string; breakpoints: Breakpoint[] }>, cwd: string, breakpointMap: Map<string, Breakpoint[]>): Promise<void> {
		for (const { file, breakpoints: bps } of breakpoints) {
			const absFile = resolve(cwd, file);
			await dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(bps));
			breakpointMap.set(absFile, bps);
		}
	}

	/**
	 * Start the session inactivity timeout. Returns the timer handle (stored on session.timeoutTimer).
	 */
	private startSessionTimeout(session: DebugSession, sessionId: string): ReturnType<typeof setTimeout> {
		return setTimeout(async () => {
			try {
				session.state = "error";
				await this.stop(sessionId);
			} catch {
				// Ignore errors during timeout cleanup
			}
		}, this.limits.sessionTimeoutMs);
	}

	/**
	 * Clean up all active sessions (for server shutdown).
	 */
	async disposeAll(): Promise<void> {
		const ids = [...this.sessions.keys()];
		await Promise.allSettled(ids.map((id) => this.stop(id)));
	}
}

/**
 * Create a SessionManager with default resource limits.
 * Use this in entry points instead of repeating the ResourceLimitsSchema.parse({}) pattern.
 */
export function createSessionManager(): SessionManager {
	return new SessionManager(ResourceLimitsSchema.parse({}));
}
