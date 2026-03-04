import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { DebugProtocol } from "@vscode/debugprotocol";
import type { DAPConnection, DebugAdapter } from "../adapters/base.js";
import { getAdapter, getAdapterForFile } from "../adapters/registry.js";
import { compressionNote, computeEffectiveConfig, estimateTokens, resolveCompressionTier, shouldUseDiffMode } from "./compression.js";
import type { StopResult } from "./dap-client.js";
import { DAPClient } from "./dap-client.js";
import { AdapterNotFoundError, AdapterPrerequisiteError, SessionLimitError, SessionNotFoundError, SessionStateError } from "./errors.js";
import { extractObservations, formatSessionLogDetailed, formatSessionLogSummary } from "./session-logger.js";
import type { Breakpoint, EnrichedActionLogEntry, ResourceLimits, SessionStatus, StopReason, TokenStats, Variable, ViewportConfig, ViewportSnapshot } from "./types.js";
import { ViewportConfigSchema } from "./types.js";
import { convertDAPVariables, renderDAPVariable } from "./value-renderer.js";
import { computeViewportDiff, isDiffEligible, renderViewport, renderViewportDiff } from "./viewport.js";

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
	breakpoints?: Array<{ file: string; breakpoints: Breakpoint[] }>;
	cwd?: string;
	env?: Record<string, string>;
	viewportConfig?: Partial<ViewportConfig>;
	stopOnEntry?: boolean;
}

// --- Session State Machine ---

export type SessionState = "launching" | "running" | "stopped" | "terminated" | "error";

// --- Action Log Entry (kept for backward compat; EnrichedActionLogEntry is now used internally) ---

export interface ActionLogEntry {
	actionNumber: number;
	tool: string;
	summary: string;
	timestamp: number;
}

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
}

// --- Launch Result ---

export interface LaunchResult {
	sessionId: string;
	viewport?: string;
	status: SessionStatus;
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

		// 3. Launch adapter to get DAPConnection
		const connection = await adapter.launch({
			command: options.command,
			cwd: options.cwd,
			env: options.env,
		});

		// 4. Create DAPClient, attach streams
		const dapClient = new DAPClient({ requestTimeoutMs: 10_000, stopTimeoutMs: this.limits.stepTimeoutMs });
		dapClient.attachStreams(connection.reader, connection.writer);

		// Run initialize handshake
		await dapClient.initialize();

		// 5. Set initial breakpoints
		const viewportConfig = ViewportConfigSchema.parse(options.viewportConfig ?? {});
		// Track which fields were explicitly set by the user (not defaulted)
		const explicitViewportFields = new Set<string>(Object.keys(options.viewportConfig ?? {}));
		const breakpointMap = new Map<string, Breakpoint[]>();

