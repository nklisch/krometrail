import { defineCommand } from "citty";
import type { SessionSummary } from "../../browser/investigation/query-engine.js";
import type { BrowserSessionInfo, Marker } from "../../browser/types.js";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import { getDaemonSocketPath } from "../../daemon/protocol.js";
import { successEnvelope } from "../envelope.js";
import { exitCodeFromError } from "../exit-codes.js";
import { formatBrowserSession, formatBrowserSessions, formatError, formatInvestigation, resolveOutputMode } from "../format.js";

/**
 * Create a DaemonClient ensuring the daemon is running.
 * Uses a longer timeout for browser.start since Chrome launch may take a few seconds.
 */
async function getClient(timeoutMs = 30_000): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: timeoutMs });
}

/**
 * Shared browser command runner with error handling and exit codes.
 */
async function runBrowserCommand(args: { json?: boolean; quiet?: boolean }, handler: (client: DaemonClient) => Promise<void>, timeoutMs?: number): Promise<void> {
	const mode = resolveOutputMode(args);
	const client = await getClient(timeoutMs);
	try {
		await handler(client);
	} catch (err) {
		process.stderr.write(`${formatError(err, mode)}\n`);
		process.exit(exitCodeFromError(err));
	} finally {
		client.dispose();
	}
}

/**
 * Parse framework-state flag:
 * "auto" → true, "react,vue" → ["react", "vue"], absent → undefined
 */
function parseFrameworkState(value: string | undefined): boolean | string[] | undefined {
	if (!value) return undefined;
	if (value === "auto") return true;
	return value
		.split(",")
		.map((s) => s.trim())
		.filter(Boolean);
}

/**
 * Split comma-separated strings, trimming whitespace and filtering empty entries.
 */
function splitComma(value: string | undefined): string[] | undefined {
	if (!value) return undefined;
	const parts = value
		.split(",")
		.map((s) => s.trim())
		.filter(Boolean);
	return parts.length > 0 ? parts : undefined;
}

export const browserStartCommand = defineCommand({
	meta: {
		name: "start",
		description: "Launch Chrome and start recording browser events",
	},
	args: {
		port: {
			type: "string",
			description: "Chrome remote debugging port",
			default: "9222",
		},
		profile: {
			type: "string",
			description: "Chrome profile name (creates isolated user-data-dir under ~/.krometrail/chrome-profiles/)",
		},
		attach: {
			type: "boolean",
			description: "Attach to an already-running Chrome instance (don't launch Chrome). Chrome must have been started with --remote-debugging-port=9222",
			default: false,
		},
		url: {
			type: "string",
			description: "URL to open when launching Chrome (ignored with --attach)",
		},
		"all-tabs": {
			type: "boolean",
			description: "Record all browser tabs (default: first/active tab only)",
			default: false,
		},
		tab: {
			type: "string",
			description: "Record only tabs matching this URL pattern",
		},
		"screenshot-interval": {
			type: "string",
			description: "Screenshot capture interval in ms (0 to disable, default: disabled)",
		},
		"framework-state": {
			type: "string",
			description: "Framework state observation: 'auto' for auto-detect, or comma-separated list (react,vue,solid,svelte)",
		},
		json: { type: "boolean", default: false, description: "Output as JSON envelope" },
		quiet: { type: "boolean", default: false, description: "Minimal output" },
	},
	async run({ args }) {
		await runBrowserCommand(
			args,
			async (client) => {
				const mode = resolveOutputMode(args);
				const info = await client.call<BrowserSessionInfo>("browser.start", {
					port: Number.parseInt(args.port, 10),
					profile: args.profile,
					attach: args.attach,
					allTabs: args["all-tabs"],
					tabFilter: args.tab,
					url: args.url,
					screenshotIntervalMs: args["screenshot-interval"] ? Number.parseInt(args["screenshot-interval"], 10) : undefined,
					frameworkState: parseFrameworkState(args["framework-state"]),
				});
				process.stdout.write(`${formatBrowserSession(info, mode)}\n`);
			},
			30_000,
		);
	},
});

