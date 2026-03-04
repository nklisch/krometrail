import type { Socket } from "node:net";
import { createConnection } from "node:net";
import type { Readable, Writable } from "node:stream";
import type { DebugProtocol } from "@vscode/debugprotocol";
import { DAPClientDisposedError, DAPConnectionError, DAPTimeoutError } from "./errors.js";

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
export type StopResult = { type: "stopped"; event: DebugProtocol.StoppedEvent } | { type: "terminated"; event: DebugProtocol.TerminatedEvent } | { type: "exited"; event: DebugProtocol.ExitedEvent };

export class DAPClient {
	private seq = 1;
	private pendingRequests = new Map<
		number,
		{
			resolve: (response: DebugProtocol.Response) => void;
			reject: (error: Error) => void;
			timer: ReturnType<typeof setTimeout>;
		}
	>();
	private eventHandlers = new Map<string, ((event: DebugProtocol.Event) => void)[]>();
	private buffer = Buffer.alloc(0);
	private options: DAPClientOptions;
	private reader: Readable | null = null;
	private writer: Writable | null = null;
	private socket: Socket | null = null;
	private _capabilities: DebugProtocol.Capabilities | null = null;
	private _connected = false;
	private _disposed = false;

	constructor(options?: Partial<DAPClientOptions>) {
		this.options = { ...DEFAULT_DAP_CLIENT_OPTIONS, ...options };
	}

	/** The DAP server's capabilities, available after initialize(). */
	get capabilities(): DebugProtocol.Capabilities {
		if (!this._capabilities) throw new Error("DAP client not initialized");
		return this._capabilities;
	}

	/** Whether the client is connected to a DAP server. */
	get connected(): boolean {
		return this._connected;
	}

	/**
	 * Connect to a DAP server over TCP.
	 * Resolves when the TCP connection is established.
	 */
	async connect(host: string, port: number): Promise<void> {
		return new Promise((resolve, reject) => {
			const socket = createConnection({ host, port });
			socket.once("connect", () => {
				this.socket = socket;
				this.attachStreams(socket, socket);
				resolve();
			});
			socket.once("error", (err) => {
				reject(new DAPConnectionError(host, port, err));
			});
		});
	}

	/**
	 * Attach to existing streams (for testing or non-TCP transports).
	 */
	attachStreams(reader: Readable, writer: Writable): void {
		this.reader = reader;
		this.writer = writer;
		this._connected = true;
		this.reader.on("data", (chunk: Buffer) => this.onData(chunk));
	}

	/**
	 * Disconnect from the DAP server, closing the underlying transport.
	 */
	disconnect(): void {
		if (this.socket) {
			this.socket.destroy();
			this.socket = null;
		}
		this._connected = false;
	}

	/**
	 * Send the DAP `initialize` request and store capabilities from the response.
	 * Does NOT wait for the `initialized` event — the caller must handle event timing.
	 * For standard adapters, `initialized` arrives quickly after the response.
	 * For debugpy.adapter, `initialized` only arrives after `launch` is sent.
	 */
	async initialize(): Promise<DebugProtocol.Capabilities> {
		const response = await this.send<DebugProtocol.InitializeResponse>("initialize", {
			clientID: "agent-lens",
			adapterID: "agent-lens",
			supportsVariableType: true,
			linesStartAt1: true,
			columnsStartAt1: true,
		});

		this._capabilities = response.body ?? {};

		return this._capabilities;
	}

