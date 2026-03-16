import { beforeEach, describe, expect, it, vi } from "vitest";
import { StepExecutor, type StepExecutorPort } from "../../../../src/browser/executor/step-executor.js";
import type { Step } from "../../../../src/browser/executor/types.js";

function makeMockPort(): StepExecutorPort {
	return {
		evaluate: vi.fn().mockResolvedValue("result"),
		navigate: vi.fn().mockResolvedValue(undefined),
		reload: vi.fn().mockResolvedValue(undefined),
		click: vi.fn().mockResolvedValue(undefined),
		fill: vi.fn().mockResolvedValue(undefined),
		select: vi.fn().mockResolvedValue(undefined),
		submit: vi.fn().mockResolvedValue(undefined),
		type: vi.fn().mockResolvedValue(undefined),
		hover: vi.fn().mockResolvedValue(undefined),
		scrollTo: vi.fn().mockResolvedValue(undefined),
		scrollBy: vi.fn().mockResolvedValue(undefined),
		waitFor: vi.fn().mockResolvedValue(undefined),
		waitForNavigation: vi.fn().mockResolvedValue(undefined),
		waitForNetworkIdle: vi.fn().mockResolvedValue(undefined),
		captureScreenshot: vi.fn().mockResolvedValue("/path/to/screenshot.jpg"),
		placeMarker: vi.fn().mockResolvedValue("marker-id-1"),
	};
}

