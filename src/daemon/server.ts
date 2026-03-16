import { unlinkSync, writeFileSync } from "node:fs";
import type { Server, Socket } from "node:net";
import { createServer } from "node:net";
import { resolve } from "node:path";
import { HARExporter } from "../browser/export/har.js";
import { SessionDiffer } from "../browser/investigation/diff.js";
import { QueryEngine } from "../browser/investigation/query-engine.js";
import { renderDiff, renderInspectResult, renderSearchResults, renderSessionOverview } from "../browser/investigation/renderers.js";
import { ReplayContextGenerator } from "../browser/investigation/replay-context.js";
import { BrowserRecorder } from "../browser/recorder/index.js";
import { BrowserDatabase } from "../browser/storage/database.js";
import { AdapterNotFoundError, AdapterPrerequisiteError, BrowserRecorderStateError, KrometrailError, LaunchError, SessionLimitError, SessionNotFoundError, SessionStateError } from "../core/errors.js";
import { getKrometrailSubdir } from "../core/paths.js";
import type { SessionManager } from "../core/session-manager.js";
import type { JsonRpcRequest, JsonRpcResponse } from "./protocol.js";
import {
	AttachParamsSchema,
	BrowserDiffParamsSchema,
	BrowserExportParamsSchema,
	BrowserInspectParamsSchema,
	BrowserMarkParamsSchema,
	BrowserOverviewParamsSchema,
	BrowserReplayContextParamsSchema,
	BrowserSearchParamsSchema,
	BrowserSessionsParamsSchema,
	BrowserStartParamsSchema,
	BrowserStopParamsSchema,
	ContinueParamsSchema,
	EvaluateParamsSchema,
	LaunchParamsSchema,
	OutputParamsSchema,
	RPC_ADAPTER_ERROR,
	RPC_INTERNAL_ERROR,
	RPC_INVALID_REQUEST,
	RPC_LAUNCH_ERROR,
	RPC_METHOD_NOT_FOUND,
	RPC_PARSE_ERROR,
	RPC_SESSION_LIMIT_ERROR,
	RPC_SESSION_NOT_FOUND,
	RPC_SESSION_STATE_ERROR,
	RunToParamsSchema,
	SessionIdParamsSchema,
	SessionLogParamsSchema,
	SetBreakpointsParamsSchema,
	SetExceptionBreakpointsParamsSchema,
	SourceParamsSchema,
	StackTraceParamsSchema,
	StepParamsSchema,
	UnwatchParamsSchema,
	VariablesParamsSchema,
	WatchParamsSchema,
} from "./protocol.js";

export interface DaemonOptions {
	/** Path to the Unix domain socket. */
	socketPath: string;
	/** Path to the PID file. */
	pidPath: string;
	/** Idle timeout in ms before auto-shutdown. Default: 60000. */
	idleTimeoutMs: number;
}

export const DEFAULT_DAEMON_OPTIONS: DaemonOptions = {
	socketPath: "", // resolved at startup
	pidPath: "",
	idleTimeoutMs: 60_000,
};

/**
 * The daemon process manages a SessionManager and listens for
 * JSON-RPC requests over a Unix domain socket.
 */
export class DaemonServer {
	private server: Server | null = null;
	private sessionManager: SessionManager;
	private options: DaemonOptions;
	private idleTimer: ReturnType<typeof setTimeout> | null = null;
	private startedAt: number = Date.now();
	private activeConnections: Set<Socket> = new Set();
	private browserRecorder: BrowserRecorder | null = null;
	private browserQueryEngine: QueryEngine | null = null;
	private browserDb: BrowserDatabase | null = null;

	constructor(sessionManager: SessionManager, options: DaemonOptions) {
		this.sessionManager = sessionManager;
		this.options = options;
	}

