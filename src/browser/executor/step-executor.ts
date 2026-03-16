import type { CaptureConfig, RunStepsResult, Step, StepResult } from "./types.js";

/** Port interface for CDP operations needed by the executor. */
export interface StepExecutorPort {
	/** Evaluate JS in the page and return the stringified result. */
	evaluate(expression: string): Promise<string>;
	/** Navigate to a URL. Resolves after page load. */
	navigate(url: string): Promise<void>;
	/** Reload the current page. */
	reload(): Promise<void>;
	/** Dispatch a mouse click at the center of the element matching selector. */
	click(selector: string): Promise<void>;
	/** Set value on an input/textarea element (triggers input + change events). */
	fill(selector: string, value: string): Promise<void>;
	/** Select an option by value in a <select> element. */
	select(selector: string, value: string): Promise<void>;
	/** Submit a form via requestSubmit(). */
	submit(selector: string): Promise<void>;
	/** Type text keystroke-by-keystroke with delay between keys. */
	type(selector: string, text: string, delayMs: number): Promise<void>;
	/** Dispatch a mouseover event on the element matching selector. */
	hover(selector: string): Promise<void>;
	/** Scroll element into view. */
	scrollTo(selector: string): Promise<void>;
	/** Scroll the page by delta pixels. */
	scrollBy(x: number, y: number): Promise<void>;
	/** Wait for an element matching selector to reach the given state. */
	waitFor(selector: string, state: "visible" | "hidden" | "attached", timeoutMs: number): Promise<void>;
	/** Wait for a navigation event (optionally matching a URL substring). */
	waitForNavigation(urlMatch: string | undefined, timeoutMs: number): Promise<void>;
	/** Wait for network to be idle (no requests for idleMs). */
	waitForNetworkIdle(idleMs: number, timeoutMs: number): Promise<void>;
	/** Capture a screenshot, return the file path. */
	captureScreenshot(label?: string): Promise<string>;
	/** Place a marker, return the marker ID. */
	placeMarker(label: string): Promise<string>;
}

export class StepExecutor {
	constructor(private port: StepExecutorPort) {}

	/**
	 * Execute a sequence of steps. Stops on first error.
	 * Returns results for all attempted steps.
	 */
	async execute(steps: Step[], capture?: CaptureConfig): Promise<RunStepsResult> {
		const screenshotMode = capture?.screenshot ?? "all";
		const autoMarkers = capture?.markers !== false;
		const results: StepResult[] = [];
		const overallStart = Date.now();

		for (let i = 0; i < steps.length; i++) {
			const step = steps[i];
			const stepStart = Date.now();
			const label = this.formatStepLabel(step);
			let markerId: string | undefined;
			let screenshotPath: string | undefined;
			let returnValue: string | undefined;

			try {
				// Auto-marker before action
				if (autoMarkers) {
					markerId = await this.port.placeMarker(`step:${i + 1}:${label}`);
				}

				// Execute the action
				returnValue = await this.executeStep(step);

				// Auto-screenshot after action
				const shouldScreenshot = this.shouldCapture(step, screenshotMode);
				if (shouldScreenshot) {
					screenshotPath = await this.port.captureScreenshot(`step:${i + 1}:${label}`);
				}

				results.push({
					index: i + 1,
					action: step.action,
					label,
					status: "ok",
					durationMs: Date.now() - stepStart,
					screenshotPath,
					markerId,
					returnValue,
				});
			} catch (err) {
				// Capture screenshot on error if configured
				if (screenshotMode === "on_error" || screenshotMode === "all") {
					try {
						screenshotPath = await this.port.captureScreenshot(`step:${i + 1}:error:${label}`);
					} catch {
						/* ignore screenshot failure on error */
					}
				}

				results.push({
					index: i + 1,
					action: step.action,
					label,
					status: "error",
					durationMs: Date.now() - stepStart,
					screenshotPath,
					markerId,
					error: err instanceof Error ? err.message : String(err),
				});
				break; // Stop on first error
			}
		}

		return {
			totalSteps: steps.length,
			completedSteps: results.filter((r) => r.status === "ok").length,
			results,
			totalDurationMs: Date.now() - overallStart,
		};
	}

