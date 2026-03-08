import type { EventType, RecordedEvent } from "../types.js";

interface PendingRequest {
	url: string;
	method: string;
	startTime: number;
}

/**
 * Transforms raw CDP events into the unified RecordedEvent format.
 * Handles network request/response correlation and event filtering.
 */
export class EventNormalizer {
	private pendingRequests = new Map<string, PendingRequest>(); // requestId → pending

	/** Process a raw CDP event and return a RecordedEvent, or null to skip. */
	normalize(method: string, params: Record<string, unknown>, tabId: string): RecordedEvent | null {
		switch (method) {
			case "Network.requestWillBeSent":
				return this.normalizeNetworkRequest(params, tabId);
			case "Network.responseReceived":
				return this.normalizeNetworkResponse(params, tabId);
			case "Network.loadingFailed":
				return this.normalizeNetworkFailed(params, tabId);
			case "Network.webSocketFrameSent":
				return this.normalizeWebSocketFrame(params, tabId, "SEND");
			case "Network.webSocketFrameReceived":
				return this.normalizeWebSocketFrame(params, tabId, "RECV");
			case "Runtime.consoleAPICalled":
				return this.normalizeConsole(params, tabId);
			case "Runtime.exceptionThrown":
				return this.normalizeException(params, tabId);
			case "Page.frameNavigated":
				return this.normalizeNavigation(params, tabId);
			case "Page.loadEventFired":
				return this.normalizeLoadEvent(tabId);
			case "Performance.metrics":
				return this.normalizePerformance(params, tabId);
			default:
				return null;
		}
	}

	private buildEvent(type: EventType, tabId: string, summary: string, data: Record<string, unknown>): RecordedEvent {
		return { id: crypto.randomUUID(), timestamp: Date.now(), type, tabId, summary, data };
	}

	private normalizeNetworkRequest(params: Record<string, unknown>, tabId: string): RecordedEvent | null {
		const requestId = params.requestId as string;
		const request = params.request as Record<string, unknown>;
		const url = request.url as string;
		const method = (request.method as string) ?? "GET";

		// Filter chrome-extension:// requests
		if (url.startsWith("chrome-extension://")) return null;

		this.pendingRequests.set(requestId, {
			url,
			method,
			startTime: Date.now(),
		});

		return this.buildEvent("network_request", tabId, `${method} ${url}`, {
			requestId,
			url,
			method,
			headers: request.headers ?? {},
			postData: request.postData,
		});
	}

	private normalizeNetworkResponse(params: Record<string, unknown>, tabId: string): RecordedEvent | null {
		const requestId = params.requestId as string;
		const response = params.response as Record<string, unknown>;
		const url = (response.url as string) ?? "";
		const status = (response.status as number) ?? 0;

		// Filter chrome-extension:// requests
		if (url.startsWith("chrome-extension://")) return null;

		const pending = this.pendingRequests.get(requestId);
		const method = pending?.method ?? "GET";
		const durationMs = pending ? Date.now() - pending.startTime : undefined;
		if (pending) this.pendingRequests.delete(requestId);

		const durationStr = durationMs !== undefined ? ` (${durationMs}ms)` : "";

		return this.buildEvent("network_response", tabId, `${status} ${method} ${url}${durationStr}`, {
			requestId,
			url,
			method,
			status,
			statusText: (response.statusText as string) ?? "",
			headers: response.headers ?? {},
			mimeType: response.mimeType,
			durationMs,
		});
	}

	private normalizeNetworkFailed(params: Record<string, unknown>, tabId: string): RecordedEvent | null {
		const requestId = params.requestId as string;
		const errorText = (params.errorText as string) ?? "Unknown error";

		const pending = this.pendingRequests.get(requestId);
		if (!pending) return null;

		const { url, method } = pending;
		if (url.startsWith("chrome-extension://")) return null;
		this.pendingRequests.delete(requestId);

		return this.buildEvent("network_response", tabId, `FAILED ${method} ${url}: ${errorText}`, {
			requestId,
			url,
			method,
			failed: true,
			errorText,
		});
	}

