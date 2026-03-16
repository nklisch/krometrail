import type { ReplayFormat, TestFramework } from "../../core/enums.js";
import type { EventRow } from "../storage/database.js";
import { formatTime, MARKER_LOOKAHEAD_MS, MARKER_LOOKBACK_MS } from "./format-helpers.js";
import { isErrorEvent } from "./predicates.js";
import type { MarkerRow, QueryEngine } from "./query-engine.js";

export type { ReplayFormat, TestFramework };

export interface ReplayContextParams {
	sessionId: string;
	aroundMarker?: string;
	timeRange?: { start: number; end: number };
	format: ReplayFormat;
	testFramework?: TestFramework;
}

export class ReplayContextGenerator {
	constructor(private queryEngine: QueryEngine) {}

	generate(params: ReplayContextParams): string {
		switch (params.format) {
			case "summary":
				return this.generateSummary(params);
			case "reproduction_steps":
				return this.generateReproSteps(params);
			case "test_scaffold":
				return this.generateTestScaffold(params);
		}
	}

	private generateSummary(params: ReplayContextParams): string {
		const { events } = this.getRelevantEvents(params);

		const lines = ["## Session Summary\n"];

		// Navigation path
		const navEvents = events.filter((e) => e.type === "navigation");
		if (navEvents.length > 0) {
			lines.push("### Navigation Path");
			for (const e of navEvents) {
				lines.push(`- ${formatTime(e.timestamp)}: ${e.summary}`);
			}
			lines.push("");
		}

		// Errors
		const errors = events.filter(isErrorEvent);
		if (errors.length > 0) {
			lines.push("### Errors");
			for (const e of errors) {
				lines.push(`- ${formatTime(e.timestamp)}: ${e.summary}`);
			}
			lines.push("");
		}

		// User actions
		const inputs = events.filter((e) => e.type === "user_input");
		if (inputs.length > 0) {
			lines.push("### User Actions");
			for (const e of inputs) {
				lines.push(`- ${formatTime(e.timestamp)}: ${e.summary}`);
			}
			lines.push("");
		}

		return lines.join("\n");
	}

	private generateReproSteps(params: ReplayContextParams): string {
		const { events } = this.getRelevantEvents(params);

		const lines = ["## Reproduction Steps\n"];
		let stepNum = 1;

		// Build steps from navigation + user input events
		for (const e of events) {
			if (e.type === "navigation" && e.summary.startsWith("Navigated to")) {
				const url = e.summary.replace("Navigated to ", "");
				lines.push(`${stepNum++}. Navigate to ${url}`);
			} else if (e.type === "user_input") {
				const full = this.queryEngine.getFullEvent(params.sessionId, e.event_id);
				if (full) {
					if (full.data.type === "click") {
						lines.push(`${stepNum++}. Click ${full.data.selector} ("${full.data.text}")`);
					} else if (full.data.type === "change") {
						lines.push(`${stepNum++}. Set ${full.data.selector} to "${full.data.value}"`);
					} else if (full.data.type === "submit") {
						lines.push(`${stepNum++}. Submit form ${full.data.selector}`);
						// List field values
						const fields = full.data.fields as Record<string, string> | undefined;
						if (fields) {
							for (const [name, value] of Object.entries(fields)) {
								lines.push(`   - ${name}: "${value}"`);
							}
						}
					}
				}
			}
		}

		// Expected vs actual
		lines.push("");
		const errorEvents = events.filter((e) => e.type === "page_error" || (e.type === "network_response" && Number.parseInt(e.summary, 10) >= 400));
		if (errorEvents.length > 0) {
			lines.push(`${stepNum}. **Expected:** Operation succeeds`);
			lines.push(`${stepNum}. **Actual:** ${errorEvents[0].summary}`);
		}

		// Evidence
		if (errorEvents.length > 0) {
			lines.push("\n## Evidence\n");
			for (const e of errorEvents) {
				lines.push(`- ${e.summary} (event_id: ${e.event_id})`);
			}
		}

		return lines.join("\n");
	}