		if (options.breakpoints) {
			for (const { file, breakpoints } of options.breakpoints) {
				const absFile = resolve(options.cwd ?? process.cwd(), file);
				await dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(breakpoints));
				breakpointMap.set(absFile, breakpoints);
			}
		}

		// 6. configurationDone
		await dapClient.configurationDone();

		// 7. Send DAP launch request — merge adapter-specific launchArgs over defaults
		const dapLaunchArgs: Record<string, unknown> = {
			noDebug: false,
			program: options.command,
			stopOnEntry: options.stopOnEntry ?? false,
			cwd: options.cwd ?? process.cwd(),
			env: options.env ?? {},
			...connection.launchArgs,
		};
		await dapClient.launch(dapLaunchArgs as DebugProtocol.LaunchRequestArguments);

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
		};

		// 9. Start session timeout
		const timeoutTimer = setTimeout(async () => {
			try {
				session.state = "error";
				await this.stop(sessionId);
			} catch {
				// Ignore errors during timeout cleanup
			}
		}, this.limits.sessionTimeoutMs);
		session.timeoutTimer = timeoutTimer;

		// 10. Register output event handler
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

		this.sessions.set(sessionId, session);

		// 8. If stopOnEntry, wait for stop and build viewport
		let viewport: string | undefined;
		if (options.stopOnEntry) {
			const stopResult = await dapClient.waitForStop(this.limits.stepTimeoutMs);
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

		this.logAction(session, "debug_launch", `Launched ${options.command}`);

		return {
			sessionId,
			viewport,
			status: session.state as SessionStatus,
		};
	}

	/**
	 * Terminate a debug session.
	 */
	async stop(sessionId: string): Promise<StopSessionResult> {
		const session = this.getSession(sessionId);
		clearTimeout(session.timeoutTimer);

		try {
			await session.dapClient.sendDisconnect(true);
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
	 * Continue execution until next stop.
	 */
	async continue(sessionId: string, timeoutMs?: number): Promise<string> {
		return this.withStoppedSession(sessionId, "debug_continue", async (session) => {
			const threadId = this.getThreadId(session);
			await session.dapClient.continue(threadId);
			session.state = "running";
			const stopResult = await session.dapClient.waitForStop(timeoutMs ?? this.limits.stepTimeoutMs);
			return this.handleStopResult(session, stopResult);
		});
	}

	/**
	 * Step in the given direction.
	 */
	async step(sessionId: string, direction: "over" | "into" | "out", count = 1): Promise<string> {
		const session = this.getSession(sessionId);
		this.assertState(session, "stopped");

		let viewport = "";
		for (let i = 0; i < count; i++) {
			this.checkAndIncrementAction(session, "debug_step");
			const threadId = this.getThreadId(session);

			if (direction === "over") {
				await session.dapClient.next(threadId);
			} else if (direction === "into") {
				await session.dapClient.stepIn(threadId);
			} else {
				await session.dapClient.stepOut(threadId);
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
	 */
	async setBreakpoints(sessionId: string, file: string, breakpoints: Breakpoint[]): Promise<DebugProtocol.Breakpoint[]> {
		const session = this.getSession(sessionId);
		const absFile = resolve(process.cwd(), file);

		const response = await session.dapClient.setBreakpoints({ path: absFile, name: file }, toSourceBreakpoints(breakpoints));

		session.breakpointMap.set(absFile, breakpoints);
		this.logAction(session, "debug_set_breakpoints", `Set ${breakpoints.length} breakpoints in ${file}`);

		return response.body?.breakpoints ?? [];
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
							if (scope === "local") return name === "locals" || name === "local";
							if (scope === "global") return name === "globals" || name === "global";
							if (scope === "closure") return name === "closure" || name === "free variables";
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

				const maxName = Math.max(...converted.map((v) => v.name.length), 4);
				for (const v of converted) {
					lines.push(`  ${v.name.padEnd(maxName)}  = ${v.value}`);
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
	 */
	private async buildViewport(session: DebugSession): Promise<ViewportSnapshot> {
		const threadId = this.getThreadId(session);
		const config = session.viewportConfig;

		// Get stack trace
		const stackResponse = await session.dapClient.stackTrace(threadId, 0, config.stackDepth);
		const frames = stackResponse.body?.stackFrames ?? [];

		if (frames.length === 0) {
			throw new Error("No stack frames available");
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

		return {
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
		return this.getThreadId(session);
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
		if (!frames[frameIndex]) throw new Error(`No frame at index ${frameIndex}`);
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

			// 1. Resolve compression tier from action count
			const tier = resolveCompressionTier(session.actionCount);

			// 2. Compute effective viewport config (base + compression overrides)
			const effectiveConfig = computeEffectiveConfig(session.viewportConfig, tier, session.explicitViewportFields);

			// 3. Build viewport snapshot using effective config (temporarily swap for buildViewport)
			const savedConfig = session.viewportConfig;
			session.viewportConfig = effectiveConfig;
			const snapshot = await this.buildViewport(session);
			session.viewportConfig = savedConfig;

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

	/**
	 * Clean up all active sessions (for server shutdown).
	 */
	async disposeAll(): Promise<void> {
		const ids = [...this.sessions.keys()];
		await Promise.allSettled(ids.map((id) => this.stop(id)));
	}
}
