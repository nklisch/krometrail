import { readFile } from "node:fs/promises";
import { z } from "zod";
import { getErrorMessage, InvalidLaunchConfigError } from "./errors.js";
import type { AttachOptions, LaunchOptions } from "./session-manager.js";

/**
 * A single launch configuration from .vscode/launch.json.
 * Index signature allows arbitrary VS Code extension fields.
 */
export interface LaunchJsonConfig {
	name: string;
	type: string;
	request: "launch" | "attach";
	program?: string;
	args?: string[];
	cwd?: string;
	env?: Record<string, string>;
	port?: number;
	host?: string;
	/** Python-specific: run as module */
	module?: string;
	/** Go-specific: debug mode */
	mode?: string;
	/** Node-specific */
	runtimeExecutable?: string;
	/** Node-specific */
	runtimeArgs?: string[];
	[key: string]: unknown;
}

/**
 * Parsed launch.json file structure.
 */
export interface LaunchJsonFile {
	version: string;
	configurations: LaunchJsonConfig[];
}

/**
 * Zod schema for a single launch.json configuration entry.
 */
export const LaunchJsonConfigSchema: z.ZodType<LaunchJsonConfig> = z
	.object({
		name: z.string(),
		type: z.string(),
		request: z.enum(["launch", "attach"]),
		program: z.string().optional(),
		args: z.array(z.string()).optional(),
		cwd: z.string().optional(),
		env: z.record(z.string(), z.string()).optional(),
		port: z.number().optional(),
		host: z.string().optional(),
		module: z.string().optional(),
		mode: z.string().optional(),
		runtimeExecutable: z.string().optional(),
		runtimeArgs: z.array(z.string()).optional(),
	})
	.passthrough() as z.ZodType<LaunchJsonConfig>;

const LaunchJsonFileSchema = z.object({
	version: z.string().default("0.2.0"),
	configurations: z.array(LaunchJsonConfigSchema),
});

/**
 * Strip JSONC features (line comments, block comments, trailing commas)
 * to produce valid JSON. VS Code's launch.json commonly uses these.
 *
 * Does not strip // or /* inside string literals.
 */
export function stripJsonc(input: string): string {
	let result = "";
	let i = 0;

	while (i < input.length) {
		const ch = input[i];

		// String literal — copy verbatim until closing quote (handle escapes)
		if (ch === '"') {
			result += ch;
			i++;
			while (i < input.length) {
				const c = input[i];
				result += c;
				if (c === "\\") {
					// Escaped character — include the next char too
					i++;
					if (i < input.length) {
						result += input[i];
					}
				} else if (c === '"') {
					break;
				}
				i++;
			}
			i++;
			continue;
		}

		// Line comment: // ...
		if (ch === "/" && i + 1 < input.length && input[i + 1] === "/") {
			// Skip until newline
			while (i < input.length && input[i] !== "\n") {
				i++;
			}
			continue;
		}

		// Block comment: /* ... */
		if (ch === "/" && i + 1 < input.length && input[i + 1] === "*") {
			i += 2;
			while (i < input.length) {
				if (input[i] === "*" && i + 1 < input.length && input[i + 1] === "/") {
					i += 2;
					break;
				}
				i++;
			}
			continue;
		}

		result += ch;
		i++;
	}

	// Remove trailing commas before ] or } (with optional whitespace between)
	result = result.replace(/,(\s*[}\]])/g, "$1");

	return result;
}

/**
 * Read and parse a .vscode/launch.json file.
 * Handles JSONC (comments and trailing commas) since VS Code allows them.
 * Returns null if the file doesn't exist.
 */
export async function parseLaunchJson(filePath: string): Promise<LaunchJsonFile | null> {
	let raw: string;
	try {
		raw = await readFile(filePath, "utf8");
	} catch (err) {
		if ((err as NodeJS.ErrnoException).code === "ENOENT") {
			return null;
		}
		throw new InvalidLaunchConfigError(`Failed to read launch.json at ${filePath}: ${getErrorMessage(err)}`);
	}

	const stripped = stripJsonc(raw);

	let parsed: unknown;
	try {
		parsed = JSON.parse(stripped);
	} catch (err) {
		throw new InvalidLaunchConfigError(`Failed to parse launch.json at ${filePath}: ${getErrorMessage(err)}`);
	}

	const result = LaunchJsonFileSchema.safeParse(parsed);
	if (!result.success) {
		throw new InvalidLaunchConfigError(`Invalid launch.json format at ${filePath}: ${result.error.message}`);
	}

	return result.data;
}

/**
 * List available configuration names from a launch.json.
 */
export function listConfigurations(launchJson: LaunchJsonFile): Array<{ name: string; type: string; request: string }> {
	return launchJson.configurations.map((c) => ({ name: c.name, type: c.type, request: c.request }));
}

