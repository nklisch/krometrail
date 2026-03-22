import { CDPPortAdapter } from "../browser/executor/cdp-adapter.js";
import type { ScenarioStore } from "../browser/executor/scenario-store.js";
import { StepExecutor } from "../browser/executor/step-executor.js";
import type { Step } from "../browser/executor/types.js";
import { HARExporter } from "../browser/export/har.js";
import { SessionDiffer } from "../browser/investigation/diff.js";
import type { QueryEngine } from "../browser/investigation/query-engine.js";
import { renderDiff, renderInspectResult, renderSearchResults, renderSessionOverview } from "../browser/investigation/renderers.js";
import { ReplayContextGenerator } from "../browser/investigation/replay-context.js";
import type { BrowserRecorder } from "../browser/recorder/index.js";
import { BrowserRecorderStateError } from "../core/errors.js";
import {
	BrowserDiffParamsSchema,
	BrowserExportParamsSchema,
	BrowserInspectParamsSchema,
	BrowserMarkParamsSchema,
	BrowserOverviewParamsSchema,
	BrowserReplayContextParamsSchema,
	BrowserSearchParamsSchema,
	BrowserSessionsParamsSchema,
	BrowserStartParamsSchema,
	BrowserStopParamsSchema,
	RunStepsParamsSchema,
} from "./protocol.js";

export interface BrowserHandlerState {
	recorder: BrowserRecorder | null;
	scenarioStore: ScenarioStore;
	getQueryEngine: () => QueryEngine;
	setRecorder: (r: BrowserRecorder | null) => void;
	resetIdleTimer: () => void;
	/** In-flight start promise — duplicate callers await the same result. */
	startPromise?: Promise<unknown>;
}

export function buildStepExecutorAdapter(recorder: BrowserRecorder): CDPPortAdapter {
	const cdpClient = recorder.getCDPClient();
	if (!cdpClient) throw new BrowserRecorderStateError("CDP client not available");
	const tabSessionId = recorder.getPrimaryTabSession();
	if (!tabSessionId) throw new BrowserRecorderStateError("No active tab session");

	return new CDPPortAdapter({
		cdpClient,
		tabSessionId,
		recorder,
		screenshotCapture: recorder.getScreenshotCapture(),
		screenshotDir: recorder.getOrCreateScreenshotDir(),
	});
}