	/**
	 * Start the daemon: bind the Unix socket, write PID file, begin listening.
	 * Removes stale socket file if it exists.
	 */
	async start(): Promise<void> {
		const { socketPath, pidPath } = this.options;

		// Check for stale socket
		try {
			const { connect } = await import("node:net");
			await new Promise<void>((resolve, reject) => {
				const testSocket = connect(socketPath, () => {
					testSocket.destroy();
					reject(new Error(`Another daemon is already running at ${socketPath}`));
				});
				testSocket.on("error", (err: NodeJS.ErrnoException) => {
					if (err.code === "ECONNREFUSED" || err.code === "ENOENT") {
						resolve(); // Stale socket or no socket — clean it up
					} else {
						reject(err);
					}
				});
			});
		} catch (err) {
			if ((err as Error).message.startsWith("Another daemon")) {
				throw err;
			}
			// ENOENT or similar — no socket file, proceed
		}

		// Remove stale socket file if present
		try {
			unlinkSync(socketPath);
		} catch {
			// Ignore if it doesn't exist
		}

		// Create server
		this.server = createServer((socket) => this.handleConnection(socket));

		await new Promise<void>((resolve, reject) => {
			this.server?.listen(socketPath, resolve);
			this.server?.once("error", reject);
		});

		// Write PID file
		writeFileSync(pidPath, String(process.pid));

		this.startedAt = Date.now();

		// Start idle timer
		this.resetIdleTimer();
	}

	/**
	 * Shut down: close all connections, clean up sessions,
	 * remove socket file, remove PID file.
	 */
	async shutdown(): Promise<void> {
		if (this.idleTimer) {
			clearTimeout(this.idleTimer);
			this.idleTimer = null;
		}

		// Close all active connections
		for (const socket of this.activeConnections) {
			socket.destroy();
		}
		this.activeConnections.clear();

		// Close server
		if (this.server) {
			await new Promise<void>((resolve) => this.server?.close(() => resolve()));
			this.server = null;
		}

		// Stop browser recording if active
		if (this.browserRecorder) {
			try {
				await this.browserRecorder.stop();
			} catch {
				// Ignore errors during cleanup
			}
			this.browserRecorder = null;
		}

		// Close browser query engine
		if (this.browserDb) {
			try {
				this.browserDb.close();
			} catch {
				// Ignore errors during cleanup
			}
			this.browserDb = null;
			this.browserQueryEngine = null;
		}

		// Dispose all sessions
		try {
			await this.sessionManager.disposeAll();
		} catch {
			// Ignore errors during cleanup
		}

		// Remove socket file
		try {
			unlinkSync(this.options.socketPath);
		} catch {
			// Ignore
		}

		// Remove PID file
		try {
			unlinkSync(this.options.pidPath);
		} catch {
			// Ignore
		}
	}

	/**
	 * Handle a single JSON-RPC request by dispatching to the session manager.
	 */
	private async handleRequest(request: JsonRpcRequest): Promise<JsonRpcResponse> {
		this.resetIdleTimer();

		try {
			const result = await this.dispatch(request.method, request.params ?? {});
			return {
				jsonrpc: "2.0",
				id: request.id,
				result: result ?? null,
			};
		} catch (err) {
			const error = this.mapError(err);
			return {
				jsonrpc: "2.0",
				id: request.id,
				error,
			};
		}
	}

	/**
	 * Map errors to JSON-RPC error objects.
	 */
	private mapError(err: unknown): { code: number; message: string; data?: unknown } {
		if (err instanceof SessionNotFoundError) {
			return { code: RPC_SESSION_NOT_FOUND, message: err.message };
		}
		if (err instanceof SessionStateError) {
			return { code: RPC_SESSION_STATE_ERROR, message: err.message };
		}
		if (err instanceof SessionLimitError) {
			return { code: RPC_SESSION_LIMIT_ERROR, message: err.message };
		}
		if (err instanceof AdapterPrerequisiteError || err instanceof AdapterNotFoundError) {
			return { code: RPC_ADAPTER_ERROR, message: (err as KrometrailError).message };
		}
		if (err instanceof LaunchError) {
			return { code: RPC_LAUNCH_ERROR, message: err.message };
		}
		if (err instanceof KrometrailError) {
			return { code: RPC_INTERNAL_ERROR, message: err.message };
		}
		if (err instanceof Error) {
			return { code: RPC_INTERNAL_ERROR, message: err.message };
		}
		return { code: RPC_INTERNAL_ERROR, message: String(err) };
	}

