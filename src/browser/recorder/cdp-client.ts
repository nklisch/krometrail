import { EventEmitter } from "node:events";
import { CDPConnectionError } from "../../core/errors.js";

export interface CDPClientOptions {
	/** Chrome CDP WebSocket URL, e.g. ws://localhost:9222/json/version */
	browserWsUrl: string;
	/** Reconnect on disconnect. Default: true */
	autoReconnect: boolean;
	/** Max reconnect attempts. Default: 10 */
	maxReconnectAttempts: number;
	/** Reconnect delay in ms. Default: 1000 */
	reconnectDelayMs: number;
}

interface PendingRequest {
	resolve: (value: unknown) => void;
	reject: (reason: unknown) => void;
}

/**
 * Fetches the browser-level CDP WebSocket URL from Chrome's HTTP endpoint.
 * Returns the webSocketDebuggerUrl from /json/version.
 */
export async function fetchBrowserWsUrl(port: number): Promise<string> {
	const response = await fetch(`http://localhost:${port}/json/version`);
	if (!response.ok) {
		throw new CDPConnectionError(`Chrome CDP HTTP endpoint returned ${response.status}`);
	}
	const info = (await response.json()) as { webSocketDebuggerUrl?: string };
	if (!info.webSocketDebuggerUrl) {
		throw new CDPConnectionError("Chrome CDP endpoint did not return webSocketDebuggerUrl");
	}
	return info.webSocketDebuggerUrl;
}

/**
 * CDP client using Chrome's flat session model (Chrome 74+).
 *
 * Browser-level commands: send({ id, method, params })
 * Tab session commands:   send({ id, method, params, sessionId })
 * Events are emitted as: ("event", sessionId, method, params)
 * Browser-level events:  ("event", "", method, params)
 */
export class CDPClient extends EventEmitter {
	private ws: WebSocket | null = null;
	private requestId = 0;
	private pending = new Map<number, PendingRequest>();
	private connected = false;
	private reconnectAttempts = 0;
	private reconnecting = false;

	constructor(private options: CDPClientOptions) {
		super();
	}

	/** Connect to the browser's CDP WebSocket endpoint. */
	async connect(): Promise<void> {
		return new Promise<void>((resolve, reject) => {
			const ws = new WebSocket(this.options.browserWsUrl);
			this.ws = ws;

			ws.onopen = () => {
				this.connected = true;
				this.reconnectAttempts = 0;
				this.reconnecting = false;
				resolve();
			};

			ws.onerror = () => {
				if (!this.connected) {
					reject(new CDPConnectionError(`Failed to connect to Chrome CDP at ${this.options.browserWsUrl}`));
				}
				// If already connected, onclose will fire next
			};

			ws.onclose = () => {
				const wasConnected = this.connected;
				this.connected = false;
				this.rejectPending(new CDPConnectionError("CDP WebSocket closed"));
				if (wasConnected) {
					this.scheduleReconnect();
				}
			};

			ws.onmessage = (event) => {
				this.onMessage(event.data as string);
			};
		});
	}

	/** Send a CDP command and wait for the response. */
	async send(method: string, params?: Record<string, unknown>): Promise<unknown> {
		const id = ++this.requestId;
		return new Promise<unknown>((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.ws?.send(JSON.stringify({ id, method, params: params ?? {} }));
		});
	}

	/** Subscribe to a CDP domain (e.g., "Network.enable"). */
	async enableDomain(domain: string, params?: Record<string, unknown>): Promise<void> {
		await this.send(`${domain}.enable`, params);
	}

	/** Create a session for a specific target (tab). Returns sessionId. */
	async attachToTarget(targetId: string): Promise<string> {
		const result = (await this.send("Target.attachToTarget", { targetId, flatten: true })) as { sessionId: string };
		return result.sessionId;
	}

	/** Send a command to a specific target session (flat session model). */
	async sendToTarget(sessionId: string, method: string, params?: Record<string, unknown>): Promise<unknown> {
		const id = ++this.requestId;
		return new Promise<unknown>((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.ws?.send(JSON.stringify({ id, method, params: params ?? {}, sessionId }));
		});
	}

	/** Disconnect and clean up. */
	async disconnect(): Promise<void> {
		this.options.autoReconnect = false;
		this.ws?.close();
		this.ws = null;
		this.connected = false;
	}

	/** Whether the client is currently connected. */
	isConnected(): boolean {
		return this.connected;
	}

	private onMessage(data: string): void {
		let message: Record<string, unknown>;
		try {
			message = JSON.parse(data) as Record<string, unknown>;
		} catch {
			return;
		}

		const sessionId = (message.sessionId as string | undefined) ?? "";

		if (typeof message.id === "number") {
			// Command response (browser-level or session-level)
			const pending = this.pending.get(message.id);
			if (pending) {
				this.pending.delete(message.id);
				if (message.error) {
					const errInfo = message.error as { message?: string };
					pending.reject(new Error(errInfo.message ?? "CDP error"));
				} else {
					pending.resolve(message.result);
				}
			}
		} else if (typeof message.method === "string") {
			// Event (browser-level or session-level)
			this.emit("event", sessionId, message.method, (message.params ?? {}) as Record<string, unknown>);
		}
	}

	private rejectPending(err: Error): void {
		for (const { reject } of this.pending.values()) {
			reject(err);
		}
		this.pending.clear();
	}

	private scheduleReconnect(): void {
		if (!this.options.autoReconnect || this.reconnecting) return;
		if (this.reconnectAttempts >= this.options.maxReconnectAttempts) {
			this.emit("disconnected");
			return;
		}
		this.reconnecting = true;
		this.reconnectAttempts++;
		setTimeout(async () => {
			try {
				await this.connect();
				this.emit("reconnected");
			} catch {
				this.reconnecting = false;
				this.scheduleReconnect();
			}
		}, this.options.reconnectDelayMs);
	}
}