	private async executeStep(step: Step): Promise<string | undefined> {
		switch (step.action) {
			case "navigate":
				await this.port.navigate(step.url);
				return undefined;
			case "reload":
				await this.port.reload();
				return undefined;
			case "click":
				await this.port.click(step.selector);
				return undefined;
			case "fill":
				await this.port.fill(step.selector, step.value);
				return undefined;
			case "select":
				await this.port.select(step.selector, step.value);
				return undefined;
			case "submit":
				await this.port.submit(step.selector);
				return undefined;
			case "type":
				await this.port.type(step.selector, step.text, step.delay_ms ?? 50);
				return undefined;
			case "hover":
				await this.port.hover(step.selector);
				return undefined;
			case "scroll_to":
				await this.port.scrollTo(step.selector);
				return undefined;
			case "scroll_by":
				await this.port.scrollBy(step.x ?? 0, step.y ?? 0);
				return undefined;
			case "wait":
				await new Promise((r) => setTimeout(r, step.ms));
				return undefined;
			case "wait_for":
				await this.port.waitFor(step.selector, step.state ?? "visible", step.timeout ?? 5000);
				return undefined;
			case "wait_for_navigation":
				await this.port.waitForNavigation(step.url, step.timeout ?? 10000);
				return undefined;
			case "wait_for_network_idle":
				await this.port.waitForNetworkIdle(step.idle_ms ?? 500, step.timeout ?? 10000);
				return undefined;
			case "screenshot": {
				const path = await this.port.captureScreenshot(step.label);
				return path || undefined;
			}
			case "mark": {
				const id = await this.port.placeMarker(step.label);
				return id || undefined;
			}
			case "evaluate":
				return await this.port.evaluate(step.expression);
		}
	}

	private shouldCapture(step: Step, mode: "all" | "none" | "on_error"): boolean {
		if (mode === "none") return false;
		if (mode === "on_error") return false;
		// mode === "all"
		if (step.action === "screenshot" || step.action === "mark") return false;
		if ("screenshot" in step && step.screenshot === false) return false;
		return true;
	}

	private formatStepLabel(step: Step): string {
		const truncate = (s: string, max = 40): string => (s.length > max ? `${s.slice(0, max - 1)}…` : s);

		switch (step.action) {
			case "navigate":
				return `navigate:${truncate(step.url)}`;
			case "reload":
				return "reload";
			case "click":
				return `click:${truncate(step.selector)}`;
			case "fill":
				return `fill:${truncate(step.selector)}`;
			case "select":
				return `select:${truncate(step.selector)}`;
			case "submit":
				return `submit:${truncate(step.selector)}`;
			case "type":
				return `type:${truncate(step.selector)}`;
			case "hover":
				return `hover:${truncate(step.selector)}`;
			case "scroll_to":
				return `scroll_to:${truncate(step.selector)}`;
			case "scroll_by":
				return `scroll_by:${step.x ?? 0},${step.y ?? 0}`;
			case "wait":
				return `wait:${step.ms}ms`;
			case "wait_for":
				return `wait_for:${truncate(step.selector)}`;
			case "wait_for_navigation":
				return step.url ? `wait_for_navigation:${truncate(step.url)}` : "wait_for_navigation";
			case "wait_for_network_idle":
				return "wait_for_network_idle";
			case "screenshot":
				return step.label ? `screenshot:${truncate(step.label)}` : "screenshot";
			case "mark":
				return `mark:${truncate(step.label)}`;
			case "evaluate":
				return `evaluate:${truncate(step.expression)}`;
		}
	}
}