export async function handleBrowserMethod(method: string, params: Record<string, unknown>, state: BrowserHandlerState): Promise<unknown> {
	switch (method) {
		// --- Browser Recording ---
		case "browser.start": {
			const p = BrowserStartParamsSchema.parse(params);
			// Idempotent: if already recording, return the existing session.
			if (state.recorder?.isRecording()) {
				return state.recorder.getSessionInfo();
			}
			// If a start is already in flight (duplicate MCP dispatch), await
			// the same promise so both callers get the same result.
			if (state.startPromise) {
				return state.startPromise;
			}
			// Clean up stale recorder from a previous failed start
			if (state.recorder) {
				try {
					await state.recorder.stop();
				} catch {
					// Ignore cleanup errors — the recorder is in a bad state
				}
				state.setRecorder(null);
			}
			// Store the start promise so concurrent calls coalesce onto it.
			state.startPromise = (async () => {
				const { BrowserRecorder } = await import("../browser/recorder/index.js");
				const recorder = new BrowserRecorder({
					port: p.port,
					attach: p.attach,
					profile: p.profile,
					allTabs: p.allTabs,
					tabFilter: p.tabFilter,
					url: p.url,
					persistence: {},
					...(p.screenshotIntervalMs !== undefined && { screenshots: { intervalMs: p.screenshotIntervalMs } }),
					frameworkState: p.frameworkState,
				});
				recorder.onAutoStop = () => {
					state.setRecorder(null);
					state.resetIdleTimer();
				};
				const result = await recorder.start();
				state.setRecorder(recorder);
				return result;
			})();
			try {
				return await state.startPromise;
			} finally {
				state.startPromise = undefined;
			}
		}

		case "browser.mark": {
			const p = BrowserMarkParamsSchema.parse(params);
			if (!state.recorder?.isRecording()) {
				throw new BrowserRecorderStateError("No active browser recording. Call browser.start first.");
			}
			return state.recorder.placeMarker(p.label);
		}

		case "browser.status": {
			return state.recorder?.getSessionInfo() ?? null;
		}

		case "browser.stop": {
			const p = BrowserStopParamsSchema.parse(params);
			if (!state.recorder) return null;
			await state.recorder.stop(p.closeBrowser);
			state.setRecorder(null);
			return null;
		}

		case "browser.refresh": {
			if (!state.recorder?.isRecording()) {
				throw new BrowserRecorderStateError("No active browser recording. Call browser.start first.");
			}
			return state.recorder.refresh();
		}

		case "browser.run-steps": {
			const p = RunStepsParamsSchema.parse(params);

			// Resolve steps: from params or from saved scenario
			let steps: Step[];
			if (p.steps) {
				steps = p.steps;
			} else if (p.name) {
				const scenario = state.scenarioStore.get(p.name);
				if (!scenario) throw new BrowserRecorderStateError(`No saved scenario named "${p.name}"`);
				steps = scenario.steps;
			} else {
				throw new BrowserRecorderStateError("Either steps or name is required");
			}

			// Save scenario if requested
			if (p.save && p.name) {
				state.scenarioStore.save(p.name, steps);
			}

			// Require active recording
			if (!state.recorder?.isRecording()) {
				throw new BrowserRecorderStateError("No active browser recording. Call browser.start first, then run steps.");
			}

			// Build the CDP port adapter and execute
			const adapter = buildStepExecutorAdapter(state.recorder);
			const executor = new StepExecutor(adapter);
			const result = await executor.execute(steps, p.capture);
			result.sessionId = state.recorder.getSessionInfo()?.id;

			return result;
		}

		// --- Browser Investigation ---
		case "browser.sessions": {
			const p = BrowserSessionsParamsSchema.parse(params);
			return state.getQueryEngine().listSessions(p);
		}

		case "browser.overview": {
			const p = BrowserOverviewParamsSchema.parse(params);
			const overview = state.getQueryEngine().getOverview(p.sessionId, {
				include: p.include,
				aroundMarker: p.aroundMarker,
				timeRange: p.timeRange,
			});
			return renderSessionOverview(overview, p.tokenBudget ?? 3000);
		}

		case "browser.search": {
			const p = BrowserSearchParamsSchema.parse(params);
			const results = state.getQueryEngine().search(p.sessionId, {
				query: p.query,
				filters: {
					eventTypes: p.eventTypes,
					statusCodes: p.statusCodes,
					timeRange: p.timeRange,
				},
				maxResults: p.maxResults,
			});
			return renderSearchResults(results, p.tokenBudget ?? 2000);
		}

		case "browser.inspect": {
			const p = BrowserInspectParamsSchema.parse(params);
			const result = state.getQueryEngine().inspect(p.sessionId, {
				eventId: p.eventId,
				markerId: p.markerId,
				timestamp: p.timestamp !== undefined ? String(p.timestamp) : undefined,
				include: p.include,
				contextWindow: p.contextWindow,
			});
			return renderInspectResult(result, p.tokenBudget ?? 3000);
		}

		case "browser.diff": {
			const p = BrowserDiffParamsSchema.parse(params);
			const differ = new SessionDiffer(state.getQueryEngine());
			const diff = differ.diff({ sessionId: p.sessionId, before: p.before, after: p.after, include: p.include });
			return renderDiff(diff, p.tokenBudget ?? 2000);
		}

		case "browser.replay-context": {
			const p = BrowserReplayContextParamsSchema.parse(params);
			const generator = new ReplayContextGenerator(state.getQueryEngine());
			return generator.generate({
				sessionId: p.sessionId,
				aroundMarker: p.aroundMarker,
				timeRange: p.timeRange,
				format: p.format,
				testFramework: p.testFramework,
			});
		}

		case "browser.export": {
			const p = BrowserExportParamsSchema.parse(params);
			const exporter = new HARExporter(state.getQueryEngine());
			const harFile = exporter.export({
				sessionId: p.sessionId,
				timeRange: p.timeRange,
				includeResponseBodies: p.includeResponseBodies,
			});
			return JSON.stringify(harFile, null, 2);
		}

		default:
			return undefined; // not handled
	}
}
