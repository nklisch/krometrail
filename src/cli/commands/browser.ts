import { defineCommand } from "citty";
import type { BrowserSessionInfo, Marker } from "../../browser/types.js";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import { getDaemonSocketPath } from "../../daemon/protocol.js";

/**
 * Create a DaemonClient ensuring the daemon is running.
 * Uses a longer timeout for browser.start since Chrome launch may take a few seconds.
 */
async function getClient(timeoutMs = 30_000): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: timeoutMs });
}

function formatSessionInfo(info: BrowserSessionInfo): string {
	const lines: string[] = [];
	const startedAt = new Date(info.startedAt).toLocaleTimeString();
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
			description: "Chrome profile name (creates isolated user-data-dir under ~/.agent-lens/chrome-profiles/)",
		},
		attach: {
			type: "boolean",
			description: "Attach to an already-running Chrome instance (don't launch Chrome)",
			default: false,
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
	},
	async run({ args }) {
		const client = await getClient(30_000);
		try {
			const info = await client.call<BrowserSessionInfo>("browser.start", {
				port: Number.parseInt(args.port, 10),
				profile: args.profile,
				attach: args.attach,
				allTabs: args["all-tabs"],
				tabFilter: args.tab,
			});
			process.stdout.write(`${formatSessionInfo(info)}\n`);
		} catch (err) {
			process.stderr.write(`Error: ${(err as Error).message}\n`);
			process.exit(1);
		} finally {
			client.dispose();
		}
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
	},
	async run({ args }) {
		const client = await getClient();
		try {
			const marker = await client.call<Marker>("browser.mark", {
				label: args.label,
			});
			const time = new Date(marker.timestamp).toLocaleTimeString();
			const label = marker.label ? `"${marker.label}"` : "(unlabeled)";
			process.stdout.write(`Marker placed: ${label} at ${time}\n`);
		} catch (err) {
			process.stderr.write(`Error: ${(err as Error).message}\n`);
			process.exit(1);
		} finally {
			client.dispose();
		}
	},
});

export const browserStatusCommand = defineCommand({
	meta: {
		name: "status",
		description: "Show browser recording status",
	},
	async run() {
		const client = await getClient();
		try {
			const info = await client.call<BrowserSessionInfo | null>("browser.status", {});
			if (!info) {
				process.stdout.write("No active browser recording. Run `agent-lens browser start` to begin.\n");
				return;
			}
			process.stdout.write(`${formatSessionInfo(info)}\n`);
		} catch (err) {
			process.stderr.write(`Error: ${(err as Error).message}\n`);
			process.exit(1);
		} finally {
			client.dispose();
		}
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
	},
	async run({ args }) {
		const client = await getClient();
		try {
			await client.call("browser.stop", {
				closeBrowser: args["close-browser"],
			});
			process.stdout.write("Browser recording stopped.\n");
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
	},
});
