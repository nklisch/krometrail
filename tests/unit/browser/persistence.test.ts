import { existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BufferConfigSchema, RollingBuffer } from "../../../src/browser/recorder/rolling-buffer.js";
import { PersistencePipeline } from "../../../src/browser/storage/persistence.js";
import { ScreenshotCapture, ScreenshotConfigSchema } from "../../../src/browser/storage/screenshot.js";
import type { BrowserSessionInfo, RecordedEvent } from "../../../src/browser/types.js";

// Minimal CDPClient mock
function makeCdpClient() {
	return {
		sendToTarget: vi.fn().mockResolvedValue({ data: "" }),
	} as unknown as import("../../../src/browser/recorder/cdp-client.js").CDPClient;
}

let dataDir: string;
let pipeline: PersistencePipeline;
let buffer: RollingBuffer;
let cdpClient: ReturnType<typeof makeCdpClient>;

function makeSessionInfo(id = "sess1"): BrowserSessionInfo {
	return {
		id,
		startedAt: 1709826622000,
		tabs: [{ targetId: "t1", url: "https://example.com", title: "Example" }],
		eventCount: 0,
		markerCount: 0,
		bufferAgeMs: 0,
	};
}

function makeEvent(id: string, timestamp: number): RecordedEvent {
	return {
		id,
		timestamp,
		type: "console",
		tabId: "t1",
		summary: `event ${id}`,
		data: {},
	};
}

beforeEach(() => {
	dataDir = resolve(tmpdir(), "agent-lens-persist-test-" + crypto.randomUUID());
	mkdirSync(dataDir, { recursive: true });
	pipeline = new PersistencePipeline({ dataDir, markerPaddingMs: 5000 });
	buffer = new RollingBuffer(BufferConfigSchema.parse({}));
	cdpClient = makeCdpClient();
});

afterEach(() => {
	pipeline.close();
});

describe("PersistencePipeline", () => {
	it("marker placement creates session directory structure", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();
		buffer.push(makeEvent("e1", now - 1000));
		const marker = buffer.placeMarker("test marker", false);

		await pipeline.onMarkerPlaced(marker, buffer, sessionInfo, cdpClient, "tab-session-1");

		const sessDir = pipeline.getSessionDir("sess1");
		expect(sessDir).toBeTruthy();
		expect(existsSync(sessDir!)).toBe(true);
		expect(existsSync(resolve(sessDir!, "network"))).toBe(true);
		expect(existsSync(resolve(sessDir!, "screenshots"))).toBe(true);
		expect(existsSync(resolve(sessDir!, "events.jsonl"))).toBe(true);
	});

	it("marker triggers flush of past-window events", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();

		// Events in window
		buffer.push(makeEvent("e1", now - 4000));
		buffer.push(makeEvent("e2", now - 2000));
		// Event outside window (>5000ms before marker)
		buffer.push(makeEvent("e3", now - 6000));

		const marker = buffer.placeMarker(undefined, false);
		await pipeline.onMarkerPlaced(marker, buffer, sessionInfo, cdpClient, "tab1");

		const sessDir = pipeline.getSessionDir("sess1")!;
		const content = readFileSync(resolve(sessDir, "events.jsonl"), "utf-8");
		const lines = content.trim().split("\n").filter(Boolean);

		// e1 and e2 are in window, e3 is outside
		expect(lines.some((l) => l.includes('"e1"'))).toBe(true);
		expect(lines.some((l) => l.includes('"e2"'))).toBe(true);
		expect(lines.some((l) => l.includes('"e3"'))).toBe(false);
	});

	it("already-persisted events are not duplicated", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();
		buffer.push(makeEvent("e1", now - 1000));

		const marker1 = buffer.placeMarker(undefined, false);
		await pipeline.onMarkerPlaced(marker1, buffer, sessionInfo, cdpClient, "tab1");

		const marker2 = buffer.placeMarker("second", false);
		await pipeline.onMarkerPlaced(marker2, buffer, sessionInfo, cdpClient, "tab1");

		const sessDir = pipeline.getSessionDir("sess1")!;
		const content = readFileSync(resolve(sessDir, "events.jsonl"), "utf-8");
		const occurrences = (content.match(/"e1"/g) ?? []).length;
		expect(occurrences).toBe(1);
	});

	it("future-window events are persisted via onNewEvent", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();
		buffer.push(makeEvent("e1", now - 1000));

		const marker = buffer.placeMarker(undefined, false);
		await pipeline.onMarkerPlaced(marker, buffer, sessionInfo, cdpClient, "tab1");

		// New event within future window
		const futureEvent = makeEvent("e-future", now + 100);
		buffer.push(futureEvent);
		pipeline.onNewEvent(futureEvent, sessionInfo);

		const sessDir = pipeline.getSessionDir("sess1")!;
		const content = readFileSync(resolve(sessDir, "events.jsonl"), "utf-8");
		expect(content).toContain("e-future");
	});

	it("onNewEvent ignores events when no session exists", () => {
		const event = makeEvent("e1", Date.now());
		// Should not throw
		expect(() => pipeline.onNewEvent(event, makeSessionInfo())).not.toThrow();
	});

	it("onNewEvent ignores events when no open windows", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();
		buffer.push(makeEvent("e1", now - 1000));

		// Place marker with very short window (already closed)
		const shortPipeline = new PersistencePipeline({ dataDir, markerPaddingMs: 0 });
		const marker = buffer.placeMarker(undefined, false);
		await shortPipeline.onMarkerPlaced(marker, buffer, sessionInfo, cdpClient, "tab1");

		const futureEvent = makeEvent("e-future", now + 1000);
		shortPipeline.onNewEvent(futureEvent, sessionInfo);

		const sessDir = shortPipeline.getSessionDir("sess1")!;
		const content = readFileSync(resolve(sessDir, "events.jsonl"), "utf-8");
		expect(content).not.toContain("e-future");
		shortPipeline.close();
	});

	it("getSessionDir returns null before any marker is placed", () => {
		expect(pipeline.getSessionDir("nonexistent")).toBeNull();
	});

	it("captures a screenshot on marker placement", async () => {
		const sessionInfo = makeSessionInfo();
		const now = Date.now();
		buffer.push(makeEvent("e1", now - 1000));

		// Mock CDP to return a valid PNG
		const mockCdp = {
			sendToTarget: vi.fn().mockResolvedValue({ data: Buffer.alloc(10).toString("base64") }),
		} as unknown as import("../../../src/browser/recorder/cdp-client.js").CDPClient;

		// Pipeline with screenshot capture enabled
		const screenshotCapture = new ScreenshotCapture(ScreenshotConfigSchema.parse({ onMarker: true }));
		const pipelineWithScreenshots = new PersistencePipeline({ dataDir, markerPaddingMs: 5000 }, screenshotCapture);

		const marker = buffer.placeMarker(undefined, false);
		await pipelineWithScreenshots.onMarkerPlaced(marker, buffer, sessionInfo, mockCdp, "tab1");

		const screenshotDir = resolve(pipelineWithScreenshots.getSessionDir("sess1")!, "screenshots");
		const files = readdirSync(screenshotDir);
		expect(files.length).toBeGreaterThan(0);
		pipelineWithScreenshots.close();
	});
});
