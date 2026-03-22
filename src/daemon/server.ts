import { unlinkSync, writeFileSync } from "node:fs";
import type { Server, Socket } from "node:net";
import { createServer } from "node:net";
import { resolve } from "node:path";
import { ScenarioStore } from "../browser/executor/scenario-store.js";
import { QueryEngine } from "../browser/investigation/query-engine.js";
import type { BrowserRecorder } from "../browser/recorder/index.js";
import { BrowserDatabase } from "../browser/storage/database.js";
import { AdapterNotFoundError, AdapterPrerequisiteError, KrometrailError, LaunchError, SessionLimitError, SessionNotFoundError, SessionStateError } from "../core/errors.js";
import { getKrometrailSubdir } from "../core/paths.js";
import type { SessionManager } from "../core/session-manager.js";
import { handleBrowserMethod } from "./browser-handlers.js";
import type { JsonRpcRequest, JsonRpcResponse } from "./protocol.js";
import {
	RPC_ADAPTER_ERROR,
	RPC_INTERNAL_ERROR,
	RPC_INVALID_REQUEST,
	RPC_LAUNCH_ERROR,
	RPC_METHOD_NOT_FOUND,
	RPC_PARSE_ERROR,
	RPC_SESSION_LIMIT_ERROR,
	RPC_SESSION_NOT_FOUND,
	RPC_SESSION_STATE_ERROR,
} from "./protocol.js";
import { handleSessionMethod } from "./session-handlers.js";

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
	private browserStartPromise: Promise<unknown> | undefined;
	private browserQueryEngine: QueryEngine | null = null;
	private browserDb: BrowserDatabase | null = null;
	private scenarioStore = new ScenarioStore();

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
		if (err instanceof AdapterPrerequisiteError) {
			return {
				code: RPC_ADAPTER_ERROR,
				message: err.message,
				data: { installHint: err.installHint, missing: err.missing, adapterId: err.adapterId, fixCommand: err.fixCommand },
			};
		}
		if (err instanceof AdapterNotFoundError) {
			return { code: RPC_ADAPTER_ERROR, message: err.message };
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
	 * Lazy-initialize the browser query engine.
	 */
	private getQueryEngine(): QueryEngine {
		if (!this.browserQueryEngine) {
			const dataDir = getKrometrailSubdir("browser");
			this.browserDb = new BrowserDatabase(resolve(dataDir, "index.db"));
			this.browserQueryEngine = new QueryEngine(this.browserDb, dataDir);
		}
		return this.browserQueryEngine;
	}

	/**
	 * Dispatch a validated RPC method call to the appropriate handler.
	 */
	private async dispatch(method: string, params: Record<string, unknown>): Promise<unknown> {
		// Try session handlers
		if (method.startsWith("session.")) {
			return handleSessionMethod(method, params, this.sessionManager);
		}

		// Try browser handlers
		if (method.startsWith("browser.")) {
			const self = this;
			return handleBrowserMethod(method, params, {
				recorder: this.browserRecorder,
				scenarioStore: this.scenarioStore,
				getQueryEngine: () => this.getQueryEngine(),
				setRecorder: (r) => {
					this.browserRecorder = r;
				},
				resetIdleTimer: () => this.resetIdleTimer(),
				get startPromise() {
					return self.browserStartPromise;
				},
				set startPromise(p: Promise<unknown> | undefined) {
					self.browserStartPromise = p;
				},
			});
		}

		// Daemon control
		switch (method) {
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

			default: {
				throw Object.assign(new Error(`Method not found: ${method}`), { rpcCode: RPC_METHOD_NOT_FOUND });
			}
		}
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