/**
 * VS Code debugger type → krometrail adapter id mapping.
 */
const TYPE_TO_LANGUAGE: Record<string, string> = {
	python: "python",
	debugpy: "python",
	"python-debugger": "python",
	node: "node",
	node2: "node",
	"pwa-node": "node",
	"pwa-extensionHost": "node",
	chrome: "node",
	"pwa-chrome": "node",
	go: "go",
	rust: "rust",
	lldb: "rust",
	codelldb: "rust",
	java: "java",
	cppdbg: "cpp",
	cppvsdbg: "cpp",
	gdb: "cpp",
	lldb_dap: "cpp",
};

const SUPPORTED_TYPES = Object.keys(TYPE_TO_LANGUAGE);

/**
 * Replace VS Code variable ${workspaceFolder} with the given directory.
 * Other VS Code variables (${env:VAR}, ${file}, etc.) are NOT substituted.
 */
function substituteWorkspaceFolder(value: string, cwd: string): string {
	return value.replace(/\$\{workspaceFolder\}/g, cwd);
}

/**
 * Convert a launch.json configuration to LaunchOptions or AttachOptions.
 * Returns { type: "launch", options } or { type: "attach", options }.
 * Throws if the configuration type is not supported.
 */
export function configToOptions(config: LaunchJsonConfig, workspaceFolder?: string): { type: "launch"; options: LaunchOptions } | { type: "attach"; options: AttachOptions } {
	const language = TYPE_TO_LANGUAGE[config.type.toLowerCase()];
	if (!language) {
		throw new InvalidLaunchConfigError(`Unsupported launch.json type: "${config.type}". Supported types: ${SUPPORTED_TYPES.join(", ")}`);
	}

	const cwd = config.cwd ? substituteWorkspaceFolder(config.cwd, workspaceFolder ?? process.cwd()) : (workspaceFolder ?? process.cwd());

	if (config.request === "attach") {
		const options: AttachOptions = {
			language,
			pid: config.pid as number | undefined,
			port: config.port,
			host: config.host,
			cwd,
			env: config.env,
		};
		return { type: "attach", options };
	}

	// Launch mode — build the command string
	let command: string;

	if (language === "python") {
		if (config.module) {
			// python -m module [args]
			const args = config.args?.join(" ") ?? "";
			command = `python3 -m ${config.module}${args ? ` ${args}` : ""}`;
		} else if (config.program) {
			const program = substituteWorkspaceFolder(config.program, cwd);
			const args = config.args?.join(" ") ?? "";
			command = `python3 ${program}${args ? ` ${args}` : ""}`;
		} else {
			throw new InvalidLaunchConfigError(`Python launch config "${config.name}" requires either "program" or "module"`);
		}
	} else if (language === "node") {
		const runtime = config.runtimeExecutable ?? "node";
		const runtimeArgs = config.runtimeArgs?.join(" ") ?? "";
		const program = config.program ? substituteWorkspaceFolder(config.program, cwd) : "";
		const args = config.args?.join(" ") ?? "";
		command = `${runtime}${runtimeArgs ? ` ${runtimeArgs}` : ""}${program ? ` ${program}` : ""}${args ? ` ${args}` : ""}`;
	} else if (language === "go") {
		const program = config.program ? substituteWorkspaceFolder(config.program, cwd) : ".";
		const mode = config.mode ?? "debug";
		const args = config.args?.join(" ") ?? "";
		if (mode === "test") {
			command = `go test ${program}${args ? ` ${args}` : ""}`;
		} else {
			command = `go run ${program}${args ? ` ${args}` : ""}`;
		}
	} else if (language === "rust") {
		const program = config.program ? substituteWorkspaceFolder(config.program, cwd) : "";
		const args = config.args?.join(" ") ?? "";
		command = program ? `${program}${args ? ` ${args}` : ""}` : `cargo run${args ? ` ${args}` : ""}`;
	} else if (language === "java") {
		const program = config.program ? substituteWorkspaceFolder(config.program, cwd) : "";
		const args = config.args?.join(" ") ?? "";
		command = program ? `java ${program}${args ? ` ${args}` : ""}` : `java${args ? ` ${args}` : ""}`;
	} else if (language === "cpp") {
		const program = config.program ? substituteWorkspaceFolder(config.program, cwd) : "";
		const args = config.args?.join(" ") ?? "";
		command = program ? `${program}${args ? ` ${args}` : ""}` : "";
		if (!command) {
			throw new InvalidLaunchConfigError(`C/C++ launch config "${config.name}" requires "program"`);
		}
	} else {
		throw new InvalidLaunchConfigError(`Cannot build command for language: ${language}`);
	}

	const options: LaunchOptions = {
		command: command.trim(),
		language,
		cwd,
		env: config.env,
	};

	return { type: "launch", options };
}
