#!/usr/bin/env bun
import { defineCommand, runMain } from "citty";
import { browserCommand } from "./commands/browser.js";
import { commandsCommand } from "./commands/commands.js";
import { debugCommand } from "./commands/debug.js";
import { doctorCommand } from "./commands/doctor.js";

const main = defineCommand({
	meta: {
		name: "krometrail",
		version: "0.1.0",
		description: "Runtime debugging viewport for AI coding agents",
	},
	args: {
		mcp: {
			type: "boolean",
			description: "Start as an MCP server on stdio instead of running the CLI",
			default: false,
		},
		tools: {
			type: "string",
			description: "Comma-separated tool groups to expose (debug, browser). Default: all. Only used with --mcp.",
		},
	},
	async run({ args }) {
		if (args.mcp) {
			const { startMcpServer } = await import("../mcp/index.js");
			const { parseToolGroups } = await import("../mcp/tool-groups.js");
			await startMcpServer({ toolGroups: parseToolGroups(args.tools) });
			return;
		}
		// citty shows help by default when no subcommand given
	},
	subCommands: {
		debug: debugCommand,
		browser: browserCommand,
		doctor: doctorCommand,
		commands: commandsCommand,
		// Hidden: internal daemon entry point
		_daemon: () =>
			defineCommand({
				meta: { hidden: true },
				async run() {
					await import("../daemon/entry.js");
				},
			}),
	},
});

runMain(main);
