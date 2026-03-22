import { readFileSync } from "node:fs";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { renderStepResults } from "../../browser/executor/renderer.js";
import { CaptureConfigSchema, type RunStepsResult, StepSchema } from "../../browser/executor/types.js";
import { SessionDiffer } from "../../browser/investigation/diff.js";
import type { InspectParams, OverviewOptions, QueryEngine } from "../../browser/investigation/query-engine.js";
import { renderDiff, renderInspectResult, renderSearchResults, renderSessionList, renderSessionOverview } from "../../browser/investigation/renderers.js";
import { ReplayContextGenerator } from "../../browser/investigation/replay-context.js";
import type { BrowserSessionInfo, Marker } from "../../browser/types.js";
import { DiffIncludeSchema, FrameworkSchema, InspectIncludeSchema, OverviewIncludeSchema, ReplayFormatSchema, SearchableEventTypeSchema, TestFrameworkSchema } from "../../core/enums.js";
import { CDPConnectionError, ChromeEarlyExitError, ChromeNotFoundError } from "../../core/errors.js";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import { getDaemonSocketPath } from "../../daemon/protocol.js";
import { type ContentBlock, errorResponse, imageContent, type ToolResult, textResponse, toolHandler } from "./utils.js";

const TimeRangeSchema = z
	.object({
		start: z.string().describe("ISO timestamp"),
		end: z.string().describe("ISO timestamp"),
	})
	.optional()
	.describe("Focus on a specific time window");

function parseTimeRange(tr: { start: string; end: string } | undefined): { start: number; end: number } | undefined {
	return tr ? { start: new Date(tr.start).getTime(), end: new Date(tr.end).getTime() } : undefined;
}

async function getDaemonClient(timeoutMs = 30_000): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: timeoutMs });
}

/**
 * Acquire a daemon client, call fn, format the result as text, and always dispose.
 * Use for chrome_* tool handlers that follow the get/call/dispose pattern.
 */
async function withDaemonClient<T>(fn: (client: DaemonClient) => Promise<T>, format: (result: T) => string, timeoutMs?: number): Promise<ToolResult> {
	const client = await getDaemonClient(timeoutMs);
	try {
		return textResponse(format(await fn(client)));
	} catch (err) {
		return errorResponse(err);
	} finally {
		client.dispose();
	}
}

function formatSessionInfo(info: BrowserSessionInfo): string {
	const lines: string[] = [];
	const startedAt = info.startedAt > 0 ? new Date(info.startedAt).toISOString() : "just now";
	lines.push(`Browser recording active since ${startedAt}`);
	lines.push(`Events: ${info.eventCount}  Markers: ${info.markerCount}  Buffer age: ${Math.round(info.bufferAgeMs / 1000)}s`);
	if (info.tabs.length > 0) {
		lines.push("Tabs:");
		for (const tab of info.tabs) {
			const title = tab.title ? `"${tab.title}" ` : "";
			lines.push(`  ${title}(${tab.url})`);
		}
	}
	return lines.join("\n");
}

