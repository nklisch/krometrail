import { StepExecutionError } from "../../core/errors.js";
import type { CDPClient } from "../recorder/cdp-client.js";
import type { BrowserRecorder } from "../recorder/index.js";
import type { ScreenshotCapture } from "../storage/screenshot.js";
import type { StepExecutorPort } from "./step-executor.js";

export interface CDPAdapterConfig {
	cdpClient: CDPClient;
	tabSessionId: string;
	recorder: BrowserRecorder;
	screenshotCapture: ScreenshotCapture | null;
	screenshotDir: string | null;
}

type EvaluateResult = {
	result?: { value?: unknown };
	exceptionDetails?: { text?: string; exception?: { description?: string } };
};

export class CDPPortAdapter implements StepExecutorPort {
	constructor(private config: CDPAdapterConfig) {}

	async evaluate(expression: string): Promise<string> {
		const res = (await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Runtime.evaluate", {
			expression,
			returnByValue: true,
		})) as EvaluateResult;

		if (res.exceptionDetails) {
			const msg = res.exceptionDetails.exception?.description ?? res.exceptionDetails.text ?? "JS evaluation failed";
			throw new StepExecutionError(0, "evaluate", undefined, msg);
		}

		return String(res.result?.value ?? "");
	}

	async navigate(url: string): Promise<void> {
		let resolvedUrl = url;
		if (url.startsWith("/")) {
			const origin = await this.evaluate("location.origin");
			resolvedUrl = `${origin}${url}`;
		}

		await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Page.navigate", { url: resolvedUrl });

		// Wait for load event or timeout after 10s
		await this.waitForLoad(10_000);
	}

	async reload(): Promise<void> {
		await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Page.reload", {});
		await this.waitForLoad(10_000);
	}

	async click(selector: string): Promise<void> {
		await this.evaluateThrow(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				el.click();
			})()`,
			selector,
		);
		await new Promise((r) => setTimeout(r, 100));
	}

	async fill(selector: string, value: string): Promise<void> {
		await this.evaluateThrow(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				el.focus();
				const nativeSetter = Object.getOwnPropertyDescriptor(
					el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
					'value'
				)?.set;
				if (nativeSetter) { nativeSetter.call(el, ${JSON.stringify(value)}); }
				else { el.value = ${JSON.stringify(value)}; }
				el.dispatchEvent(new Event('input', { bubbles: true }));
				el.dispatchEvent(new Event('change', { bubbles: true }));
			})()`,
			selector,
		);
	}

	async select(selector: string, value: string): Promise<void> {
		await this.evaluateThrow(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				el.value = ${JSON.stringify(value)};
				el.dispatchEvent(new Event('change', { bubbles: true }));
			})()`,
			selector,
		);
	}

	async submit(selector: string): Promise<void> {
		await this.evaluateThrow(
			`(() => {
				const form = document.querySelector(${JSON.stringify(selector)});
				if (!form) throw new Error('Form not found: ' + ${JSON.stringify(selector)});
				form.requestSubmit();
			})()`,
			selector,
		);
	}

	async type(selector: string, text: string, delayMs: number): Promise<void> {
		// Focus the element first
		await this.evaluateThrow(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				el.focus();
			})()`,
			selector,
		);

		for (const char of text) {
			await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Input.dispatchKeyEvent", {
				type: "keyDown",
				text: char,
				key: char,
			});
			await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Input.dispatchKeyEvent", {
				type: "keyUp",
				key: char,
			});
			if (delayMs > 0) {
				await new Promise((r) => setTimeout(r, delayMs));
			}
		}
	}

	async hover(selector: string): Promise<void> {
		const rectJson = await this.evaluateRaw(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				const r = el.getBoundingClientRect();
				return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
			})()`,
		);

		const { x, y } = JSON.parse(rectJson) as { x: number; y: number };

		await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Input.dispatchMouseEvent", {
			type: "mouseMoved",
			x,
			y,
		});

		// Also dispatch JS events for frameworks that rely on mouseover/mouseenter
		await this.evaluateRaw(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (el) {
					el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
					el.dispatchEvent(new MouseEvent('mouseenter', { bubbles: false }));
				}
			})()`,
		);
	}

	async scrollTo(selector: string): Promise<void> {
		await this.evaluateThrow(
			`(() => {
				const el = document.querySelector(${JSON.stringify(selector)});
				if (!el) throw new Error('Element not found: ' + ${JSON.stringify(selector)});
				el.scrollIntoView({ behavior: 'smooth', block: 'center' });
			})()`,
			selector,
		);
	}

	async scrollBy(x: number, y: number): Promise<void> {
		await this.evaluateRaw(`window.scrollBy(${x}, ${y})`);
	}

	async waitFor(selector: string, state: "visible" | "hidden" | "attached", timeoutMs: number): Promise<void> {
		const deadline = Date.now() + timeoutMs;

		let checkExpr: string;
		switch (state) {
			case "visible":
				checkExpr = `(() => { const el = document.querySelector(${JSON.stringify(selector)}); return !!(el && el.offsetParent !== null && getComputedStyle(el).visibility !== 'hidden'); })()`;
				break;
			case "hidden":
				checkExpr = `(() => { const el = document.querySelector(${JSON.stringify(selector)}); return !el || el.offsetParent === null || getComputedStyle(el).visibility === 'hidden'; })()`;
				break;
			case "attached":
				checkExpr = `document.querySelector(${JSON.stringify(selector)}) !== null`;
				break;
		}

		while (Date.now() < deadline) {
			const result = await this.evaluateRaw(checkExpr);
			if (result === "true") return;
			await new Promise((r) => setTimeout(r, 100));
		}

		throw new StepExecutionError(0, "wait_for", selector, `Timeout waiting for selector "${selector}" to be ${state} (${timeoutMs}ms)`);
	}

	async waitForNavigation(urlMatch: string | undefined, timeoutMs: number): Promise<void> {
		return new Promise<void>((resolve, reject) => {
			const timer = setTimeout(() => {
				this.config.cdpClient.off("event", listener);
				reject(new StepExecutionError(0, "wait_for_navigation", undefined, `Timeout waiting for navigation${urlMatch ? ` to URL containing "${urlMatch}"` : ""} (${timeoutMs}ms)`));
			}, timeoutMs);

			const listener = (_sessionId: string, method: string, params: Record<string, unknown>) => {
				if (method !== "Page.frameNavigated") return;
				const frame = params.frame as { url?: string } | undefined;
				if (!urlMatch || frame?.url?.includes(urlMatch)) {
					clearTimeout(timer);
					this.config.cdpClient.off("event", listener);
					resolve();
				}
			};

			this.config.cdpClient.on("event", listener);
		});
	}

	async waitForNetworkIdle(idleMs: number, timeoutMs: number): Promise<void> {
		return new Promise<void>((resolve, reject) => {
			let inflight = 0;
			let idleTimer: ReturnType<typeof setTimeout> | null = null;

			const overallTimer = setTimeout(() => {
				cleanup();
				reject(new StepExecutionError(0, "wait_for_network_idle", undefined, `Timeout waiting for network idle (${timeoutMs}ms)`));
			}, timeoutMs);

			const startIdleTimer = () => {
				if (idleTimer) clearTimeout(idleTimer);
				if (inflight === 0) {
					idleTimer = setTimeout(() => {
						cleanup();
						resolve();
					}, idleMs);
				}
			};

			const listener = (_sessionId: string, method: string, _params: Record<string, unknown>) => {
				if (method === "Network.requestWillBeSent") {
					inflight++;
					if (idleTimer) {
						clearTimeout(idleTimer);
						idleTimer = null;
					}
				} else if (method === "Network.loadingFinished" || method === "Network.loadingFailed") {
					inflight = Math.max(0, inflight - 1);
					startIdleTimer();
				}
			};

			const cleanup = () => {
				clearTimeout(overallTimer);
				if (idleTimer) clearTimeout(idleTimer);
				this.config.cdpClient.off("event", listener);
			};

			this.config.cdpClient.on("event", listener);

			// Start the idle timer immediately in case no requests are in flight
			startIdleTimer();
		});
	}

	async captureScreenshot(_label?: string): Promise<string> {
		const { screenshotCapture, screenshotDir, cdpClient, tabSessionId } = this.config;
		if (!screenshotCapture || !screenshotDir) return "";
		return screenshotCapture.capture(cdpClient, tabSessionId, screenshotDir);
	}

	async placeMarker(label: string): Promise<string> {
		const marker = await this.config.recorder.placeMarker(label);
		return marker.id;
	}

	/** Evaluate JS and throw StepExecutionError if the JS throws. */
	private async evaluateThrow(expression: string, selector?: string): Promise<void> {
		const res = (await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Runtime.evaluate", {
			expression,
			returnByValue: true,
		})) as EvaluateResult;

		if (res.exceptionDetails) {
			const msg = res.exceptionDetails.exception?.description ?? res.exceptionDetails.text ?? "JS evaluation failed";
			throw new StepExecutionError(0, "evaluate", selector, msg);
		}
	}

	/** Evaluate JS and return the raw string value. */
	private async evaluateRaw(expression: string): Promise<string> {
		const res = (await this.config.cdpClient.sendToTarget(this.config.tabSessionId, "Runtime.evaluate", {
			expression,
			returnByValue: true,
		})) as EvaluateResult;

		if (res.exceptionDetails) {
			const msg = res.exceptionDetails.exception?.description ?? res.exceptionDetails.text ?? "JS evaluation failed";
			throw new StepExecutionError(0, "evaluate", undefined, msg);
		}

		return String(res.result?.value ?? "");
	}

	/** Poll until document.readyState === "complete" or timeout. */
	private async waitForLoad(timeoutMs: number): Promise<void> {
		const deadline = Date.now() + timeoutMs;
		while (Date.now() < deadline) {
			const state = await this.evaluateRaw("document.readyState");
			if (state === "complete") return;
			await new Promise((r) => setTimeout(r, 100));
		}
		// Don't throw on timeout — page may have loaded already
	}
}