export const browserMarkCommand = defineCommand({
	meta: {
		name: "mark",
		description: "Place a marker in the browser recording buffer",
	},
	args: {
		label: {
			type: "positional",
			description: "Label for the marker",
			required: false,
		},
		json: { type: "boolean", default: false, description: "Output as JSON envelope" },
		quiet: { type: "boolean", default: false, description: "Minimal output" },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const marker = await client.call<Marker>("browser.mark", {
				label: args.label,
			});
			if (mode === "json") {
				process.stdout.write(`${successEnvelope({ id: marker.id, timestamp: new Date(marker.timestamp).toISOString(), label: marker.label })}\n`);
			} else {
				const time = new Date(marker.timestamp).toLocaleTimeString();
				const label = marker.label ? `"${marker.label}"` : "(unlabeled)";
				process.stdout.write(`Marker placed: ${label} at ${time}\n`);
			}
		});
	},
});

export const browserStatusCommand = defineCommand({
	meta: {
		name: "status",
		description: "Show browser recording status",
	},
	args: {
		json: { type: "boolean", default: false, description: "Output as JSON envelope" },
		quiet: { type: "boolean", default: false, description: "Minimal output" },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const info = await client.call<BrowserSessionInfo | null>("browser.status", {});
			if (!info) {
				if (mode === "json") {
					process.stdout.write(`${successEnvelope({ active: false })}\n`);
				} else {
					process.stdout.write("No active browser recording. Run `krometrail browser start` to begin.\n");
				}
				return;
			}
			process.stdout.write(`${formatBrowserSession(info, mode)}\n`);
		});
	},
});

export const browserStopCommand = defineCommand({
	meta: {
		name: "stop",
		description: "Stop browser recording",
	},
	args: {
		"close-browser": {
			type: "boolean",
			description: "Also close the Chrome browser",
			default: false,
		},
		json: { type: "boolean", default: false, description: "Output as JSON envelope" },
		quiet: { type: "boolean", default: false, description: "Minimal output" },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			await client.call("browser.stop", {
				closeBrowser: args["close-browser"],
			});
			if (mode === "json") {
				process.stdout.write(`${successEnvelope({ stopped: true })}\n`);
			} else {
				process.stdout.write("Browser recording stopped.\n");
			}
		});
	},
});

export const browserSessionsCommand = defineCommand({
	meta: {
		name: "sessions",
		description: "List recorded browser sessions",
	},
	args: {
		"has-markers": { type: "boolean", description: "Only sessions with markers" },
		"has-errors": { type: "boolean", description: "Only sessions with errors" },
		after: { type: "string", description: "Only sessions after this date (ISO timestamp)" },
		before: { type: "string", description: "Only sessions before this date (ISO timestamp)" },
		limit: { type: "string", description: "Max results (default: 10)" },
		"url-contains": { type: "string", description: "Filter sessions by URL pattern" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const sessions = await client.call<SessionSummary[]>("browser.sessions", {
				hasMarkers: args["has-markers"],
				hasErrors: args["has-errors"],
				after: args.after ? new Date(args.after).getTime() : undefined,
				before: args.before ? new Date(args.before).getTime() : undefined,
				limit: args.limit ? Number.parseInt(args.limit, 10) : 10,
				urlContains: args["url-contains"],
			});
			process.stdout.write(`${formatBrowserSessions(sessions, mode)}\n`);
		});
	},
});

