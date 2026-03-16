import { z } from "zod";

// --- Action Schemas ---

export const STEP_ACTIONS = [
	"navigate",
	"reload",
	"click",
	"fill",
	"select",
	"submit",
	"type",
	"hover",
	"scroll_to",
	"scroll_by",
	"wait",
	"wait_for",
	"wait_for_navigation",
	"wait_for_network_idle",
	"screenshot",
	"mark",
	"evaluate",
] as const;
export const StepActionSchema = z.enum(STEP_ACTIONS);
export type StepAction = z.infer<typeof StepActionSchema>;

// Navigation
const NavigateStepSchema = z.object({
	action: z.literal("navigate"),
	url: z.string().describe("URL to navigate to (absolute or relative to current origin)"),
	screenshot: z.boolean().optional(),
});
const ReloadStepSchema = z.object({
	action: z.literal("reload"),
	screenshot: z.boolean().optional(),
});

// Input
const ClickStepSchema = z.object({
	action: z.literal("click"),
	selector: z.string().describe("CSS selector of element to click"),
	screenshot: z.boolean().optional(),
});
const FillStepSchema = z.object({
	action: z.literal("fill"),
	selector: z.string().describe("CSS selector of input/textarea element"),
	value: z.string().describe("Value to set"),
	screenshot: z.boolean().optional(),
});
const SelectStepSchema = z.object({
	action: z.literal("select"),
	selector: z.string().describe("CSS selector of <select> element"),
	value: z.string().describe("Option value to select"),
	screenshot: z.boolean().optional(),
});
const SubmitStepSchema = z.object({
	action: z.literal("submit"),
	selector: z.string().describe("CSS selector of <form> element"),
	screenshot: z.boolean().optional(),
});
const TypeStepSchema = z.object({
	action: z.literal("type"),
	selector: z.string().describe("CSS selector of element to type into"),
	text: z.string().describe("Text to type keystroke-by-keystroke"),
	delay_ms: z.number().optional().describe("Delay between keystrokes in ms. Default: 50"),
	screenshot: z.boolean().optional(),
});
const HoverStepSchema = z.object({
	action: z.literal("hover"),
	selector: z.string().describe("CSS selector of element to hover over"),
	screenshot: z.boolean().optional(),
});

// Scroll
const ScrollToStepSchema = z.object({
	action: z.literal("scroll_to"),
	selector: z.string().describe("CSS selector of element to scroll into view"),
	screenshot: z.boolean().optional(),
});
const ScrollByStepSchema = z.object({
	action: z.literal("scroll_by"),
	x: z.number().optional().describe("Horizontal scroll delta in pixels. Default: 0"),
	y: z.number().optional().describe("Vertical scroll delta in pixels. Default: 0"),
	screenshot: z.boolean().optional(),
});

// Waiting
const WaitStepSchema = z.object({
	action: z.literal("wait"),
	ms: z.number().describe("Milliseconds to wait"),
	screenshot: z.boolean().optional(),
});
const WaitForStepSchema = z.object({
	action: z.literal("wait_for"),
	selector: z.string().describe("CSS selector to wait for"),
	state: z.enum(["visible", "hidden", "attached"]).optional().describe("Element state to wait for. Default: visible"),
	timeout: z.number().optional().describe("Timeout in ms. Default: 5000"),
	screenshot: z.boolean().optional(),
});
const WaitForNavigationStepSchema = z.object({
	action: z.literal("wait_for_navigation"),
	url: z.string().optional().describe("URL substring to match. If omitted, waits for any navigation."),
	timeout: z.number().optional().describe("Timeout in ms. Default: 10000"),
	screenshot: z.boolean().optional(),
});
const WaitForNetworkIdleStepSchema = z.object({
	action: z.literal("wait_for_network_idle"),
	idle_ms: z.number().optional().describe("Required idle period in ms. Default: 500"),
	timeout: z.number().optional().describe("Timeout in ms. Default: 10000"),
	screenshot: z.boolean().optional(),
});

// Capture (explicit — beyond auto-capture)
const ScreenshotStepSchema = z.object({
	action: z.literal("screenshot"),
	label: z.string().optional().describe("Label for the screenshot"),
});
const MarkStepSchema = z.object({
	action: z.literal("mark"),
	label: z.string().describe("Label for the marker"),
});

// Evaluation
const EvaluateStepSchema = z.object({
	action: z.literal("evaluate"),
	expression: z.string().describe("JavaScript expression to evaluate in the page context"),
	screenshot: z.boolean().optional(),
});

// --- Discriminated Union ---

export const StepSchema = z.discriminatedUnion("action", [
	NavigateStepSchema,
	ReloadStepSchema,
	ClickStepSchema,
	FillStepSchema,
	SelectStepSchema,
	SubmitStepSchema,
	TypeStepSchema,
	HoverStepSchema,
	ScrollToStepSchema,
	ScrollByStepSchema,
	WaitStepSchema,
	WaitForStepSchema,
	WaitForNavigationStepSchema,
	WaitForNetworkIdleStepSchema,
	ScreenshotStepSchema,
	MarkStepSchema,
	EvaluateStepSchema,
]);
export type Step = z.infer<typeof StepSchema>;

// --- Capture Config ---

export const CAPTURE_SCREENSHOT_MODES = ["all", "none", "on_error"] as const;
export const CaptureScreenshotModeSchema = z.enum(CAPTURE_SCREENSHOT_MODES);

export const CaptureConfigSchema = z.object({
	screenshot: CaptureScreenshotModeSchema.optional().describe('When to auto-screenshot each step. Default: "all"'),
	markers: z.boolean().optional().describe("Auto-place a marker at each step. Default: true"),
});
export type CaptureConfig = z.infer<typeof CaptureConfigSchema>;

// --- Run Steps Params ---

export const RunStepsParamsSchema = z.object({
	steps: z.array(StepSchema).optional().describe("Ordered actions to execute. Required unless replaying a named scenario."),
	name: z.string().optional().describe("Name for saving or replaying a scenario"),
	save: z.boolean().optional().describe("Save steps under the given name for later replay. Requires name."),
	capture: CaptureConfigSchema.optional().describe("Capture configuration for all steps"),
});
export type RunStepsParams = z.infer<typeof RunStepsParamsSchema>;

// --- Step Result ---

export interface StepResult {
	index: number;
	action: StepAction;
	/** Short description of the step, e.g. "navigate /login" */
	label: string;
	status: "ok" | "error";
	durationMs: number;
	/** Screenshot file path, if captured */
	screenshotPath?: string;
	/** Marker ID, if placed */
	markerId?: string;
	/** Error message, if failed */
	error?: string;
	/** Return value from evaluate steps */
	returnValue?: string;
}

export interface RunStepsResult {
	/** Total steps attempted */
	totalSteps: number;
	/** Steps that completed successfully */
	completedSteps: number;
	/** Per-step results */
	results: StepResult[];
	/** Session ID of the recording (if auto-started or already active) */
	sessionId?: string;
	/** Total execution time */
	totalDurationMs: number;
}
