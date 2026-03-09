import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { SessionDiffer } from "../../browser/investigation/diff.js";
import type { InspectParams, OverviewOptions, QueryEngine } from "../../browser/investigation/query-engine.js";
import { renderDiff, renderInspectResult, renderSearchResults, renderSessionList, renderSessionOverview } from "../../browser/investigation/renderers.js";
import { ReplayContextGenerator } from "../../browser/investigation/replay-context.js";
import type { BrowserSessionInfo, Marker } from "../../browser/types.js";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import { getDaemonSocketPath } from "../../daemon/protocol.js";
import { errorResponse } from "./utils.js";

async function getDaemonClient(timeoutMs = 30_000): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: timeoutMs });
}

function formatSessionInfo(info: BrowserSessionInfo): string {
	const lines: string[] = [];
	const startedAt = new Date(info.startedAt).toISOString();
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
	// Tool: browser_start
	server.tool(
		"browser_start",
		"Launch Chrome and start recording browser events (network, console, user input). " +
			"Returns a session info summary once Chrome is ready. " +
			"Use browser_status to check recording state, browser_mark to place markers, browser_stop to end the session. " +
			"After stopping, use session_list and session_overview to investigate what was recorded.",
		{
			url: z.string().optional().describe("URL to open when launching Chrome"),
			port: z.number().optional().describe("Chrome remote debugging port. Default: 9222"),
			profile: z.string().optional().describe("Chrome profile name — creates an isolated profile under ~/.agent-lens/chrome-profiles/"),
			attach: z.boolean().optional().describe("Attach to an already-running Chrome instance instead of launching one. Chrome must have been started with --remote-debugging-port=9222"),
			all_tabs: z.boolean().optional().describe("Record all browser tabs. Default: first/active tab only"),
			tab_filter: z.string().optional().describe("Record only tabs whose URL matches this pattern"),
		},
		async ({ url, port, profile, attach, all_tabs, tab_filter }) => {
			const client = await getDaemonClient(30_000);
			try {
				const info = await client.call<BrowserSessionInfo>("browser.start", {
					port: port ?? 9222,
					profile,
					attach: attach ?? false,
					allTabs: all_tabs ?? false,
					tabFilter: tab_filter,
					url,
				});
				return { content: [{ type: "text" as const, text: formatSessionInfo(info) }] };
			} catch (err) {
				return errorResponse(err);
			} finally {
				client.dispose();
			}
		},
	);

	// Tool: browser_status
	server.tool(
		"browser_status",
		"Show the current browser recording status — whether Chrome is active, how many events and markers have been captured, and which tabs are being recorded.",
		{},
		async () => {
			const client = await getDaemonClient();
			try {
				const info = await client.call<BrowserSessionInfo | null>("browser.status", {});
				if (!info) {
					return { content: [{ type: "text" as const, text: "No active browser recording. Use browser_start to begin." }] };
				}
				return { content: [{ type: "text" as const, text: formatSessionInfo(info) }] };
			} catch (err) {
				return errorResponse(err);
			} finally {
				client.dispose();
			}
		},
	);

	// Tool: browser_mark
	server.tool(
		"browser_mark",
		"Place a named marker in the browser recording buffer at the current moment. " +
			"Markers let you annotate significant events (e.g. 'submitted form', 'saw error') so you can quickly find them later with session_overview or session_search using around_marker.",
		{
			label: z.string().optional().describe("Label for the marker, e.g. 'form submitted' or 'error appeared'"),
		},
		async ({ label }) => {
			const client = await getDaemonClient();
			try {
				const marker = await client.call<Marker>("browser.mark", { label });
				const time = new Date(marker.timestamp).toISOString();
				const markerLabel = marker.label ? `"${marker.label}"` : "(unlabeled)";
				return { content: [{ type: "text" as const, text: `Marker placed: ${markerLabel} at ${time} (id: ${marker.id})` }] };
			} catch (err) {
				return errorResponse(err);
			} finally {
				client.dispose();
			}
		},
	);

	// Tool: browser_stop
	server.tool(
		"browser_stop",
		"Stop the active browser recording session and flush all buffered events to the database. " +
			"After stopping, use session_list to find the recorded session and session_overview to investigate it.",
		{
			close_browser: z.boolean().optional().describe("Also close the Chrome browser. Default: false"),
		},
		async ({ close_browser }) => {
			const client = await getDaemonClient();
			try {
				await client.call("browser.stop", { closeBrowser: close_browser ?? false });
				return { content: [{ type: "text" as const, text: "Browser recording stopped. Use session_list to find the recorded session." }] };
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
		async ({ after, before, url_contains, has_markers, has_errors, limit }) => {
			try {
				const sessions = queryEngine.listSessions({
					after: after ? new Date(after).getTime() : undefined,
					before: before ? new Date(before).getTime() : undefined,
					urlContains: url_contains,
					hasMarkers: has_markers,
					hasErrors: has_errors,
					limit: limit ?? 10,
				});
				return { content: [{ type: "text" as const, text: renderSessionList(sessions) }] };
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 2: session_overview
	server.tool(
		"session_overview",
		"Get a structured overview of a recorded browser session — navigation timeline, markers, " +
			"errors, and network summary. Use this to understand what happened before diving into details. " +
			"Focus on a specific marker with around_marker.",
		{
			session_id: z.string().describe("Session ID from session_list"),
			include: z
				.array(z.enum(["timeline", "markers", "errors", "network_summary"]))
				.optional()
				.describe("What to include. Default: all"),
			around_marker: z.string().optional().describe("Center overview on this marker ID"),
			time_range: z
				.object({
					start: z.string().describe("ISO timestamp"),
					end: z.string().describe("ISO timestamp"),
				})
				.optional()
				.describe("Focus on a specific time window"),
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 3000"),
		},
		async ({ session_id, include, around_marker, time_range, token_budget }) => {
			try {
				const overview = queryEngine.getOverview(session_id, {
					include: include as OverviewOptions["include"],
					aroundMarker: around_marker,
					timeRange: time_range ? { start: new Date(time_range.start).getTime(), end: new Date(time_range.end).getTime() } : undefined,
				});
				return {
					content: [{ type: "text" as const, text: renderSessionOverview(overview, token_budget ?? 3000) }],
				};
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 3: session_search
	server.tool(
		"session_search",
		"Search recorded browser session events. Supports natural language search (uses FTS5) " +
			"and structured filters (event type, status code, time range). " +
			"Use natural language for exploratory search, structured filters for precise queries.",
		{
			session_id: z.string().describe("Session ID"),
			query: z.string().optional().describe("Natural language search query, e.g. 'validation error' or 'phone format'"),
			event_types: z
				.array(z.enum(["navigation", "network_request", "network_response", "console", "page_error", "user_input", "websocket", "performance", "marker"]))
				.optional()
				.describe("Filter by event type"),
			status_codes: z.array(z.number()).optional().describe("Filter network responses by HTTP status code, e.g. [400, 422, 500]"),
			time_range: z
				.object({
					start: z.string().describe("ISO timestamp"),
					end: z.string().describe("ISO timestamp"),
				})
				.optional()
				.describe("Filter by time window"),
			around_marker: z.string().optional().describe("Center search around this marker ID (±120s before, +30s after)"),
			url_pattern: z.string().optional().describe("Glob pattern to filter by URL in summary, e.g. '**/api/patients**'"),
			console_levels: z.array(z.string()).optional().describe("Filter console events by level, e.g. ['error', 'warn']"),
			contains_text: z.string().optional().describe("Case-insensitive substring match on event summary"),
			max_results: z.number().optional().describe("Max results. Default: 10"),
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 2000"),
		},
		async ({ session_id, query, event_types, status_codes, time_range, around_marker, url_pattern, console_levels, contains_text, max_results, token_budget }) => {
			try {
				const results = queryEngine.search(session_id, {
					query,
					filters: {
						eventTypes: event_types,
						statusCodes: status_codes,
						timeRange: time_range ? { start: new Date(time_range.start).getTime(), end: new Date(time_range.end).getTime() } : undefined,
						aroundMarker: around_marker,
						urlPattern: url_pattern,
						consoleLevels: console_levels,
						containsText: contains_text,
					},
					maxResults: max_results ?? 10,
				});
				return {
					content: [{ type: "text" as const, text: renderSearchResults(results, token_budget ?? 2000) }],
				};
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
			"and nearest screenshot. This is the primary evidence-gathering tool.",
		{
			session_id: z.string().describe("Session ID"),
			event_id: z.string().optional().describe("Specific event ID (from session_search results)"),
			marker_id: z.string().optional().describe("Jump to a marker"),
			timestamp: z.string().optional().describe("ISO timestamp — inspect the moment closest to this time"),
			include: z
				.array(z.enum(["surrounding_events", "network_body", "screenshot", "form_state", "console_context"]))
				.optional()
				.describe("What to include alongside the event detail. Default: all"),
			context_window: z.number().optional().describe("Seconds of surrounding events to include. Default: 5"),
			token_budget: z.number().optional().describe("Max tokens for the response. Default: 3000"),
		},
		async ({ session_id, event_id, marker_id, timestamp, include, context_window, token_budget }) => {
			try {
				const result = queryEngine.inspect(session_id, {
					eventId: event_id,
					markerId: marker_id,
					timestamp: timestamp ? new Date(timestamp).getTime() : undefined,
					include: include as InspectParams["include"],
					contextWindow: context_window ?? 5,
				});
				return {
					content: [{ type: "text" as const, text: renderInspectResult(result, token_budget ?? 3000) }],
				};
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 5: session_diff
	server.tool(
		"session_diff",
		"Compare two moments in a recorded browser session. Shows what changed between two " +
			"timestamps or events: URL, form state, storage, new console messages, and network activity. " +
			"Useful for understanding what happened between page load and an error.",
		{
			session_id: z.string().describe("Session ID"),
			before: z.string().describe("First moment — timestamp (ISO or HH:MM:SS) or event ID"),
			after: z.string().describe("Second moment — timestamp (ISO or HH:MM:SS) or event ID"),
			include: z
				.array(z.enum(["form_state", "storage", "url", "console_new", "network_new"]))
				.optional()
				.describe("What to diff. Default: all"),
			token_budget: z.number().optional().describe("Max tokens. Default: 2000"),
		},
		async ({ session_id, before, after, include, token_budget }) => {
			try {
				const differ = new SessionDiffer(queryEngine);
				const diff = differ.diff({ sessionId: session_id, before, after, include });
				return { content: [{ type: "text" as const, text: renderDiff(diff, token_budget ?? 2000) }] };
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 6: session_replay_context
	server.tool(
		"session_replay_context",
		"Generate a reproduction context from a recorded browser session. " +
			"Outputs reproduction steps, test scaffolds (Playwright or Cypress), or a summary. " +
			"Use this to create actionable artifacts from investigation findings.",
		{
			session_id: z.string().describe("Session ID"),
			around_marker: z.string().optional().describe("Focus on events around this marker"),
			time_range: z
				.object({
					start: z.string(),
					end: z.string(),
				})
				.optional()
				.describe("Focus on a specific time window"),
			format: z
				.enum(["summary", "reproduction_steps", "test_scaffold"])
				.describe("Output format: 'summary' for overview, 'reproduction_steps' for step-by-step, 'test_scaffold' for automated test code"),
			test_framework: z.enum(["playwright", "cypress"]).optional().describe("Test framework for scaffold generation. Default: playwright"),
		},
		async ({ session_id, around_marker, time_range, format, test_framework }) => {
			try {
				const generator = new ReplayContextGenerator(queryEngine);
				const text = generator.generate({
					sessionId: session_id,
					aroundMarker: around_marker,
					timeRange: time_range ? { start: new Date(time_range.start).getTime(), end: new Date(time_range.end).getTime() } : undefined,
					format,
					testFramework: test_framework,
				});
				return { content: [{ type: "text" as const, text }] };
			} catch (err) {
				return errorResponse(err);
			}
		},
	);
}