	/**
	 * Dispatch a validated RPC method call to SessionManager.
	 */
	private async dispatch(method: string, params: Record<string, unknown>): Promise<unknown> {
		switch (method) {
			// --- Session Lifecycle ---
			case "session.launch": {
				const p = LaunchParamsSchema.parse(params);
				return this.sessionManager.launch(p);
			}

			case "session.attach": {
				const p = AttachParamsSchema.parse(params);
				return this.sessionManager.attach(p);
			}

			case "session.stop": {
				const p = SessionIdParamsSchema.parse(params);
				return this.sessionManager.stop(p.sessionId);
			}

			case "session.status": {
				const p = SessionIdParamsSchema.parse(params);
				return this.sessionManager.getStatus(p.sessionId);
			}

			// --- Execution Control ---
			case "session.continue": {
				const p = ContinueParamsSchema.parse(params);
				const viewport = await this.sessionManager.continue(p.sessionId, p.timeoutMs, p.threadId);
				return { viewport };
			}

			case "session.step": {
				const p = StepParamsSchema.parse(params);
				const viewport = await this.sessionManager.step(p.sessionId, p.direction, p.count, p.threadId);
				return { viewport };
			}

			case "session.runTo": {
				const p = RunToParamsSchema.parse(params);
				const viewport = await this.sessionManager.runTo(p.sessionId, p.file, p.line, p.timeoutMs);
				return { viewport };
			}

			// --- Breakpoints ---
			case "session.setBreakpoints": {
				const p = SetBreakpointsParamsSchema.parse(params);
				const verifiedBps = await this.sessionManager.setBreakpoints(p.sessionId, p.file, p.breakpoints);
				return { breakpoints: verifiedBps };
			}

			case "session.setExceptionBreakpoints": {
				const p = SetExceptionBreakpointsParamsSchema.parse(params);
				await this.sessionManager.setExceptionBreakpoints(p.sessionId, p.filters);
				return null;
			}

			case "session.listBreakpoints": {
				const p = SessionIdParamsSchema.parse(params);
				const bpMap = this.sessionManager.listBreakpoints(p.sessionId);
				const files: Record<string, Array<{ line: number; condition?: string; hitCondition?: string; logMessage?: string }>> = {};
				for (const [file, bps] of bpMap) {
					files[file] = bps.map((bp) => ({
						line: bp.line,
						...(bp.condition !== undefined && { condition: bp.condition }),
						...(bp.hitCondition !== undefined && { hitCondition: bp.hitCondition }),
						...(bp.logMessage !== undefined && { logMessage: bp.logMessage }),
					}));
				}
				return { files };
			}

			// --- State Inspection ---
			case "session.evaluate": {
				const p = EvaluateParamsSchema.parse(params);
				return this.sessionManager.evaluate(p.sessionId, p.expression, p.frameIndex, p.maxDepth);
			}

			case "session.variables": {
				const p = VariablesParamsSchema.parse(params);
				return this.sessionManager.getVariables(p.sessionId, p.scope, p.frameIndex, p.filter, p.maxDepth);
			}

			case "session.stackTrace": {
				const p = StackTraceParamsSchema.parse(params);
				return this.sessionManager.getStackTrace(p.sessionId, p.maxFrames, p.includeSource);
			}

			case "session.source": {
				const p = SourceParamsSchema.parse(params);
				return this.sessionManager.getSource(p.sessionId, p.file, p.startLine, p.endLine);
			}

			// --- Session Intelligence ---
			case "session.watch": {
				const p = WatchParamsSchema.parse(params);
				return this.sessionManager.addWatchExpressions(p.sessionId, p.expressions);
			}

			case "session.unwatch": {
				const p = UnwatchParamsSchema.parse(params);
				return this.sessionManager.removeWatchExpressions(p.sessionId, p.expressions);
			}

			case "session.sessionLog": {
				const p = SessionLogParamsSchema.parse(params);
				return this.sessionManager.getSessionLog(p.sessionId, p.format);
			}

			case "session.output": {
				const p = OutputParamsSchema.parse(params);
				return this.sessionManager.getOutput(p.sessionId, p.stream, p.sinceAction);
			}

			case "session.threads": {
				const p = SessionIdParamsSchema.parse(params);
				return this.sessionManager.getThreads(p.sessionId);
			}

			// --- Daemon Control ---
			case "daemon.ping": {
				return {
					uptime: Date.now() - this.startedAt,
					sessions: this.sessionManager.listSessions().length,
				};
			}

			case "daemon.sessions": {
				return this.sessionManager.listSessions();
			}

			case "daemon.shutdown": {
				// Shutdown asynchronously after responding
				setImmediate(() =>
					this.shutdown()
						.then(() => process.exit(0))
						.catch(() => process.exit(1)),
				);
				return null;
			}

			// --- Browser Recording ---
			case "browser.start": {
				const p = BrowserStartParamsSchema.parse(params);
				if (this.browserRecorder?.isRecording()) {
					throw new BrowserRecorderStateError("Browser recording is already active. Call browser.stop first.");
				}
				this.browserRecorder = new BrowserRecorder({
					port: p.port,
					attach: p.attach,
					profile: p.profile,
					allTabs: p.allTabs,
					tabFilter: p.tabFilter,
					url: p.url,
					persistence: {},
					...(p.screenshotIntervalMs !== undefined && { screenshots: { intervalMs: p.screenshotIntervalMs } }),
					frameworkState: p.frameworkState,
				});
				return this.browserRecorder.start();
			}

			case "browser.mark": {
				const p = BrowserMarkParamsSchema.parse(params);
				if (!this.browserRecorder?.isRecording()) {
					throw new BrowserRecorderStateError("No active browser recording. Call browser.start first.");
				}
				return this.browserRecorder.placeMarker(p.label);
			}

			case "browser.status": {
				return this.browserRecorder?.getSessionInfo() ?? null;
			}

			case "browser.stop": {
				const p = BrowserStopParamsSchema.parse(params);
				if (!this.browserRecorder) return null;
				await this.browserRecorder.stop(p.closeBrowser);
				this.browserRecorder = null;
				return null;
			}

			// --- Browser Investigation ---
			case "browser.sessions": {
				const p = BrowserSessionsParamsSchema.parse(params);
				return this.getQueryEngine().listSessions(p);
			}

			case "browser.overview": {
				const p = BrowserOverviewParamsSchema.parse(params);
				const overview = this.getQueryEngine().getOverview(p.sessionId, {
					include: p.include,
					aroundMarker: p.aroundMarker,
					timeRange: p.timeRange,
				});
				return renderSessionOverview(overview, p.tokenBudget ?? 3000);
			}

			case "browser.search": {
				const p = BrowserSearchParamsSchema.parse(params);
				const results = this.getQueryEngine().search(p.sessionId, {
					query: p.query,
					filters: {
						eventTypes: p.eventTypes,
						statusCodes: p.statusCodes,
						timeRange: p.timeRange,
					},
					maxResults: p.maxResults,
				});
				return renderSearchResults(results, p.tokenBudget ?? 2000);
			}

			case "browser.inspect": {
				const p = BrowserInspectParamsSchema.parse(params);
				const result = this.getQueryEngine().inspect(p.sessionId, {
					eventId: p.eventId,
					markerId: p.markerId,
					timestamp: p.timestamp !== undefined ? String(p.timestamp) : undefined,
					include: p.include,
					contextWindow: p.contextWindow,
				});
				return renderInspectResult(result, p.tokenBudget ?? 3000);
			}

			case "browser.diff": {
				const p = BrowserDiffParamsSchema.parse(params);
				const differ = new SessionDiffer(this.getQueryEngine());
				const diff = differ.diff({ sessionId: p.sessionId, before: p.before, after: p.after, include: p.include });
				return renderDiff(diff, p.tokenBudget ?? 2000);
			}

			case "browser.replay-context": {
				const p = BrowserReplayContextParamsSchema.parse(params);
				const generator = new ReplayContextGenerator(this.getQueryEngine());
				return generator.generate({
					sessionId: p.sessionId,
					aroundMarker: p.aroundMarker,
					timeRange: p.timeRange,
					format: p.format,
					testFramework: p.testFramework,
				});
			}

			case "browser.export": {
				const p = BrowserExportParamsSchema.parse(params);
				const exporter = new HARExporter(this.getQueryEngine());
				const harFile = exporter.export({
					sessionId: p.sessionId,
					timeRange: p.timeRange,
					includeResponseBodies: p.includeResponseBodies,
				});
				return JSON.stringify(harFile, null, 2);
			}

			default: {
				throw Object.assign(new Error(`Method not found: ${method}`), { rpcCode: RPC_METHOD_NOT_FOUND });
			}
		}
	}