export const browserOverviewCommand = defineCommand({
	meta: {
		name: "overview",
		description: "Get a structured overview of a recorded browser session",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		"around-marker": { type: "string", description: "Center on marker ID" },
		"token-budget": { type: "string", description: "Token budget (default: 3000)" },
		include: { type: "string", description: "Comma-separated: timeline,markers,errors,network_summary,framework (default: all)" },
		"time-range": { type: "string", description: "Time range as START..END (ISO timestamps or HH:MM:SS)" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			// Parse time-range: "start..end"
			let timeRange: { start: string; end: string } | undefined;
			if (args["time-range"]) {
				const parts = args["time-range"].split("..");
				if (parts.length === 2) timeRange = { start: parts[0].trim(), end: parts[1].trim() };
			}
			const text = await client.call<string>("browser.overview", {
				sessionId: args.id,
				aroundMarker: args["around-marker"],
				tokenBudget: args["token-budget"] ? Number.parseInt(args["token-budget"], 10) : 3000,
				include: splitComma(args.include),
				timeRange,
			});
			process.stdout.write(`${formatInvestigation(text, "overview", mode)}\n`);
		});
	},
});

export const browserSearchCommand = defineCommand({
	meta: {
		name: "search",
		description: "Search recorded browser session events",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		query: { type: "string", description: "Natural language search query" },
		"status-codes": { type: "string", description: "Filter by HTTP status codes (comma-separated, e.g. 422,500)" },
		"event-types": { type: "string", description: "Filter by event types (comma-separated)" },
		"around-marker": { type: "string", description: "Center search around marker ID" },
		"url-pattern": { type: "string", description: "Glob pattern for URL filtering (e.g. '**/api/**')" },
		"console-levels": { type: "string", description: "Console levels, comma-separated (e.g. 'error,warn')" },
		"contains-text": { type: "string", description: "Case-insensitive substring match on event summary" },
		framework: { type: "string", description: "Filter by framework: react, vue, solid, svelte" },
		component: { type: "string", description: "Filter by component name (substring match)" },
		pattern: { type: "string", description: "Filter by bug pattern (e.g. 'stale_closure', 'infinite_rerender')" },
		"max-results": { type: "string", description: "Max results (default: 10)", default: "10" },
		"token-budget": { type: "string", description: "Token budget (default: 2000)" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const text = await client.call<string>("browser.search", {
				sessionId: args.id,
				query: args.query,
				statusCodes: args["status-codes"]
					? args["status-codes"]
							.split(",")
							.map((s) => Number.parseInt(s.trim(), 10))
							.filter(Number.isFinite)
					: undefined,
				eventTypes: splitComma(args["event-types"]),
				aroundMarker: args["around-marker"],
				urlPattern: args["url-pattern"],
				consoleLevels: splitComma(args["console-levels"]),
				containsText: args["contains-text"],
				framework: args.framework,
				component: args.component,
				pattern: args.pattern,
				maxResults: args["max-results"] ? Number.parseInt(args["max-results"], 10) : 10,
				tokenBudget: args["token-budget"] ? Number.parseInt(args["token-budget"], 10) : 2000,
			});
			process.stdout.write(`${formatInvestigation(text, "search", mode)}\n`);
		});
	},
});

export const browserInspectCommand = defineCommand({
	meta: {
		name: "inspect",
		description: "Deep-dive into a specific event or moment in a recorded browser session",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		event: { type: "string", description: "Event ID to inspect" },
		marker: { type: "string", description: "Marker ID to jump to" },
		timestamp: { type: "string", description: "ISO timestamp to inspect nearest moment" },
		include: { type: "string", description: "Comma-separated: surrounding_events,network_body,screenshot,form_state,console_context (default: all)" },
		"context-window": { type: "string", description: "Seconds of surrounding events (default: 5)" },
		"token-budget": { type: "string", description: "Token budget (default: 3000)" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const text = await client.call<string>("browser.inspect", {
				sessionId: args.id,
				eventId: args.event,
				markerId: args.marker,
				timestamp: args.timestamp ? new Date(args.timestamp).getTime() : undefined,
				include: splitComma(args.include),
				contextWindow: args["context-window"] ? Number.parseInt(args["context-window"], 10) : 5,
				tokenBudget: args["token-budget"] ? Number.parseInt(args["token-budget"], 10) : 3000,
			});
			process.stdout.write(`${formatInvestigation(text, "inspect", mode)}\n`);
		});
	},
});

