import { beforeEach, describe, expect, it } from "vitest";
import { EventNormalizer } from "../../../src/browser/recorder/event-normalizer.js";

describe("EventNormalizer", () => {
	let normalizer: EventNormalizer;

	beforeEach(() => {
		normalizer = new EventNormalizer();
	});

	describe("Network.requestWillBeSent", () => {
		it("produces network_request event", () => {
			const event = normalizer.normalize(
				"Network.requestWillBeSent",
				{
					requestId: "req1",
					request: { url: "https://api.example.com/users", method: "GET", headers: {} },
				},
				"tab1",
			);

			expect(event).not.toBeNull();
			expect(event?.type).toBe("network_request");
			expect(event?.tabId).toBe("tab1");
			expect(event?.summary).toBe("GET https://api.example.com/users");
			expect(event?.data.requestId).toBe("req1");
			expect(event?.data.url).toBe("https://api.example.com/users");
			expect(event?.data.method).toBe("GET");
		});

		it("filters chrome-extension:// requests", () => {
			const event = normalizer.normalize(
				"Network.requestWillBeSent",
				{
					requestId: "req-ext",
					request: { url: "chrome-extension://abc123/background.js", method: "GET", headers: {} },
				},
				"tab1",
			);
			expect(event).toBeNull();
		});
	});

	describe("Network.responseReceived", () => {
		it("correlates with request and includes duration", () => {
			// First send a request
			normalizer.normalize("Network.requestWillBeSent", { requestId: "req2", request: { url: "https://api.example.com/users", method: "POST", headers: {} } }, "tab1");

			const event = normalizer.normalize(
				"Network.responseReceived",
				{
					requestId: "req2",
					response: { url: "https://api.example.com/users", status: 200, statusText: "OK", headers: {}, mimeType: "application/json" },
				},
				"tab1",
			);

			expect(event).not.toBeNull();
			expect(event?.type).toBe("network_response");
			expect(event?.data.status).toBe(200);
			expect(event?.data.method).toBe("POST");
			expect(event?.data.durationMs).toBeDefined();
			expect(typeof event?.data.durationMs).toBe("number");
		});

		it("summary includes status and duration", () => {
			normalizer.normalize("Network.requestWillBeSent", { requestId: "req3", request: { url: "https://example.com/api", method: "GET", headers: {} } }, "tab1");
			const event = normalizer.normalize(
				"Network.responseReceived",
				{
					requestId: "req3",
					response: { url: "https://example.com/api", status: 404, statusText: "Not Found", headers: {}, mimeType: "text/plain" },
				},
				"tab1",
			);

			expect(event?.summary).toMatch(/^404 GET/);
			expect(event?.summary).toMatch(/\d+ms\)/);
		});
	});

	describe("Network.loadingFailed", () => {
		it("produces failed network_response event", () => {
			normalizer.normalize("Network.requestWillBeSent", { requestId: "req4", request: { url: "https://example.com/fail", method: "GET", headers: {} } }, "tab1");
			const event = normalizer.normalize("Network.loadingFailed", { requestId: "req4", errorText: "net::ERR_CONNECTION_REFUSED" }, "tab1");

			expect(event).not.toBeNull();
			expect(event?.type).toBe("network_response");
			expect(event?.data.failed).toBe(true);
			expect(event?.summary).toContain("FAILED");
			expect(event?.summary).toContain("ERR_CONNECTION_REFUSED");
		});

		it("returns null for unknown requestId", () => {
			const event = normalizer.normalize("Network.loadingFailed", { requestId: "unknown", errorText: "error" }, "tab1");
			expect(event).toBeNull();
		});
	});

	describe("Runtime.consoleAPICalled", () => {
		it("maps console.error to error level", () => {
			const event = normalizer.normalize(
				"Runtime.consoleAPICalled",
				{
					type: "error",
					args: [{ type: "string", value: "Something went wrong" }],
				},
				"tab1",
			);

			expect(event?.type).toBe("console");
			expect(event?.data.level).toBe("error");
			expect(event?.summary).toContain("[error]");
			expect(event?.summary).toContain("Something went wrong");
		});

		it("maps console.log to log level", () => {
			const event = normalizer.normalize(
				"Runtime.consoleAPICalled",
				{
					type: "log",
					args: [{ type: "string", value: "Hello" }],
				},
				"tab1",
			);

			expect(event?.data.level).toBe("log");
		});

		it("maps console.warning to warn level", () => {
			const event = normalizer.normalize(
				"Runtime.consoleAPICalled",
				{
					type: "warning",
					args: [{ type: "string", value: "Deprecated" }],
				},
				"tab1",
			);

			expect(event?.data.level).toBe("warn");
		});

		it("filters __BL__ prefixed messages", () => {
			const event = normalizer.normalize(
				"Runtime.consoleAPICalled",
				{
					type: "debug",
					args: [
						{ type: "string", value: "__BL__" },
						{ type: "string", value: '{"type":"click","ts":123}' },
					],
				},
				"tab1",
			);

			expect(event).toBeNull();
		});
	});

	describe("Runtime.exceptionThrown", () => {
		it("produces page_error event", () => {
			const event = normalizer.normalize(
				"Runtime.exceptionThrown",
				{
					exceptionDetails: {
						text: "Uncaught TypeError",
						exception: {
							type: "object",
							description: "TypeError: Cannot read properties of null\n    at app.js:42",
						},
						stackTrace: {
							callFrames: [{ url: "app.js", lineNumber: 42, columnNumber: 5, functionName: "handleClick" }],
						},
					},
				},
				"tab1",
			);

			expect(event?.type).toBe("page_error");
			expect(event?.summary).toContain("Uncaught TypeError");
			expect(event?.summary).toContain("app.js");
		});
	});

	describe("Page.frameNavigated", () => {
		it("produces navigation event for main frame", () => {
			const event = normalizer.normalize(
				"Page.frameNavigated",
				{
					frame: { id: "frame1", url: "https://example.com/dashboard", name: "" },
				},
				"tab1",
			);

			expect(event?.type).toBe("navigation");
			expect(event?.summary).toContain("https://example.com/dashboard");
			expect(event?.data.isMainFrame).toBe(true);
		});

		it("marks sub-frames as non-main", () => {
			const event = normalizer.normalize(
				"Page.frameNavigated",
				{
					frame: { id: "frame2", parentId: "frame1", url: "https://ads.example.com/", name: "ad" },
				},
				"tab1",
			);

			expect(event?.data.isMainFrame).toBe(false);
		});
	});

	describe("Page.loadEventFired", () => {
		it("produces navigation event", () => {
			const event = normalizer.normalize("Page.loadEventFired", {}, "tab1");

			expect(event?.type).toBe("navigation");
			expect(event?.summary).toContain("DOMContentLoaded");
		});
	});

	describe("Network.webSocketFrameSent/Received", () => {
		it("produces websocket event for sent frame", () => {
			const event = normalizer.normalize(
				"Network.webSocketFrameSent",
				{
					requestId: "ws1",
					url: "wss://example.com/ws",
					response: { payloadData: '{"type":"ping"}' },
				},
				"tab1",
			);

			expect(event?.type).toBe("websocket");
			expect(event?.summary).toContain("WS SEND");
			expect(event?.summary).toContain("ping");
		});

		it("produces websocket event for received frame", () => {
			const event = normalizer.normalize(
				"Network.webSocketFrameReceived",
				{
					requestId: "ws1",
					url: "wss://example.com/ws",
					response: { payloadData: '{"type":"pong"}' },
				},
				"tab1",
			);

			expect(event?.type).toBe("websocket");
			expect(event?.summary).toContain("WS RECV");
		});
	});

	it("returns null for unknown CDP events", () => {
		expect(normalizer.normalize("Profiler.someEvent", {}, "tab1")).toBeNull();
		expect(normalizer.normalize("Network.dataReceived", { requestId: "r1", dataLength: 100, encodedDataLength: 100 }, "tab1")).toBeNull();
	});
});
