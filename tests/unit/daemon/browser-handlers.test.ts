import { describe, expect, it, vi, beforeEach } from "vitest";
import { BrowserRecorderStateError } from "../../../src/core/errors.js";

// Mock the dynamic import of BrowserRecorder
const mockStart = vi.fn();
const mockStop = vi.fn();
const mockIsRecording = vi.fn();
let mockOnAutoStop: (() => void) | null = null;

vi.mock("../../../src/browser/recorder/index.js", () => {
	function BrowserRecorder(_opts: unknown) {
		this.start = mockStart;
		this.stop = mockStop;
		this.isRecording = mockIsRecording;
		Object.defineProperty(this, "onAutoStop", {
			get() {
				return mockOnAutoStop;
			},
			set(fn: (() => void) | null) {
				mockOnAutoStop = fn;
			},
			configurable: true,
		});
	}
	return { BrowserRecorder };
});

// Import AFTER mocking
const { handleBrowserMethod } = await import("../../../src/daemon/browser-handlers.js");

function createMockState(recorder: unknown = null) {
	return {
		recorder: recorder as any,
		scenarioStore: {} as any,
		getQueryEngine: vi.fn() as any,
		setRecorder: vi.fn(),
		resetIdleTimer: vi.fn(),
	};
}

const VALID_START_PARAMS = {
	port: 9222,
	attach: false,
	allTabs: false,
};

beforeEach(() => {
	vi.clearAllMocks();
	mockOnAutoStop = null;
	mockStart.mockResolvedValue({ id: "test-session", startedAt: Date.now(), tabs: [], eventCount: 0, markerCount: 0, bufferAgeMs: 0 });
	mockIsRecording.mockReturnValue(false);
});

describe("browser.start recorder lifecycle", () => {
	it("returns existing session info when recorder is already recording (idempotent)", async () => {
		mockIsRecording.mockReturnValue(true);
		const mockSessionInfo = { id: "existing", startedAt: 1000, tabs: [], eventCount: 5, markerCount: 0, bufferAgeMs: 100 };
		const staleRecorder = { isRecording: mockIsRecording, stop: mockStop, getSessionInfo: vi.fn().mockReturnValue(mockSessionInfo) };
		const state = createMockState(staleRecorder);

		const result = await handleBrowserMethod("browser.start", VALID_START_PARAMS, state);
		expect(result).toBe(mockSessionInfo);
		expect(state.setRecorder).not.toHaveBeenCalled();
	});

	it("cleans up stale non-recording recorder before creating new one", async () => {
		const staleRecorder = { isRecording: vi.fn().mockReturnValue(false), stop: vi.fn().mockResolvedValue(undefined) };
		const state = createMockState(staleRecorder);

		await handleBrowserMethod("browser.start", VALID_START_PARAMS, state);

		// Stale recorder should have been stopped
		expect(staleRecorder.stop).toHaveBeenCalled();
		// setRecorder(null) called for cleanup, then setRecorder(newRecorder) after start
		expect(state.setRecorder).toHaveBeenCalledWith(null);
	});

	it("ignores errors when cleaning up stale recorder", async () => {
		const staleRecorder = { isRecording: vi.fn().mockReturnValue(false), stop: vi.fn().mockRejectedValue(new Error("cleanup failed")) };
		const state = createMockState(staleRecorder);

		// Should NOT throw even though stale stop() failed
		await handleBrowserMethod("browser.start", VALID_START_PARAMS, state);
		expect(staleRecorder.stop).toHaveBeenCalled();
	});

	it("does not register recorder if start() throws", async () => {
		mockStart.mockRejectedValue(new Error("Chrome failed to launch"));
		const state = createMockState(null);

		await expect(handleBrowserMethod("browser.start", VALID_START_PARAMS, state)).rejects.toThrow("Chrome failed to launch");

		// setRecorder should never have been called with a recorder
		// It may have been called with null for stale cleanup, but not with a recorder instance
		for (const call of state.setRecorder.mock.calls) {
			expect(call[0]).toBeNull();
		}
	});

	it("registers recorder only after successful start()", async () => {
		const state = createMockState(null);

		await handleBrowserMethod("browser.start", VALID_START_PARAMS, state);

		// setRecorder should have been called once with the new recorder
		const nonNullCalls = state.setRecorder.mock.calls.filter((c: any[]) => c[0] !== null);
		expect(nonNullCalls).toHaveLength(1);
		// And start() should have been called before setRecorder
		expect(mockStart).toHaveBeenCalledTimes(1);
	});
});