	private generateTestScaffold(params: ReplayContextParams): string {
		const { events } = this.getRelevantEvents(params);
		const framework = params.testFramework ?? "playwright";

		if (framework === "playwright") {
			return this.generatePlaywrightTest(events, params.sessionId);
		} else if (framework === "cypress") {
			return this.generateCypressTest(events, params.sessionId);
		}
		throw new Error(`Unsupported test framework: ${framework}`);
	}

	private generatePlaywrightTest(events: EventRow[], sessionId: string): string {
		const lines = ["import { test, expect } from '@playwright/test';", "", "test('reproduce issue from browser session', async ({ page }) => {"];

		for (const e of events) {
			if (e.type === "navigation" && e.summary.startsWith("Navigated to")) {
				const url = e.summary.replace("Navigated to ", "");
				lines.push(`\tawait page.goto('${url}');`);
			} else if (e.type === "user_input") {
				const full = this.queryEngine.getFullEvent(sessionId, e.event_id);
				if (full) {
					if (full.data.type === "click") {
						lines.push(`\tawait page.click('${full.data.selector}');`);
					} else if (full.data.type === "change") {
						lines.push(`\tawait page.fill('${full.data.selector}', '${full.data.value}');`);
					} else if (full.data.type === "submit") {
						const fields = full.data.fields as Record<string, string> | undefined;
						if (fields) {
							for (const [name, value] of Object.entries(fields)) {
								if (value !== "[MASKED]") {
									lines.push(`\tawait page.fill('[name="${name}"]', '${value}');`);
								}
							}
						}
						lines.push(`\tawait page.click('${full.data.selector} [type="submit"], ${full.data.selector} button');`);
					}
				}
			}
		}

		// Add assertion for the error
		const errorEvent = events.find((e) => e.type === "network_response" && Number.parseInt(e.summary, 10) >= 400);
		if (errorEvent) {
			lines.push("");
			lines.push("\t// Verify the issue is fixed");
			lines.push("\t// TODO: Add appropriate assertion based on expected behavior");
		}

		lines.push("});");
		return lines.join("\n");
	}

	private generateCypressTest(events: EventRow[], sessionId: string): string {
		const lines = ["describe('reproduce issue from browser session', () => {", "\tit('should not reproduce the bug', () => {"];

		for (const e of events) {
			if (e.type === "navigation" && e.summary.startsWith("Navigated to")) {
				const url = e.summary.replace("Navigated to ", "");
				lines.push(`\t\tcy.visit('${url}');`);
			} else if (e.type === "user_input") {
				const full = this.queryEngine.getFullEvent(sessionId, e.event_id);
				if (full) {
					if (full.data.type === "click") {
						lines.push(`\t\tcy.get('${full.data.selector}').click();`);
					} else if (full.data.type === "change") {
						lines.push(`\t\tcy.get('${full.data.selector}').clear().type('${full.data.value}');`);
					} else if (full.data.type === "submit") {
						const fields = full.data.fields as Record<string, string> | undefined;
						if (fields) {
							for (const [name, value] of Object.entries(fields)) {
								if (value !== "[MASKED]") {
									lines.push(`\t\tcy.get('[name="${name}"]').clear().type('${value}');`);
								}
							}
						}
						lines.push(`\t\tcy.get('${full.data.selector}').submit();`);
					}
				}
			}
		}

		lines.push("\t});");
		lines.push("});");
		return lines.join("\n");
	}

	getRelevantEvents(params: ReplayContextParams): { events: EventRow[]; markers: MarkerRow[] } {
		const markers = this.queryEngine.getMarkers(params.sessionId);
		let timeRange: { start: number; end: number };

		if (params.aroundMarker) {
			const marker = markers.find((m) => m.id === params.aroundMarker);
			if (!marker) throw new Error(`Marker ${params.aroundMarker} not found`);
			timeRange = {
				start: marker.timestamp - MARKER_LOOKBACK_MS,
				end: marker.timestamp + MARKER_LOOKAHEAD_MS,
			};
		} else if (params.timeRange) {
			timeRange = params.timeRange;
		} else {
			// Default: entire session
			const session = this.queryEngine.getSession(params.sessionId);
			timeRange = {
				start: session.started_at,
				end: session.ended_at ?? Date.now(),
			};
		}

		const events = this.queryEngine.search(params.sessionId, {
			filters: { timeRange },
			maxResults: 200,
		});

		return { events, markers };
	}
}