	private getQueryEngine(): QueryEngine {
		if (!this.browserQueryEngine) {
			const dataDir = getKrometrailSubdir("browser");
			this.browserDb = new BrowserDatabase(resolve(dataDir, "index.db"));
			this.browserQueryEngine = new QueryEngine(this.browserDb, dataDir);
		}
		return this.browserQueryEngine;
	}

	/**
	 * Reset the idle timer. Called on every incoming request.
	 */
	private resetIdleTimer(): void {
		if (this.idleTimer) {
			clearTimeout(this.idleTimer);
			this.idleTimer = null;
		}

		this.idleTimer = setTimeout(() => {
			const sessions = this.sessionManager.listSessions();
			const browserActive = this.browserRecorder?.isRecording() ?? false;
			if (sessions.length === 0 && !browserActive) {
				this.shutdown()
					.then(() => process.exit(0))
					.catch(() => process.exit(1));
			}
		}, this.options.idleTimeoutMs);

		// Don't prevent process exit
		if (this.idleTimer.unref) {
			this.idleTimer.unref();
		}
	}

	/**
	 * Handle a new socket connection.
	 */
	private handleConnection(socket: Socket): void {
		this.activeConnections.add(socket);
		socket.on("close", () => this.activeConnections.delete(socket));

		let buffer = "";

		socket.on("data", (chunk: Buffer) => {
			buffer += chunk.toString("utf8");
			const lines = buffer.split("\n");
			buffer = lines.pop() ?? ""; // Keep last incomplete line in buffer

			for (const line of lines) {
				const trimmed = line.trim();
				if (!trimmed) continue;

				let request: JsonRpcRequest;
				try {
					request = JSON.parse(trimmed) as JsonRpcRequest;
				} catch {
					const errorResponse: JsonRpcResponse = {
						jsonrpc: "2.0",
						id: 0,
						error: { code: RPC_PARSE_ERROR, message: "Parse error: invalid JSON" },
					};
					socket.write(`${JSON.stringify(errorResponse)}\n`);
					continue;
				}

				// Validate basic JSON-RPC structure
				if (!request.jsonrpc || !request.method || request.id === undefined) {
					const errorResponse: JsonRpcResponse = {
						jsonrpc: "2.0",
						id: request.id ?? 0,
						error: { code: RPC_INVALID_REQUEST, message: "Invalid JSON-RPC request" },
					};
					socket.write(`${JSON.stringify(errorResponse)}\n`);
					continue;
				}

				// Dispatch asynchronously
				this.handleRequest(request)
					.then((response) => {
						if (!socket.destroyed) {
							socket.write(`${JSON.stringify(response)}\n`);
						}
					})
					.catch(() => {
						// handleRequest itself catches all errors; this path shouldn't happen
					});
			}
		});

		socket.on("error", () => {
			this.activeConnections.delete(socket);
		});
	}
}

/**
 * Entry point for spawning the daemon as a background process.
 */
export async function startDaemon(): Promise<void> {
	const { registerAllAdapters } = await import("../adapters/registry.js");
	const { createSessionManager } = await import("../core/session-manager.js");
	const { setupGracefulShutdown } = await import("../core/shutdown.js");
	const { getDaemonPidPath, getDaemonSocketPath } = await import("./protocol.js");

	registerAllAdapters();
	const sessionManager = createSessionManager();

	const server = new DaemonServer(sessionManager, {
		socketPath: getDaemonSocketPath(),
		pidPath: getDaemonPidPath(),
		idleTimeoutMs: 60_000,
	});

	await server.start();
	setupGracefulShutdown(() => server.shutdown());
}