export function registerBrowserTools(server: McpServer, queryEngine: QueryEngine): void {
	// Tool: chrome_start
	server.tool(
		"chrome_start",
		"Launch Chrome and start recording browser events (network, console, user input). " +
			"By default, launches a new isolated Chrome instance — no conflict with an existing Chrome window. " +
			"Use profile='krometrail' (or any name) to get a fully isolated Chrome that won't collide with your regular browser. " +
			"Returns a session info summary once Chrome is ready. " +
			"Use chrome_status to check recording state, chrome_mark to place markers, chrome_stop to end the session. " +
			"After stopping, use session_list and session_overview to investigate what was recorded. " +
			"Use attach=true only if Chrome was already launched with --remote-debugging-port=9222.",
		{
			url: z.string().optional().describe("URL to open when launching Chrome"),
			port: z.number().optional().describe("Chrome remote debugging port. Default: 9222"),
			profile: z
				.string()
				.optional()
				.describe(
					"Chrome profile name — creates an isolated user-data-dir under ~/.krometrail/chrome-profiles/<name>. " +
						"Each profile has its own cookies, storage, and login state. " +
						"Use this to avoid conflicts with an already-running Chrome. Example: 'krometrail'",
				),
			attach: z
				.boolean()
				.optional()
				.describe(
					"Attach to an already-running Chrome instance (don't launch). " +
						"Requires Chrome to have been started with --remote-debugging-port=9222. " +
						"If Chrome is running normally without that flag, use profile instead to launch an isolated instance.",
				),
			all_tabs: z.boolean().optional().describe("Record all browser tabs. Default: first/active tab only"),
			tab_filter: z.string().optional().describe("Glob pattern — record only tabs whose URL matches, e.g. '**/app/**'"),
			screenshot_interval_ms: z.number().optional().describe("Periodic screenshot interval in ms. 0 or omit to disable. Example: 5000 for a screenshot every 5s"),
			framework_state: z
				.union([z.boolean(), z.array(FrameworkSchema)])
				.optional()
				.describe("Enable framework state observation. " + "true = auto-detect all supported frameworks. " + '["react"] = only React. ' + '["react", "vue"] = both. ' + "Default: false (disabled)."),
		},
		async ({ url, port, profile, attach, all_tabs, tab_filter, screenshot_interval_ms, framework_state }) => {
			const client = await getDaemonClient(30_000);
			try {
				const info = await client.call<BrowserSessionInfo>("browser.start", {
					port: port ?? 9222,
					profile,
					attach: attach ?? false,
					allTabs: all_tabs ?? false,
					tabFilter: tab_filter,
					url,
					screenshotIntervalMs: screenshot_interval_ms,
					frameworkState: framework_state,
				});
				return textResponse(formatSessionInfo(info));
			} catch (err) {
				if (err instanceof ChromeEarlyExitError) {
					return textResponse(
						`Error: ${err.message}\n\n` +
							"Chrome launched but exited immediately — likely an existing Chrome instance absorbed the launch.\n\n" +
							"Fix: close your existing Chrome browser, then retry chrome_start.\n" +
							"If you can't close Chrome, ask the user to launch it with remote debugging:\n" +
							"  google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/krometrail-chrome\n" +
							"  Then: chrome_start(attach: true)",
					);
				}
				if (err instanceof CDPConnectionError) {
					return textResponse(
						`Error: ${err.message}\n\n` +
							"Chrome was launched but its debug port never became available.\n\n" +
							"This can happen if:\n" +
							"- Another process is using port 9222\n" +
							"- Chrome is taking unusually long to start\n\n" +
							"Fix option 1 — try a different port:\n" +
							"  chrome_start(port: 9223, profile: 'krometrail', url: '<your-url>')\n\n" +
							"Fix option 2 — ask the user to launch Chrome manually:\n" +
							"  google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/krometrail-chrome\n" +
							"  Then: chrome_start(attach: true)",
					);
				}
				if (err instanceof ChromeNotFoundError) {
					return textResponse(`Error: ${err.message}`);
				}
				return errorResponse(err);
			} finally {
				client.dispose();
			}
		},
	);

	// Tool: chrome_status
	server.tool("chrome_status", "Show the current Chrome recording status — whether Chrome is active, how many events and markers have been captured, and which tabs are being recorded.", {}, () =>
		withDaemonClient(
			(client) => client.call<BrowserSessionInfo | null>("browser.status", {}),
			(info) => (info ? formatSessionInfo(info) : "No active Chrome recording. Use chrome_start to begin."),
		),
	);

	// Tool: chrome_mark
	server.tool(
		"chrome_mark",
		"Place a named marker in the Chrome recording buffer at the current moment. " +
			"Markers let you annotate significant events (e.g. 'submitted form', 'saw error') so you can quickly find them later with session_overview or session_search using around_marker.",
		{
			label: z.string().optional().describe("Label for the marker, e.g. 'form submitted' or 'error appeared'. Descriptive labels help you find this marker later with around_marker."),
		},
		({ label }) =>
			withDaemonClient(
				(client) => client.call<Marker>("browser.mark", { label }),
				(marker) => {
					const time = new Date(marker.timestamp).toISOString();
					const markerLabel = marker.label ? `"${marker.label}"` : "(unlabeled)";
					return `Marker placed: ${markerLabel} at ${time} (id: ${marker.id})`;
				},
			),
	);

	// Tool: chrome_stop
	server.tool(
		"chrome_stop",
		"Stop the active Chrome recording session and flush all buffered events to the database. " +
			"After stopping, use session_list to find the recorded session and session_overview to investigate it.",
		{
			close_chrome: z.boolean().optional().describe("Also close the Chrome browser. Default: false"),
		},
		({ close_chrome }) =>
			withDaemonClient(
				(client) => client.call("browser.stop", { closeBrowser: close_chrome ?? false }),
				() => "Chrome recording stopped. Use session_list to find the recorded session.",
			),
	);

	// Tool: chrome_refresh
	server.tool(
		"chrome_refresh",
		"Reload the current page and clear all recorded events and markers — a quick reset to a clean slate without restarting the browser session. " +
			"Use this when you want to start a fresh investigation on the same page, or after making code changes that require a page reload. " +
			"The recording session stays active; only the buffer is wiped.",
		{},
		() =>
			withDaemonClient(
				(client) => client.call<BrowserSessionInfo>("browser.refresh", {}),
				(info) => {
					const lines = ["Page reloaded and buffer cleared."];
					lines.push(formatSessionInfo(info));
					return lines.join("\n");
				},
			),
	);

	// Tool: chrome_run_steps
	server.tool(
		"chrome_run_steps",
		"Execute a sequence of browser actions (navigate, click, fill, wait, etc.) in one call. " +
			"Requires an active recording session (chrome_start). " +
			"Each step is auto-marked and auto-screenshotted by default for investigation. " +
			"Tip: start with a screenshot step to understand the current page state before interacting. " +
			"Use reload (not navigate to the same URL) to force a full page refresh — navigate may hit SPA client-side cache. " +
			"Use press_key with key='Enter' to submit forms that lack a submit button. " +
			"Use name + save to store a scenario for replay. Pass just name to replay a saved scenario.",
		{
			steps: z
				.array(StepSchema)
				.optional()
				.describe(
					"Ordered actions to execute. Each step has an 'action' field: " +
						"navigate, reload, click, fill, select, submit, type, press_key, hover, " +
						"scroll_to, scroll_by, wait, wait_for, wait_for_navigation, " +
						"wait_for_network_idle, screenshot, mark, evaluate",
				),
			name: z.string().optional().describe("Scenario name. Use with save=true to store, or alone to replay."),
			save: z.boolean().optional().describe("Save the steps as a named scenario for later replay"),
			capture: CaptureConfigSchema.optional().describe("Capture config. screenshot: 'all' (default), 'none', 'on_error'. markers: true (default) or false."),
		},
		async ({ steps, name, save, capture }) => {
			const client = await getDaemonClient(120_000);
			try {
				const result = await client.call<RunStepsResult>("browser.run-steps", { steps, name, save, capture });
				const content: ContentBlock[] = [{ type: "text" as const, text: renderStepResults(result) }];

				// Include screenshots inline as images
				for (const step of result.results) {
					if (step.screenshotPath) {
						try {
							const data = readFileSync(step.screenshotPath).toString("base64");
							content.push(imageContent(data));
						} catch {
							// Screenshot file may have been cleaned up — skip silently
						}
					}
				}

				return { content };
			} catch (err) {
				return errorResponse(err);
			} finally {
				client.dispose();
			}
		},
	);

	// Tool 1: session_list
	server.tool(
		"session_list",
		"List recorded browser sessions. Use this to find sessions to investigate. " + "Filter by time, URL, or whether the session has markers/errors.",
		{
			after: z.string().optional().describe("ISO timestamp — only sessions after this time"),
			before: z.string().optional().describe("ISO timestamp — only sessions before this time"),
			url_contains: z.string().optional().describe("Filter by URL pattern"),
			has_markers: z.boolean().optional().describe("Only sessions with user-placed markers"),
			has_errors: z.boolean().optional().describe("Only sessions with captured errors (4xx/5xx, exceptions, console errors)"),
			limit: z.number().optional().describe("Max results. Default: 10"),
		},
		toolHandler(async ({ after, before, url_contains, has_markers, has_errors, limit }) => {
			const sessions = queryEngine.listSessions({
				after: after ? new Date(after).getTime() : undefined,
				before: before ? new Date(before).getTime() : undefined,
				urlContains: url_contains,
				hasMarkers: has_markers,
				hasErrors: has_errors,
				limit: limit ?? 10,
			});
			return renderSessionList(sessions);
		}),
	);

	// Tool 2: session_overview
	server.tool(
		"session_overview",
		"Get a structured overview of a recorded browser session — navigation timeline, markers, " +
			"errors, and network summary. Use this to understand what happened before diving into details. " +
			"Focus on a specific marker with around_marker.",
		{
			session_id: z.string().describe('Session ID from session_list, or "latest" for the most recent session'),
			include: z.array(OverviewIncludeSchema).optional().describe("What to include. Default: all"),
			around_marker: z.string().optional().describe("Center overview on a marker — accepts marker ID or label"),
			time_range: TimeRangeSchema,
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 3000"),
		},
		async ({ session_id, include, around_marker, time_range, token_budget }) => {
			try {
				const overview = queryEngine.getOverview(session_id, {
					include: include as OverviewOptions["include"],
					aroundMarker: around_marker,
					timeRange: parseTimeRange(time_range),
				});
				return textResponse(renderSessionOverview(overview, token_budget ?? 3000));
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 3: session_search
	server.tool(
		"session_search",
		"Search recorded browser session events. Supports natural language search (uses FTS5) " +
			"and structured filters (event type, status code, time range, framework, component, pattern). " +
			"Use natural language for exploratory search, structured filters for precise queries.",
		{
			session_id: z.string().describe('Session ID, or "latest" for the most recent session'),
			query: z.string().optional().describe("Natural language search query, e.g. 'validation error' or 'phone format'"),
			event_types: z.array(SearchableEventTypeSchema).optional().describe("Filter by event type"),
			status_codes: z.array(z.number()).optional().describe("Filter network responses by HTTP status code, e.g. [400, 422, 500]"),
			time_range: TimeRangeSchema,
			around_marker: z.string().optional().describe("Center search around a marker — accepts marker ID or label (±120s before, +30s after)"),
			url_pattern: z.string().optional().describe("Glob pattern to filter by URL in summary, e.g. '**/api/patients**'"),
			console_levels: z.array(z.string()).optional().describe("Filter console events by level, e.g. ['error', 'warn']"),
			contains_text: z.string().optional().describe("Case-insensitive substring match on event summary"),
			limit: z.number().optional().describe("Max results. Default: 10"),
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 2000"),
			framework: FrameworkSchema.optional().describe("Filter by framework. Automatically narrows to framework event types."),
			component: z.string().optional().describe("Filter by component name (substring match), e.g. 'UserProfile'"),
			pattern: z.string().optional().describe("Filter by bug pattern name, e.g. 'stale_closure', 'infinite_rerender'"),
		},
		async ({ session_id, query, event_types, status_codes, time_range, around_marker, url_pattern, console_levels, contains_text, limit, token_budget, framework, component, pattern }) => {
			try {
				const results = queryEngine.search(session_id, {
					query,
					filters: {
						eventTypes: event_types,
						statusCodes: status_codes,
						timeRange: parseTimeRange(time_range),
						aroundMarker: around_marker,
						urlPattern: url_pattern,
						consoleLevels: console_levels,
						containsText: contains_text,
						framework,
						component,
						pattern,
					},
					maxResults: limit ?? 10,
				});
				return textResponse(renderSearchResults(results, token_budget ?? 2000));
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 4: session_inspect
	server.tool(
		"session_inspect",
		"Deep-dive into a specific event or moment in a recorded browser session. " +
			"Returns full event detail, network request/response bodies, surrounding events, " +
			"and nearest screenshot. This is the primary evidence-gathering tool. " +
			"If multiple of event_id, marker_id, and timestamp are provided, precedence is: event_id > marker_id > timestamp.",
		{
			session_id: z.string().describe('Session ID, or "latest" for the most recent session'),
			event_id: z.string().optional().describe("Specific event ID (from session_search results)"),
			marker_id: z.string().optional().describe("Jump to a marker — accepts marker ID or label from session_overview"),
			timestamp: z.string().optional().describe("Timestamp — ISO format, wall-clock (HH:mm:ss.SSS from overview), or epoch ms"),
			include: z.array(InspectIncludeSchema).optional().describe("What to include alongside the event detail. Default: all"),
			context_window: z.number().optional().describe("Seconds of surrounding events to include. Default: 5"),
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 3000"),
		},
		toolHandler(async ({ session_id, event_id, marker_id, timestamp, include, context_window, token_budget }) => {
			const result = queryEngine.inspect(session_id, {
				eventId: event_id,
				markerId: marker_id,
				timestamp: timestamp ?? undefined,
				include: include as InspectParams["include"],
				contextWindow: context_window ?? 5,
			});
			return renderInspectResult(result, token_budget ?? 3000);
		}),
	);

	// Tool 5: session_diff
	server.tool(
		"session_diff",
		"Compare two moments in a recorded browser session. Shows what changed between two " +
			"timestamps or events: URL, form state, storage, new console messages, and network activity. " +
			"Useful for understanding what happened between page load and an error.",
		{
			session_id: z.string().describe('Session ID, or "latest" for the most recent session'),
			from: z.string().describe("First moment — ISO timestamp, wall-clock (HH:mm:ss.SSS), epoch ms, or event ID"),
			to: z.string().describe("Second moment — ISO timestamp, wall-clock (HH:mm:ss.SSS), epoch ms, or event ID"),
			include: z.array(DiffIncludeSchema).optional().describe("What to diff. Default: form_state, storage, url, console_new, network_new (framework_state must be explicitly requested)"),
			token_budget: z.number().optional().describe("Max tokens. Default: 2000"),
		},
		toolHandler(async ({ session_id, from, to, include, token_budget }) => {
			const differ = new SessionDiffer(queryEngine);
			const diff = differ.diff({ sessionId: session_id, before: from, after: to, include });
			return renderDiff(diff, token_budget ?? 2000);
		}),
	);

	// Tool 6: session_replay_context
	server.tool(
		"session_replay_context",
		"Generate a reproduction context from a recorded browser session. " +
			"Outputs reproduction steps, test scaffolds (Playwright or Cypress), or a summary. " +
			"Use this to create actionable artifacts from investigation findings.",
		{
			session_id: z.string().describe('Session ID, or "latest" for the most recent session'),
			around_marker: z.string().optional().describe("Focus on events around this marker — accepts marker ID or label"),
			time_range: TimeRangeSchema,
			format: ReplayFormatSchema.describe("Output format: 'summary' for overview, 'reproduction_steps' for step-by-step, 'test_scaffold' for automated test code"),
			test_framework: TestFrameworkSchema.optional().describe("Test framework for scaffold generation. Default: playwright"),
		},
		toolHandler(async ({ session_id, around_marker, time_range, format, test_framework }) => {
			const generator = new ReplayContextGenerator(queryEngine);
			return generator.generate({
				sessionId: session_id,
				aroundMarker: around_marker,
				timeRange: parseTimeRange(time_range),
				format,
				testFramework: test_framework,
			});
		}),
	);
}