export const browserDiffCommand = defineCommand({
	meta: {
		name: "diff",
		description: "Compare two moments in a recorded browser session",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		from: { type: "string", description: "First moment — timestamp (ISO or HH:MM:SS) or event ID", required: true },
		to: { type: "string", description: "Second moment — timestamp (ISO or HH:MM:SS) or event ID", required: true },
		include: { type: "string", description: "Comma-separated: form_state,storage,url,console_new,network_new,framework_state (default: all except framework_state)" },
		"token-budget": { type: "string", description: "Token budget (default: 2000)" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const text = await client.call<string>("browser.diff", {
				sessionId: args.id,
				before: args.from,
				after: args.to,
				include: splitComma(args.include),
				tokenBudget: args["token-budget"] ? Number.parseInt(args["token-budget"], 10) : 2000,
			});
			process.stdout.write(`${formatInvestigation(text, "diff", mode)}\n`);
		});
	},
});

export const browserReplayContextCommand = defineCommand({
	meta: {
		name: "replay-context",
		description: "Generate a reproduction context from a recorded browser session",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		"around-marker": { type: "string", description: "Focus on events around this marker ID" },
		format: { type: "string", description: "Output format: summary, reproduction_steps, test_scaffold (default: reproduction_steps)" },
		framework: { type: "string", description: "Test framework for scaffold: playwright or cypress (default: playwright)" },
		json: { type: "boolean", description: "JSON output", default: false },
		quiet: { type: "boolean", description: "Minimal output", default: false },
	},
	async run({ args }) {
		await runBrowserCommand(args, async (client) => {
			const mode = resolveOutputMode(args);
			const format = (args.format ?? "reproduction_steps") as "summary" | "reproduction_steps" | "test_scaffold";
			const text = await client.call<string>("browser.replay-context", {
				sessionId: args.id,
				aroundMarker: args["around-marker"],
				format,
				testFramework: args.framework ?? "playwright",
			});
			process.stdout.write(`${formatInvestigation(text, "replay-context", mode)}\n`);
		});
	},
});

export const browserExportCommand = defineCommand({
	meta: {
		name: "export",
		description: "Export a recorded browser session (HAR format)",
	},
	args: {
		id: { type: "positional", description: "Session ID", required: true },
		format: { type: "string", description: "Export format: har (default: har)" },
		output: { type: "string", description: "Output file path (default: stdout)" },
	},
	async run({ args }) {
		const client = await getClient();
		try {
			const text = await client.call<string>("browser.export", {
				sessionId: args.id,
				format: args.format ?? "har",
			});
			if (args.output) {
				const { writeFileSync } = await import("node:fs");
				writeFileSync(args.output, text, "utf-8");
				process.stdout.write(`Exported to ${args.output}\n`);
			} else {
				process.stdout.write(`${text}\n`);
			}
		} catch (err) {
			process.stderr.write(`Error: ${(err as Error).message}\n`);
			process.exit(1);
		} finally {
			client.dispose();
		}
	},
});

export const browserCommand = defineCommand({
	meta: {
		name: "browser",
		description: "Browser recording (CDP recorder — passive observer for network, console, and user input events)",
	},
	subCommands: {
		start: browserStartCommand,
		mark: browserMarkCommand,
		status: browserStatusCommand,
		stop: browserStopCommand,
		sessions: browserSessionsCommand,
		overview: browserOverviewCommand,
		search: browserSearchCommand,
		inspect: browserInspectCommand,
		diff: browserDiffCommand,
		"replay-context": browserReplayContextCommand,
		export: browserExportCommand,
	},
});
