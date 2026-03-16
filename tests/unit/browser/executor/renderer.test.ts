import { describe, expect, it } from "vitest";
import { renderStepResults } from "../../../../src/browser/executor/renderer.js";
import type { RunStepsResult, StepResult } from "../../../../src/browser/executor/types.js";

function makeStepResult(overrides: Partial<StepResult> = {}): StepResult {
	return {
		index: 1,
		action: "navigate",
		label: "navigate:/login",
		status: "ok",
		durationMs: 320,
		...overrides,
	};
}

function makeResult(overrides: Partial<RunStepsResult> = {}): RunStepsResult {
	return {
		totalSteps: 1,
		completedSteps: 1,
		results: [makeStepResult()],
		totalDurationMs: 350,
		...overrides,
	};
}

describe("renderStepResults", () => {
	it("renders all-success case", () => {
		const result = makeResult({
			totalSteps: 3,
			completedSteps: 3,
			results: [
				makeStepResult({ index: 1, action: "navigate", label: "navigate:/login", durationMs: 320 }),
				makeStepResult({ index: 2, action: "fill", label: "fill:#email", durationMs: 45 }),
				makeStepResult({ index: 3, action: "click", label: "click:#submit", durationMs: 12 }),
			],
			totalDurationMs: 2100,
			sessionId: "abc-123",
		});

		const output = renderStepResults(result);
		expect(output).toContain("3/3 completed");
		expect(output).toContain("2.1s total");
		expect(output).not.toContain("STOPPED");
		expect(output).toContain("navigate:/login");
		expect(output).toContain("✓");
		expect(output).toContain("abc-123");
		expect(output).toContain("session_overview");
	});

	it("renders partial-failure case with stop point and error", () => {
		const result = makeResult({
			totalSteps: 5,
			completedSteps: 3,
			results: [
				makeStepResult({ index: 1, action: "navigate", label: "navigate:/login", durationMs: 320 }),
				makeStepResult({ index: 2, action: "fill", label: "fill:#email", durationMs: 45 }),
				makeStepResult({ index: 3, action: "click", label: "click:#submit", durationMs: 12 }),
				makeStepResult({
					index: 4,
					action: "wait_for",
					label: "wait_for:.dashboard",
					durationMs: 5003,
					status: "error",
					error: 'Timeout waiting for selector ".dashboard" (5000ms)',
				}),
			],
			totalDurationMs: 1800,
			sessionId: "abc-123",
		});

		const output = renderStepResults(result);
		expect(output).toContain("3/5 completed");
		expect(output).toContain("STOPPED on step 4");
		expect(output).toContain("✗");
		expect(output).toContain('Timeout waiting for selector ".dashboard"');
	});

	it("renders evaluate step return value", () => {
		const result = makeResult({
			totalSteps: 1,
			completedSteps: 1,
			results: [
				makeStepResult({
					index: 1,
					action: "evaluate",
					label: "evaluate:document.title",
					durationMs: 12,
					returnValue: "My App",
				}),
			],
			totalDurationMs: 15,
			sessionId: "sess-1",
		});

		const output = renderStepResults(result);
		expect(output).toContain('"My App"');
	});

	it("renders screenshot filename (basename only, not full path)", () => {
		const result = makeResult({
			results: [
				makeStepResult({
					index: 1,
					screenshotPath: "/home/user/.krometrail/browser/sessions/abc/screenshots/1234567890.jpg",
				}),
			],
		});

		const output = renderStepResults(result);
		expect(output).toContain("1234567890.jpg");
		expect(output).not.toContain("/home/user/");
	});

	it("shows session ID at the bottom", () => {
		const result = makeResult({ sessionId: "test-session-id" });
		const output = renderStepResults(result);
		const lines = output.split("\n");
		const sessionLine = lines.find((l) => l.includes("test-session-id"));
		expect(sessionLine).toBeDefined();
	});

	it("omits session line when no sessionId", () => {
		const result = makeResult({ sessionId: undefined });
		const output = renderStepResults(result);
		expect(output).not.toContain("Session:");
	});
});
