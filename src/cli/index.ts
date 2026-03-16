#!/usr/bin/env bun
import { defineCommand, runMain, showUsage } from "citty";
import pkg from "../../package.json";
import { sendPing } from "../telemetry.js";
import { browserCommand } from "./commands/browser.js";
import { commandsCommand } from "./commands/commands.js";
import { debugCommand } from "./commands/debug.js";
import { doctorCommand } from "./commands/doctor.js";

const main = defineCommand({
	meta: {
		name: "krometrail",
		version: pkg.version,
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
	async run({ args, cmd, rawArgs }) {
		if (args.mcp) {
			sendPing("mcp_start"); // fire-and-forget
			const { startMcpServer } = await import("../mcp/index.js");
			const { parseToolGroups } = await import("../mcp/tool-groups.js");
			await startMcpServer({ toolGroups: parseToolGroups(args.tools) });
			return;
		}
		// Citty always calls the parent run() even when a subcommand is dispatched.
		// Only show usage when no subcommand was provided.
		const subCommandNames = ["debug", "browser", "doctor", "commands", "completions", "_daemon"];
		const hasSubCommand = rawArgs.some((a) => subCommandNames.includes(a));
		if (hasSubCommand) return;
		sendPing("run"); // fire-and-forget
		await showUsage(cmd);
	},
	subCommands: {
		debug: debugCommand,
		browser: browserCommand,
		doctor: doctorCommand,
		commands: commandsCommand,
		completions: () => import("./commands/completions.js").then((m) => m.completionsCommand),
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
