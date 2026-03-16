import { readFileSync } from "node:fs";
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
import { extractObservations, formatSessionLogDetailed, formatSessionLogSummary } from "./session-logger.js";
import type { Breakpoint, EnrichedActionLogEntry, ExceptionInfo, ResourceLimits, SessionStatus, StopReason, ThreadInfo, TokenStats, Variable, ViewportConfig, ViewportSnapshot } from "./types.js";
import { ResourceLimitsSchema, ViewportConfigSchema } from "./types.js";

export type { SessionState };

import { convertDAPVariables, renderDAPVariable } from "./value-renderer.js";
import { computeViewportDiff, isDiffEligible, renderAlignedVariables, renderViewport, renderViewportDiff } from "./viewport.js";

// --- Helpers ---

function toSourceBreakpoints(bps: Breakpoint[]): DebugProtocol.SourceBreakpoint[] {
	return bps.map((bp) => ({
		line: bp.line,
		condition: bp.condition,
		hitCondition: bp.hitCondition,
		logMessage: bp.logMessage,
	}));
}

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

// --- Verified Breakpoint ---

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

// --- Session Capabilities ---

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
			throw new AdapterPrerequisiteError(adapter.id, prereqs.missing ?? [], prereqs.installHint);
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
		this.registerOutputHandler(session, dapClient);

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
			throw new AdapterPrerequisiteError(adapter.id, prereqs.missing ?? [], prereqs.installHint);
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
		this.registerOutputHandler(session, dapClient);

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

		let stopResultPromise: Promise<StopResult>;

		if (session.state === "stopped") {
			const tid = threadId ?? this.getThreadId(session);
			await session.dapClient.continue(tid);
			session.state = "running";
			stopResultPromise = session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
		} else {
			// Use pendingStopPromise if registered during launch to avoid race conditions,
			// otherwise fall back to a fresh waitForStop.
			stopResultPromise = session.pendingStopPromise ?? session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
			session.pendingStopPromise = null;
		}

		const stopResult = await stopResultPromise;
		return this.handleStopResult(session, stopResult);
	}

	/**
	 * Step in the given direction.
	 */
	async step(sessionId: string, direction: StepDirection, count = 1, threadId?: number): Promise<string> {
		const session = this.getSession(sessionId);
		this.assertState(session, "stopped");

		let viewport = "";
		for (let i = 0; i < count; i++) {
			this.checkAndIncrementAction(session, "debug_step");
			const tid = threadId ?? this.getThreadId(session);

			if (direction === "over") {
				await session.dapClient.next(tid);
			} else if (direction === "into") {
				await session.dapClient.stepIn(tid);
			} else {
				await session.dapClient.stepOut(tid);
			}

			session.state = "running";
			const stopResult = await session.dapClient.waitForStop(this.limits.stepTimeoutMs);
			viewport = await this.handleStopResult(session, stopResult);

			if ((session.state as string) === "terminated") break;
		}

		return viewport;
	}

	/**
	 * Run to a specific location by setting a temp breakpoint, continuing,
	 * then removing the temp breakpoint.
	 */
	async runTo(sessionId: string, file: string, line: number, timeoutMs?: number): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_run_to", async (session) => {
			const absFile = resolve(session.connection.process?.pid ? process.cwd() : process.cwd(), file);
			const existing = session.breakpointMap.get(absFile) ?? [];

			// Add temp breakpoint
			const allBps = [...existing, { line } as Breakpoint];
			await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(allBps));

			// Continue
			const threadId = this.getThreadId(session);
			await session.dapClient.continue(threadId);
			session.state = "running";

			const stopResult = await session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
			const viewport = await this.handleStopResult(session, stopResult);

			// Restore original breakpoints (remove temp)
			await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(existing));

			return viewport;
		});
	}

	/**
	 * Set breakpoints in a file (DAP semantics: replaces all in that file).
	 * Returns VerifiedBreakpoint[] with verification info from the debugger.
	 */
	async setBreakpoints(sessionId: string, file: string, breakpoints: Breakpoint[]): Promise<VerifiedBreakpoint[]> {
		const session = this.getSession(sessionId);
		const absFile = resolve(process.cwd(), file);

		const response = await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(breakpoints));

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
				conditionAccepted: bp.condition ? (v?.verified ?? false) : undefined,
			};
		});
	}

	/**
	 * Set exception breakpoint filters.
	 */
	async setExceptionBreakpoints(sessionId: string, filters: string[]): Promise<void> {
		const session = this.getSession(sessionId);
		await session.dapClient.setExceptionBreakpoints(filters);
		this.logAction(session, "debug_set_exception_breakpoints", `Set exception filters: ${filters.join(", ")}`);
	}

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

	/**
	 * Get structured capability info from DAP for this session.
	 */
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
			const frameId = await this.getFrameId(session, frameIndex);
			const response = await session.dapClient.evaluate(expression, frameId, "repl");

			const rendered = renderDAPVariable(
				{
					name: expression,
					value: response.body.result,
					type: response.body.type,
					variablesReference: response.body.variablesReference,
					evaluateName: expression,
					presentationHint: undefined,
					namedVariables: undefined,
					indexedVariables: undefined,
					memoryReference: undefined,
				},
				{
					depth: 0,
					maxDepth,
					stringTruncateLength: session.viewportConfig.stringTruncateLength,
					collectionPreviewItems: session.viewportConfig.collectionPreviewItems,
				},
			);

			this.logAction(session, "debug_evaluate", `Evaluated: ${expression} = ${rendered}`);
			return rendered;
		});
	}

	/**
	 * Get variables for a scope.
	 */
	async getVariables(sessionId: string, scope: "local" | "global" | "closure" | "all" = "local", frameIndex = 0, filter?: string, maxDepth = 1): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_variables", async (session) => {
			const frameId = await this.getFrameId(session, frameIndex);
			const scopesResponse = await session.dapClient.scopes(frameId);
			const scopes = scopesResponse.body?.scopes ?? [];

			const targetScopes =
				scope === "all"
					? scopes
					: scopes.filter((s) => {
							const name = s.name.toLowerCase();
							// Prefix matching handles adapters that append context (e.g. js-debug: "Local: main", "Block: main").
							if (scope === "local") return name.startsWith("local") || name.startsWith("block");
							if (scope === "global") return name.startsWith("global");
							if (scope === "closure") return name.startsWith("closure") || name === "free variables";
							return false;
						});

			const lines: string[] = [];
			const filterRegex = filter ? new RegExp(filter) : null;

			for (const s of targetScopes) {
				const varsResponse = await session.dapClient.variables(s.variablesReference);
				const vars = varsResponse.body?.variables ?? [];
				const converted = convertDAPVariables(vars, session.viewportConfig).filter((v) => !filterRegex || filterRegex.test(v.name));

				if (scope === "all" && converted.length > 0) {
					lines.push(`[${s.name}]`);
				}

				for (const line of renderAlignedVariables(converted, 4)) {
					lines.push(line);
				}
			}

			void maxDepth;
			return lines.join("\n");
		});
	}

	/**
	 * Get the full stack trace.
	 */
	async getStackTrace(sessionId: string, maxFrames = 20, includeSource = false): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_stack_trace", async (session) => {
			const threadId = this.getThreadId(session);
			const response = await session.dapClient.stackTrace(threadId, 0, maxFrames);
			const frames = response.body?.stackFrames ?? [];

			const lines: string[] = [];
			for (let i = 0; i < frames.length; i++) {
				const f = frames[i];
				const marker = i === 0 ? "→" : " ";
				const file = f.source?.path ?? f.source?.name ?? "<unknown>";
				const shortFile = file.split("/").pop() ?? file;
				lines.push(`${marker} #${i} ${shortFile}:${f.line}  ${f.name}()`);

				if (includeSource && f.source?.path) {
					try {
						const sourceLines = await this.readSourceFile(session, f.source.path);
						const start = Math.max(0, f.line - 2);
						const end = Math.min(sourceLines.length - 1, f.line + 1);
						for (let l = start; l <= end; l++) {
							const arrow = l + 1 === f.line ? "→" : " ";
							lines.push(`    ${arrow}${String(l + 1).padStart(4)}│ ${sourceLines[l]}`);
						}
					} catch {
						// Skip source if unavailable
					}
				}
			}

			return lines.join("\n");
		});
	}

	/**
	 * Read source file, return numbered lines for the requested range.
	 */
	async getSource(sessionId: string, file: string, startLine = 1, endLine?: number): Promise<string> {
		const session = this.getSession(sessionId);
		this.checkAndIncrementAction(session, "debug_source");

		const lines = await this.readSourceFile(session, file);
		const end = endLine ?? startLine + 40;
		const slice = lines.slice(startLine - 1, end);

		return slice.map((text, i) => `${String(startLine + i).padStart(4)}│ ${text}`).join("\n");
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
		for (const expr of expressions) {
			if (!session.watchExpressions.includes(expr)) {
				session.watchExpressions.push(expr);
			}
		}
		this.logAction(session, "debug_watch", `Added ${expressions.length} watch expressions`);
		return [...session.watchExpressions];
	}

	/**
	 * Remove watch expressions from the session.
	 * Accepts expressions to remove. Returns remaining watch list.
	 */
	removeWatchExpressions(sessionId: string, expressions: string[]): string[] {
		const session = this.getSession(sessionId);
		session.watchExpressions = session.watchExpressions.filter((e) => !expressions.includes(e));
		this.logAction(session, "debug_unwatch", `Removed ${expressions.length} watch expression(s)`);
		return [...session.watchExpressions];
	}

	/**
	 * Get the enriched session log with observations, compression, and token stats.
	 */
	getSessionLog(sessionId: string, format: "summary" | "detailed" = "summary"): string {
		const session = this.getSession(sessionId);
		const elapsedMs = Date.now() - session.startedAt;

		if (format === "detailed") {
			return formatSessionLogDetailed(session.actionLog, elapsedMs, session.tokenStats);
		}

		return formatSessionLogSummary(session.actionLog, 10, elapsedMs, session.tokenStats);
	}

	/**
	 * Get captured output from the debugee.
	 */
	getOutput(sessionId: string, stream: "stdout" | "stderr" | "both" = "both", sinceAction = 0): string {
		const session = this.getSession(sessionId);
		const lines: string[] = [];

		if (stream === "stdout" || stream === "both") {
			for (const entry of session.outputBuffer.stdout) {
				if (entry.actionNumber >= sinceAction) {
					lines.push(stream === "both" ? `[stdout] ${entry.text}` : entry.text);
				}
			}
		}

		if (stream === "stderr" || stream === "both") {
			for (const entry of session.outputBuffer.stderr) {
				if (entry.actionNumber >= sinceAction) {
					lines.push(stream === "both" ? `[stderr] ${entry.text}` : entry.text);
				}
			}
		}

		return lines.join("");
	}

	/**
	 * Build a ViewportSnapshot from the current DAP state.
	 * @param configOverride Optional viewport config to use instead of session.viewportConfig.
	 */
	private async buildViewport(session: DebugSession, configOverride?: ViewportConfig): Promise<ViewportSnapshot> {
		const threadId = this.getThreadId(session);
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
				sourceLines = await this.readSourceFile(session, sourceFile);
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
	 * Read source file from disk, using per-session cache.
	 */
	private async readSourceFile(session: DebugSession, filePath: string): Promise<string[]> {
		const cached = session.sourceCache.get(filePath);
		if (cached) return cached;

		const text = readFileSync(filePath, "utf-8");
		const lines = text.split("\n");
		session.sourceCache.set(filePath, lines);
		return lines;
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
	 * Get the thread ID for the session.
	 * Falls back to 1 (DAP convention for single-threaded programs with no explicit thread).
	 */
	private getThreadId(session: DebugSession): number {
		return session.lastStoppedThreadId ?? 1;
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
	 * Get frameId for a given frame index.
	 */
	private async getFrameId(session: DebugSession, frameIndex: number): Promise<number> {
		if (frameIndex === 0 && session.lastStoppedFrameId !== null) {
			return session.lastStoppedFrameId;
		}
		const threadId = this.getThreadId(session);
		const response = await session.dapClient.stackTrace(threadId, 0, frameIndex + 1);
		const frames = response.body?.stackFrames ?? [];
		if (!frames[frameIndex]) throw new SessionStateError(session.id, `no-frame-at-index-${frameIndex}`, ["stopped"]);
		return frames[frameIndex].id;
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
						const threadId = this.getThreadId(session);
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
	 * Register the DAP output event handler that captures stdout/stderr into the session buffer.
	 */
	private registerOutputHandler(session: DebugSession, dapClient: DAPClient): void {
		dapClient.on("output", (event) => {
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
				const target = session.outputBuffer.stdout.length >= session.outputBuffer.stderr.length ? session.outputBuffer.stdout : session.outputBuffer.stderr;
				if (target.length === 0) break;
				const removed = target.shift();
				if (removed) session.outputBuffer.totalBytes -= Buffer.byteLength(removed.text);
			}
		});
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