	/**
	 * Send a DAP request and wait for the correlated response.
	 * Rejects after requestTimeoutMs with a descriptive error.
	 */
	async send<T extends DebugProtocol.Response>(command: string, args?: Record<string, unknown>): Promise<T> {
		if (this._disposed) throw new DAPClientDisposedError();

		const seq = this.seq++;
		const request: DebugProtocol.Request = {
			seq,
			type: "request",
			command,
			arguments: args,
		};

		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this.pendingRequests.delete(seq);
				reject(new DAPTimeoutError(command, this.options.requestTimeoutMs));
			}, this.options.requestTimeoutMs);

			this.pendingRequests.set(seq, {
				resolve: resolve as (r: DebugProtocol.Response) => void,
				reject,
				timer,
			});
			this.writeMessage(request);
		});
	}

	/**
	 * Register an event handler for a specific DAP event type.
	 */
	on(event: string, handler: (event: DebugProtocol.Event) => void): void {
		const handlers = this.eventHandlers.get(event) ?? [];
		handlers.push(handler);
		this.eventHandlers.set(event, handlers);
	}

	/**
	 * Remove an event handler.
	 */
	off(event: string, handler: (event: DebugProtocol.Event) => void): void {
		const handlers = this.eventHandlers.get(event);
		if (!handlers) return;
		const idx = handlers.indexOf(handler);
		if (idx !== -1) handlers.splice(idx, 1);
	}

	/**
	 * Wait for the debugee to stop (breakpoint, step, exception, pause)
	 * or terminate. Returns a discriminated union.
	 * Rejects after timeoutMs (default: options.stopTimeoutMs).
	 */
	waitForStop(timeoutMs?: number): Promise<StopResult> {
		const timeout = timeoutMs ?? this.options.stopTimeoutMs;
		return new Promise((resolve, reject) => {
			let resolved = false;
			let timer: ReturnType<typeof setTimeout>;

			const cleanup = () => {
				this.off("stopped", stoppedHandler);
				this.off("terminated", terminatedHandler);
				this.off("exited", exitedHandler);
				clearTimeout(timer);
			};

			const stoppedHandler = (event: DebugProtocol.Event) => {
				if (resolved) return;
				resolved = true;
				cleanup();
				resolve({ type: "stopped", event: event as DebugProtocol.StoppedEvent });
			};

			const terminatedHandler = (event: DebugProtocol.Event) => {
				if (resolved) return;
				resolved = true;
				cleanup();
				resolve({ type: "terminated", event: event as DebugProtocol.TerminatedEvent });
			};

			const exitedHandler = (event: DebugProtocol.Event) => {
				if (resolved) return;
				resolved = true;
				cleanup();
				resolve({ type: "exited", event: event as DebugProtocol.ExitedEvent });
			};

			this.on("stopped", stoppedHandler);
			this.on("terminated", terminatedHandler);
			this.on("exited", exitedHandler);

			timer = setTimeout(() => {
				if (resolved) return;
				resolved = true;
				cleanup();
				reject(new DAPTimeoutError("waitForStop", timeout));
			}, timeout);
		});
	}

	// --- Typed Request Helpers ---

	/** DAP configurationDone — signals end of breakpoint configuration phase. */
	configurationDone(): Promise<DebugProtocol.ConfigurationDoneResponse> {
		return this.send("configurationDone");
	}

	/** DAP launch — launch the debugee. */
	launch(args: DebugProtocol.LaunchRequestArguments): Promise<DebugProtocol.LaunchResponse> {
		return this.send("launch", args as Record<string, unknown>);
	}

	/** DAP setBreakpoints — set breakpoints in a single source file. */
	setBreakpoints(source: DebugProtocol.Source, breakpoints: DebugProtocol.SourceBreakpoint[]): Promise<DebugProtocol.SetBreakpointsResponse> {
		return this.send("setBreakpoints", { source, breakpoints });
	}

	/** DAP setExceptionBreakpoints — configure exception breakpoint filters. */
	setExceptionBreakpoints(filters: string[]): Promise<DebugProtocol.SetExceptionBreakpointsResponse> {
		return this.send("setExceptionBreakpoints", { filters });
	}

	/** DAP continue — resume execution. */
	continue(threadId: number): Promise<DebugProtocol.ContinueResponse> {
		return this.send("continue", { threadId });
	}

	/** DAP next — step over. */
	next(threadId: number): Promise<DebugProtocol.NextResponse> {
		return this.send("next", { threadId });
	}

	/** DAP stepIn — step into. */
	stepIn(threadId: number): Promise<DebugProtocol.StepInResponse> {
		return this.send("stepIn", { threadId });
	}

	/** DAP stepOut — step out. */
	stepOut(threadId: number): Promise<DebugProtocol.StepOutResponse> {
		return this.send("stepOut", { threadId });
	}

	/** DAP stackTrace — get the call stack for a thread. */
	stackTrace(threadId: number, startFrame?: number, levels?: number): Promise<DebugProtocol.StackTraceResponse> {
		return this.send("stackTrace", { threadId, startFrame, levels });
	}

	/** DAP scopes — get scopes for a stack frame. */
	scopes(frameId: number): Promise<DebugProtocol.ScopesResponse> {
		return this.send("scopes", { frameId });
	}

	/** DAP variables — get variables for a scope/variable reference. */
	variables(variablesReference: number): Promise<DebugProtocol.VariablesResponse> {
		return this.send("variables", { variablesReference });
	}

	/** DAP evaluate — evaluate an expression in a frame context. */
	evaluate(expression: string, frameId?: number, context?: "watch" | "repl" | "hover" | "clipboard"): Promise<DebugProtocol.EvaluateResponse> {
		return this.send("evaluate", { expression, frameId, context: context ?? "repl" });
	}

	/** DAP disconnect — ask the debug adapter to disconnect. */
	sendDisconnect(terminateDebuggee?: boolean): Promise<DebugProtocol.DisconnectResponse> {
		return this.send("disconnect", { terminateDebuggee });
	}

	/** DAP terminate — ask the debugee to terminate gracefully. */
	terminate(): Promise<DebugProtocol.TerminateResponse> {
		return this.send("terminate");
	}

	/** DAP threads — get all threads. */
	threads(): Promise<DebugProtocol.ThreadsResponse> {
		return this.send("threads");
	}

	/**
	 * Dispose the client: reject all pending requests, clear handlers,
	 * disconnect transport.
	 */
	dispose(): void {
		this._disposed = true;
		for (const [, { reject, timer }] of this.pendingRequests) {
			clearTimeout(timer);
			reject(new DAPClientDisposedError());
		}
		this.pendingRequests.clear();
		this.eventHandlers.clear();
		this.disconnect();
	}

	private writeMessage(message: DebugProtocol.ProtocolMessage): void {
		if (!this.writer) throw new Error("DAP client not connected");
		const json = JSON.stringify(message);
		const header = `Content-Length: ${Buffer.byteLength(json)}\r\n\r\n`;
		this.writer.write(header + json);
	}

	private onData(chunk: Buffer): void {
		this.buffer = Buffer.concat([this.buffer, chunk]);
		this.processBuffer();
	}

	private processBuffer(): void {
		while (true) {
			const headerEnd = this.buffer.indexOf("\r\n\r\n");
			if (headerEnd === -1) return;

			const header = this.buffer.subarray(0, headerEnd).toString();
			const match = header.match(/Content-Length:\s*(\d+)/i);
			if (!match) {
				// Skip malformed header
				this.buffer = this.buffer.subarray(headerEnd + 4);
				continue;
			}

			const contentLength = Number.parseInt(match[1], 10);
			const messageStart = headerEnd + 4;
			const messageEnd = messageStart + contentLength;

			if (this.buffer.length < messageEnd) return; // Need more data

			const body = this.buffer.subarray(messageStart, messageEnd).toString();
			this.buffer = this.buffer.subarray(messageEnd);

			try {
				const message = JSON.parse(body) as DebugProtocol.ProtocolMessage;
				this.handleMessage(message);
			} catch {
				// Skip malformed JSON
			}
		}
	}

	private handleMessage(message: DebugProtocol.ProtocolMessage): void {
		if (message.type === "response") {
			const response = message as DebugProtocol.Response;
			const pending = this.pendingRequests.get(response.request_seq);
			if (pending) {
				clearTimeout(pending.timer);
				this.pendingRequests.delete(response.request_seq);
				if (response.success) {
					pending.resolve(response);
				} else {
					pending.reject(new Error(response.message ?? `DAP error: ${response.command}`));
				}
			}
		} else if (message.type === "event") {
			const event = message as DebugProtocol.Event;
			const handlers = this.eventHandlers.get(event.event);
			if (handlers) {
				for (const handler of [...handlers]) {
					handler(event);
				}
			}
		}
	}
}
