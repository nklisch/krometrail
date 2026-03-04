import type { Readable, Writable } from "node:stream";
import type { DebugProtocol } from "@vscode/debugprotocol";

/**
 * Minimal DAP client that speaks the Debug Adapter Protocol over streams.
 *
 * DAP uses a simple framing protocol: each message is preceded by
 * `Content-Length: <n>\r\n\r\n` followed by <n> bytes of JSON.
 *
 * The client handles:
 * - Request/response correlation via sequence IDs
 * - Event dispatching
 * - Stream framing (Content-Length parsing)
 */
export class DAPClient {
	private seq = 1;
	private pendingRequests = new Map<
		number,
		{
			resolve: (response: DebugProtocol.Response) => void;
			reject: (error: Error) => void;
		}
	>();
	private eventHandlers = new Map<string, ((event: DebugProtocol.Event) => void)[]>();
	private buffer = Buffer.alloc(0);

	constructor(
		private reader: Readable,
		private writer: Writable,
	) {
		this.reader.on("data", (chunk: Buffer) => this.onData(chunk));
	}

	async send<T extends DebugProtocol.Response>(
		command: string,
		args?: Record<string, unknown>,
	): Promise<T> {
		const seq = this.seq++;
		const request: DebugProtocol.Request = {
			seq,
			type: "request",
			command,
			arguments: args,
		};

		return new Promise((resolve, reject) => {
			this.pendingRequests.set(seq, {
				resolve: resolve as (r: DebugProtocol.Response) => void,
				reject,
			});
			this.writeMessage(request);
		});
	}

	on(event: string, handler: (event: DebugProtocol.Event) => void): void {
		const handlers = this.eventHandlers.get(event) ?? [];
		handlers.push(handler);
		this.eventHandlers.set(event, handlers);
	}

	dispose(): void {
		for (const [, { reject }] of this.pendingRequests) {
			reject(new Error("DAP client disposed"));
		}
		this.pendingRequests.clear();
	}

	private writeMessage(message: DebugProtocol.ProtocolMessage): void {
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
				for (const handler of handlers) {
					handler(event);
				}
			}
		}
	}
}