	private normalizeWebSocketFrame(params: Record<string, unknown>, tabId: string, direction: "SEND" | "RECV"): RecordedEvent {
		const response = params.response as Record<string, unknown> | undefined;
		const payload = ((response?.payloadData as string) ?? "").slice(0, 200);
		const url = (params.url as string) ?? "";

		return this.buildEvent("websocket", tabId, `WS ${direction}: ${payload}`, {
			url,
			direction,
			payload,
			requestId: params.requestId,
		});
	}

	private normalizeConsole(params: Record<string, unknown>, tabId: string): RecordedEvent | null {
		const args = (params.args as Array<Record<string, unknown>>) ?? [];
		// Filter __BL__ prefixed messages (input tracker events)
		if (args[0]?.value === "__BL__") return null;

		const type = (params.type as string) ?? "log";
		const level = this.mapConsoleLevel(type);
		const text = args
			.map((a) => a.value ?? a.description ?? JSON.stringify(a))
			.join(" ")
			.slice(0, 500);

		return this.buildEvent("console", tabId, `[${level}] ${text}`, {
			level,
			text,
			args: args.map((a) => ({ type: a.type, value: a.value ?? a.description })),
			stackTrace: params.stackTrace,
		});
	}

	private normalizeException(params: Record<string, unknown>, tabId: string): RecordedEvent {
		const detail = params.exceptionDetails as Record<string, unknown>;
		const exception = detail?.exception as Record<string, unknown> | undefined;
		const text = (exception?.description as string) ?? (detail?.text as string) ?? "Unknown exception";
		const stack = detail?.stackTrace as Record<string, unknown> | undefined;
		const frame = (stack?.callFrames as Array<Record<string, unknown>>)?.[0];
		const location = frame ? ` at ${frame.url}:${frame.lineNumber}` : "";
		const summary = `Uncaught ${text.split("\n")[0]}${location}`;

		return this.buildEvent("page_error", tabId, summary.slice(0, 300), {
			text,
			stackTrace: detail?.stackTrace,
			lineNumber: detail?.lineNumber,
			columnNumber: detail?.columnNumber,
			url: detail?.url,
		});
	}

	private normalizeNavigation(params: Record<string, unknown>, tabId: string): RecordedEvent {
		const frame = params.frame as Record<string, unknown>;
		const url = (frame?.url as string) ?? "";
		const name = (frame?.name as string) ?? "";

		// Only record main frame navigations (no parentId)
		const isMainFrame = !frame?.parentId;
		const summary = isMainFrame ? `Navigated to ${url}` : `Subframe navigated to ${url}${name ? ` (${name})` : ""}`;

		return this.buildEvent("navigation", tabId, summary, {
			url,
			frameId: frame?.id,
			isMainFrame,
		});
	}

	private normalizeLoadEvent(tabId: string): RecordedEvent {
		return this.buildEvent("navigation", tabId, "Page loaded (DOMContentLoaded)", { loadType: "load" });
	}

	private normalizePerformance(params: Record<string, unknown>, tabId: string): RecordedEvent {
		const metrics = (params.metrics as Array<{ name: string; value: number }>) ?? [];
		const parts: string[] = [];

		for (const m of metrics) {
			if (m.name === "LayoutDuration" && m.value > 0) {
				parts.push(`Layout: ${(m.value * 1000).toFixed(0)}ms`);
			} else if (m.name === "ScriptDuration" && m.value > 0) {
				parts.push(`Script: ${(m.value * 1000).toFixed(0)}ms`);
			} else if (m.name === "TaskDuration" && m.value > 0) {
				parts.push(`Task: ${(m.value * 1000).toFixed(0)}ms`);
			}
		}

		const summary = parts.length > 0 ? parts.join(", ") : "Performance metrics";

		return this.buildEvent("performance", tabId, summary, {
			metrics: Object.fromEntries(metrics.map((m) => [m.name, m.value])),
		});
	}

	private mapConsoleLevel(cdpType: string): string {
		switch (cdpType) {
			case "error":
				return "error";
			case "warning":
				return "warn";
			case "info":
				return "info";
			case "debug":
				return "debug";
			default:
				return "log";
		}
	}
}