describe("StepExecutor", () => {
	let port: StepExecutorPort;
	let executor: StepExecutor;

	beforeEach(() => {
		port = makeMockPort();
		executor = new StepExecutor(port);
	});

	describe("sequential execution", () => {
		it("runs all steps in order", async () => {
			const steps: Step[] = [
				{ action: "navigate", url: "/login" },
				{ action: "fill", selector: "#email", value: "test@example.com" },
				{ action: "click", selector: "#submit" },
			];

			const result = await executor.execute(steps);
			expect(result.totalSteps).toBe(3);
			expect(result.completedSteps).toBe(3);
			expect(result.results).toHaveLength(3);
			expect(result.results[0].status).toBe("ok");
			expect(result.results[1].status).toBe("ok");
			expect(result.results[2].status).toBe("ok");

			expect(port.navigate).toHaveBeenCalledWith("/login");
			expect(port.fill).toHaveBeenCalledWith("#email", "test@example.com");
			expect(port.click).toHaveBeenCalledWith("#submit");
		});

		it("sets correct action and index on each result", async () => {
			const steps: Step[] = [{ action: "navigate", url: "/" }, { action: "reload" }];
			const result = await executor.execute(steps);
			expect(result.results[0].index).toBe(1);
			expect(result.results[0].action).toBe("navigate");
			expect(result.results[1].index).toBe(2);
			expect(result.results[1].action).toBe("reload");
		});
	});

	describe("stop-on-first-error", () => {
		it("stops on first error with partial results", async () => {
			const clickError = new Error("Element not found: #missing");
			(port.click as ReturnType<typeof vi.fn>).mockRejectedValueOnce(clickError);

			const steps: Step[] = [
				{ action: "navigate", url: "/login" },
				{ action: "click", selector: "#missing" },
				{ action: "fill", selector: "#email", value: "test" },
			];

			const result = await executor.execute(steps);
			expect(result.totalSteps).toBe(3);
			expect(result.completedSteps).toBe(1);
			expect(result.results).toHaveLength(2);
			expect(result.results[0].status).toBe("ok");
			expect(result.results[1].status).toBe("error");
			expect(result.results[1].error).toContain("Element not found");
			// fill was never called
			expect(port.fill).not.toHaveBeenCalled();
		});

		it("includes error message in failed result", async () => {
			(port.navigate as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error("Navigation failed"));
			const steps: Step[] = [{ action: "navigate", url: "/fail" }];
			const result = await executor.execute(steps);
			expect(result.results[0].error).toBe("Navigation failed");
		});
	});

	describe("capture modes", () => {
		it("mode all: takes screenshot after each action", async () => {
			const steps: Step[] = [
				{ action: "navigate", url: "/" },
				{ action: "click", selector: "#btn" },
			];
			await executor.execute(steps, { screenshot: "all" });
			expect(port.captureScreenshot).toHaveBeenCalledTimes(2);
		});

		it("mode none: takes no auto-screenshots", async () => {
			const steps: Step[] = [
				{ action: "navigate", url: "/" },
				{ action: "click", selector: "#btn" },
			];
			await executor.execute(steps, { screenshot: "none" });
			// Only called during explicit screenshot steps — none here
			expect(port.captureScreenshot).not.toHaveBeenCalled();
		});

		it("mode on_error: no screenshot on success", async () => {
			const steps: Step[] = [{ action: "navigate", url: "/" }];
			await executor.execute(steps, { screenshot: "on_error" });
			expect(port.captureScreenshot).not.toHaveBeenCalled();
		});

		it("mode on_error: takes screenshot on failure", async () => {
			(port.navigate as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error("fail"));
			const steps: Step[] = [{ action: "navigate", url: "/" }];
			await executor.execute(steps, { screenshot: "on_error" });
			expect(port.captureScreenshot).toHaveBeenCalledTimes(1);
		});

		it("mode all: takes screenshot on failure too", async () => {
			(port.navigate as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error("fail"));
			const steps: Step[] = [{ action: "navigate", url: "/" }];
			await executor.execute(steps, { screenshot: "all" });
			// Error path screenshot
			expect(port.captureScreenshot).toHaveBeenCalledTimes(1);
		});
	});

	describe("per-step screenshot override", () => {
		it("screenshot: false suppresses auto-screenshot for that step", async () => {
			const steps: Step[] = [
				{ action: "fill", selector: "#email", value: "test", screenshot: false },
				{ action: "click", selector: "#btn" },
			];
			await executor.execute(steps, { screenshot: "all" });
			// Only the click step should trigger a screenshot, not fill
			expect(port.captureScreenshot).toHaveBeenCalledTimes(1);
		});

		it("screenshot and mark actions never trigger auto-screenshot", async () => {
			const steps: Step[] = [{ action: "screenshot" }, { action: "mark", label: "test" }];
			await executor.execute(steps, { screenshot: "all" });
			// screenshot action calls captureScreenshot once (explicit), mark calls placeMarker
			// auto-screenshot is suppressed for both
			expect(port.captureScreenshot).toHaveBeenCalledTimes(1); // 1 from explicit screenshot step
		});
	});

	describe("auto-markers", () => {
		it("places marker before each step when markers enabled", async () => {
			const steps: Step[] = [
				{ action: "navigate", url: "/" },
				{ action: "click", selector: "#btn" },
			];
			await executor.execute(steps, { markers: true });
			expect(port.placeMarker).toHaveBeenCalledTimes(2);
		});

		it("does not place auto-markers when markers: false", async () => {
			const steps: Step[] = [{ action: "navigate", url: "/" }];
			await executor.execute(steps, { markers: false });
			expect(port.placeMarker).not.toHaveBeenCalled();
		});

		it("uses correct auto-marker label format: step:N:action:detail", async () => {
			const steps: Step[] = [{ action: "navigate", url: "/login" }];
			await executor.execute(steps, { markers: true });
			const markerLabel = (port.placeMarker as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
			expect(markerLabel).toMatch(/^step:1:navigate:/);
		});
	});

	describe("evaluate step", () => {
		it("returns value in result returnValue", async () => {
			(port.evaluate as ReturnType<typeof vi.fn>).mockResolvedValueOnce("42");
			const steps: Step[] = [{ action: "evaluate", expression: "1 + 1" }];
			const result = await executor.execute(steps);
			expect(result.results[0].returnValue).toBe("42");
			expect(port.evaluate).toHaveBeenCalledWith("1 + 1");
		});
	});

	describe("all action types dispatched correctly", () => {
		it("dispatches reload", async () => {
			await executor.execute([{ action: "reload" }]);
			expect(port.reload).toHaveBeenCalled();
		});

		it("dispatches select", async () => {
			await executor.execute([{ action: "select", selector: "#sel", value: "opt1" }]);
			expect(port.select).toHaveBeenCalledWith("#sel", "opt1");
		});

		it("dispatches submit", async () => {
			await executor.execute([{ action: "submit", selector: "form" }]);
			expect(port.submit).toHaveBeenCalledWith("form");
		});

		it("dispatches type with default delay 50", async () => {
			await executor.execute([{ action: "type", selector: "#in", text: "hi" }]);
			expect(port.type).toHaveBeenCalledWith("#in", "hi", 50);
		});

		it("dispatches type with custom delay", async () => {
			await executor.execute([{ action: "type", selector: "#in", text: "hi", delay_ms: 100 }]);
			expect(port.type).toHaveBeenCalledWith("#in", "hi", 100);
		});

		it("dispatches hover", async () => {
			await executor.execute([{ action: "hover", selector: ".menu" }]);
			expect(port.hover).toHaveBeenCalledWith(".menu");
		});

		it("dispatches scroll_to", async () => {
			await executor.execute([{ action: "scroll_to", selector: "#footer" }]);
			expect(port.scrollTo).toHaveBeenCalledWith("#footer");
		});

		it("dispatches scroll_by with defaults", async () => {
			await executor.execute([{ action: "scroll_by" }]);
			expect(port.scrollBy).toHaveBeenCalledWith(0, 0);
		});

		it("dispatches wait_for with defaults", async () => {
			await executor.execute([{ action: "wait_for", selector: ".el" }]);
			expect(port.waitFor).toHaveBeenCalledWith(".el", "visible", 5000);
		});

		it("dispatches wait_for_navigation with defaults", async () => {
			await executor.execute([{ action: "wait_for_navigation" }]);
			expect(port.waitForNavigation).toHaveBeenCalledWith(undefined, 10000);
		});

		it("dispatches wait_for_network_idle with defaults", async () => {
			await executor.execute([{ action: "wait_for_network_idle" }]);
			expect(port.waitForNetworkIdle).toHaveBeenCalledWith(500, 10000);
		});
	});

	describe("result metadata", () => {
		it("includes durationMs for each step", async () => {
			const steps: Step[] = [{ action: "navigate", url: "/" }];
			const result = await executor.execute(steps);
			expect(result.results[0].durationMs).toBeTypeOf("number");
			expect(result.results[0].durationMs).toBeGreaterThanOrEqual(0);
		});

		it("includes totalDurationMs", async () => {
			const result = await executor.execute([{ action: "navigate", url: "/" }]);
			expect(result.totalDurationMs).toBeTypeOf("number");
			expect(result.totalDurationMs).toBeGreaterThanOrEqual(0);
		});

		it("includes screenshotPath when screenshot taken", async () => {
			const result = await executor.execute([{ action: "navigate", url: "/" }], { screenshot: "all" });
			expect(result.results[0].screenshotPath).toBe("/path/to/screenshot.jpg");
		});

		it("includes markerId from auto-marker", async () => {
			const result = await executor.execute([{ action: "navigate", url: "/" }], { markers: true });
			expect(result.results[0].markerId).toBe("marker-id-1");
		});
	});
});
