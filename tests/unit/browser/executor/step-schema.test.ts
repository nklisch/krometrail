import { describe, expect, it } from "vitest";
import { RunStepsParamsSchema, STEP_ACTIONS, StepSchema } from "../../../../src/browser/executor/types.js";

describe("StepSchema", () => {
	it("parses all 17 action types", () => {
		const validSteps = [
			{ action: "navigate", url: "https://example.com" },
			{ action: "reload" },
			{ action: "click", selector: "#btn" },
			{ action: "fill", selector: "#input", value: "hello" },
			{ action: "select", selector: "#sel", value: "option1" },
			{ action: "submit", selector: "form" },
			{ action: "type", selector: "#input", text: "hello" },
			{ action: "hover", selector: ".menu" },
			{ action: "scroll_to", selector: "#target" },
			{ action: "scroll_by", x: 0, y: 100 },
			{ action: "wait", ms: 500 },
			{ action: "wait_for", selector: ".spinner" },
			{ action: "wait_for_navigation" },
			{ action: "wait_for_network_idle" },
			{ action: "screenshot" },
			{ action: "mark", label: "my marker" },
			{ action: "evaluate", expression: "document.title" },
		];

		expect(STEP_ACTIONS).toHaveLength(17);

		for (const step of validSteps) {
			expect(() => StepSchema.parse(step)).not.toThrow();
		}
	});

	it("rejects unknown action types", () => {
		expect(() => StepSchema.parse({ action: "unknown_action" })).toThrow();
	});

	it("validates per-step screenshot override", () => {
		// screenshot: false is valid on navigate
		expect(() => StepSchema.parse({ action: "navigate", url: "/login", screenshot: false })).not.toThrow();
		// screenshot: true is valid on click
		expect(() => StepSchema.parse({ action: "click", selector: "#btn", screenshot: true })).not.toThrow();
		// screenshot key is not on mark/screenshot actions (those only have label)
		const markStep = StepSchema.parse({ action: "mark", label: "test" });
		expect("screenshot" in markStep).toBe(false);
	});

	it("parse navigate with relative URL", () => {
		const step = StepSchema.parse({ action: "navigate", url: "/dashboard" });
		expect(step.action).toBe("navigate");
		if (step.action === "navigate") {
			expect(step.url).toBe("/dashboard");
		}
	});

	it("parses type step with optional delay_ms", () => {
		const step = StepSchema.parse({ action: "type", selector: "#input", text: "hello", delay_ms: 100 });
		expect(step.action).toBe("type");
		if (step.action === "type") {
			expect(step.delay_ms).toBe(100);
		}
	});

	it("parses wait_for with optional state and timeout", () => {
		const step = StepSchema.parse({ action: "wait_for", selector: ".el", state: "hidden", timeout: 8000 });
		expect(step.action).toBe("wait_for");
		if (step.action === "wait_for") {
			expect(step.state).toBe("hidden");
			expect(step.timeout).toBe(8000);
		}
	});
});

describe("RunStepsParamsSchema", () => {
	it("accepts steps array", () => {
		const result = RunStepsParamsSchema.parse({
			steps: [{ action: "navigate", url: "/login" }],
		});
		expect(result.steps).toHaveLength(1);
	});

	it("accepts name only (for replay)", () => {
		const result = RunStepsParamsSchema.parse({ name: "login-flow" });
		expect(result.name).toBe("login-flow");
		expect(result.steps).toBeUndefined();
	});

	it("accepts name + save + steps", () => {
		const result = RunStepsParamsSchema.parse({
			name: "login-flow",
			save: true,
			steps: [{ action: "navigate", url: "/login" }],
		});
		expect(result.name).toBe("login-flow");
		expect(result.save).toBe(true);
	});

	it("accepts capture config", () => {
		const result = RunStepsParamsSchema.parse({
			steps: [{ action: "reload" }],
			capture: { screenshot: "on_error", markers: false },
		});
		expect(result.capture?.screenshot).toBe("on_error");
		expect(result.capture?.markers).toBe(false);
	});

	it("accepts empty object (steps and name both optional at schema level)", () => {
		// The schema allows empty — validation of 'steps or name required' is in the handler
		expect(() => RunStepsParamsSchema.parse({})).not.toThrow();
	});
});
