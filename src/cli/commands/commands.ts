import { defineCommand } from "citty";
import { successEnvelope } from "../envelope.js";
import { resolveOutputMode } from "../format.js";

export interface CommandArgInfo {
	name: string;
	type: "positional" | "string" | "boolean";
	required: boolean;
	alias?: string;
	description: string;
	default?: unknown;
}

export interface CommandInfo {
	name: string;
	description: string;
	group: string;
	args: CommandArgInfo[];
}

export interface CommandsData {
	version: string;
	groups: Array<{
		name: string;
		description: string;
		commands: CommandInfo[];
	}>;
}

/**
 * Extract args from a citty command definition's args object.
 */
function extractArgs(argsObj: Record<string, unknown> | undefined): CommandArgInfo[] {
	if (!argsObj) return [];
	const result: CommandArgInfo[] = [];
	for (const [name, def] of Object.entries(argsObj)) {
		if (typeof def !== "object" || def === null) continue;
		const d = def as Record<string, unknown>;
		result.push({
			name,
			type: (d.type as CommandArgInfo["type"]) ?? "string",
			required: (d.required as boolean) ?? false,
			alias: d.alias as string | undefined,
			description: (d.description as string) ?? "",
			default: d.default,
		});
	}
	return result;
}

/**
 * Build the command inventory from the citty command definitions.
 * Dynamically imports debug and browser commands to avoid circular deps at import time.
 */
export async function buildCommandInventory(groupFilter?: string): Promise<CommandsData> {
	const { debugCommand } = await import("./debug.js");
	const { browserCommand } = await import("./browser.js");
	const { doctorCommand } = await import("./doctor.js");

	const version = "0.1.0";

	// Helper: extract subcommands from a group command
	function extractGroupCommands(groupCmd: ReturnType<typeof defineCommand>, groupName: string): CommandInfo[] {
		const subCmds = (groupCmd as unknown as { _subCommandsRaw?: Record<string, unknown>; subCommands?: Record<string, unknown> }).subCommands;
		if (!subCmds) return [];
		const commands: CommandInfo[] = [];
		for (const [cmdName, cmdDef] of Object.entries(subCmds)) {
			if (typeof cmdDef !== "object" || cmdDef === null) continue;
			const cmd = cmdDef as Record<string, unknown>;
			const meta = (cmd.meta as Record<string, unknown>) ?? {};
			const description = (meta.description as string) ?? "";
			const argsObj = (cmd.args as Record<string, unknown>) ?? {};
			commands.push({
				name: cmdName,
				description,
				group: groupName,
				args: extractArgs(argsObj),
			});
		}
		return commands;
	}

	const allGroups: CommandsData["groups"] = [
		{
			name: "debug",
			description: "Debug commands (launch, step, eval, ...)",
			commands: extractGroupCommands(debugCommand, "debug"),
		},
		{
			name: "browser",
			description: "Browser recording commands",
			commands: extractGroupCommands(browserCommand, "browser"),
		},
		{
			name: "top-level",
			description: "Top-level commands",
			commands: [
				{
					name: "doctor",
					description: (((doctorCommand as unknown as Record<string, unknown>).meta as Record<string, unknown>)?.description as string) ?? "Check installed debuggers",
					group: "top-level",
					args: extractArgs((doctorCommand as unknown as Record<string, unknown>).args as Record<string, unknown>),
				},
				{
					name: "commands",
					description: "List all available commands (machine-readable)",
					group: "top-level",
					args: [
						{ name: "json", type: "boolean", required: false, description: "Output as JSON (default: true for this command)", default: true },
						{ name: "group", type: "string", required: false, description: "Filter by command group: debug, browser, or all" },
					],
				},
			],
		},
	];

	const groups = groupFilter && groupFilter !== "all" ? allGroups.filter((g) => g.name === groupFilter) : allGroups;

	return { version, groups };
}

export const commandsCommand = defineCommand({
	meta: { name: "commands", description: "List all available commands (machine-readable)" },
	args: {
		json: {
			type: "boolean",
			description: "Output as JSON (default: true for this command)",
			default: true,
		},
		quiet: {
			type: "boolean",
			description: "Minimal output",
			default: false,
		},
		group: {
			type: "string",
			description: "Filter by command group: debug, browser, or all",
		},
	},
	async run({ args }) {
		const mode = resolveOutputMode(args);
		const inventory = await buildCommandInventory(args.group);

		if (mode !== "json" && !args.json) {
			// Text mode: compact table
			for (const group of inventory.groups) {
				process.stdout.write(`\n${group.name.toUpperCase()}\n`);
				for (const cmd of group.commands) {
					const flags = cmd.args
						.filter((a) => a.type !== "positional")
						.map((a) => `--${a.name}`)
						.join(" ");
					process.stdout.write(`  ${cmd.name.padEnd(20)}${cmd.description.padEnd(50)}${flags}\n`);
				}
			}
		} else {
			// JSON mode (default)
			process.stdout.write(`${successEnvelope(inventory)}\n`);
		}
	},
});
