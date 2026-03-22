import { resolve as resolvePath } from "node:path";
import { defineCommand } from "citty";
import { listAdapters, registerAllAdapters } from "../../adapters/registry.js";
import { STEP_DIRECTIONS, type StepDirection } from "../../core/enums.js";
import { configToOptions, listConfigurations, parseLaunchJson } from "../../core/launch-json.js";
import type { BreakpointsListPayload, BreakpointsResultPayload, LaunchResultPayload, StatusResultPayload, StopResultPayload, ThreadInfoPayload, ViewportPayload } from "../../daemon/protocol.js";
import { listDetectors, registerAllDetectors } from "../../frameworks/index.js";
import { successEnvelope } from "../envelope.js";
import {
	formatBreakpointsList,
	formatBreakpointsSet,
	formatEvaluate,
	formatLaunch,
	formatLog,
	formatOutput,
	formatSource,
	formatStackTrace,
	formatStatus,
	formatStop,
	formatThreads,
	formatVariables,
	formatViewport,
	formatWatchExpressions,
} from "../format.js";
import { parseBreakpointString, parseLocation, parseSourceRange } from "../parsers.js";
import { globalArgs, runCommand } from "./shared.js";

// Register adapters/detectors so we can derive descriptions from the live registry.
// Adapter instantiation is lightweight (no side effects until launch/attach is called).
registerAllAdapters();
registerAllDetectors();

function languageDescription(prefix = "Language"): string {
	const parts = listAdapters().map((a) => [a.id, ...(a.aliases ?? [])].join("/"));
	return `${prefix}. Supported: ${parts.join(", ")}`;
}

function frameworkDescription(): string {
	const ids = listDetectors().map((d) => d.id);
	return `Override framework auto-detection. Known: ${ids.join(", ")}. Use 'none' to disable.`;
}

/**
 * Parse KEY=VAL pairs from a comma-separated or repeated env string.
 * Handles "KEY1=VAL1,KEY2=VAL2" or multiple --env flags.
 */
function parseEnvString(envStr: string): Record<string, string> {
	const result: Record<string, string> = {};
	// Support comma-separated pairs
	const pairs = envStr
		.split(",")
		.map((s) => s.trim())
		.filter(Boolean);
	for (const pair of pairs) {
		const eqIdx = pair.indexOf("=");
		if (eqIdx === -1) continue;
		const key = pair.slice(0, eqIdx).trim();
		const val = pair.slice(eqIdx + 1).trim();
		if (key) result[key] = val;
	}
	return result;
}

/**
 * Build a viewportConfig partial from viewport-related CLI flags.
 */
function buildViewportConfig(args: { "source-lines"?: string; "stack-depth"?: string; "locals-depth"?: string; "token-budget"?: string; "diff-mode"?: boolean }): Record<string, unknown> | undefined {
	const config: Record<string, unknown> = {};
	if (args["source-lines"] !== undefined) config.sourceLines = Number(args["source-lines"]);
	if (args["stack-depth"] !== undefined) config.stackDepth = Number(args["stack-depth"]);
	if (args["locals-depth"] !== undefined) config.localsDepth = Number(args["locals-depth"]);
	if (args["token-budget"] !== undefined) config.tokenBudget = Number(args["token-budget"]);
	if (args["diff-mode"]) config.diffMode = true;
	return Object.keys(config).length > 0 ? config : undefined;
}

function collectExpressions(firstExpr: string, args: Record<string, unknown>): string[] {
	const extraArgs = args._ as string[] | undefined;
	return [firstExpr, ...(extraArgs ?? [])];
}

// --- Session Lifecycle ---

export const launchCommand = defineCommand({
	meta: { name: "launch", description: "Launch a debug session" },
	args: {
		command: {
			type: "positional",
			description: "Command to debug, e.g. 'python app.py' or 'pytest tests/'",
			required: false,
		},
		break: {
			type: "string",
			description: "Set breakpoint(s), e.g. 'order.py:147' or 'order.py:147 when discount < 0'",
			alias: "b",
		},
		language: {
			type: "string",
			description: languageDescription("Override language detection"),
		},
		framework: {
			type: "string",
			description: frameworkDescription(),
		},
		"stop-on-entry": {
			type: "boolean",
			description: "Pause on first executable line",
			default: false,
		},
		config: {
			type: "string",
			description: "Path to launch.json file (default: .vscode/launch.json)",
		},
		"config-name": {
			type: "string",
			description: "Name of the configuration to use from launch.json",
			alias: "C",
		},
		cwd: {
			type: "string",
			description: "Working directory for the debug target",
		},
		env: {
			type: "string",
			description: "Environment variables as KEY=VAL pairs (comma-separated, e.g. DEBUG=1,LOG=verbose)",
		},
		"source-lines": {
			type: "string",
			description: "Lines of source context above/below current line (default: 15)",
		},
		"stack-depth": {
			type: "string",
			description: "Max call stack frames to show (default: 5)",
		},
		"locals-depth": {
			type: "string",
			description: "Object expansion depth for locals (default: 1)",
		},
		"token-budget": {
			type: "string",
			description: "Approximate token budget for viewport output (default: 8000)",
		},
		"diff-mode": {
			type: "boolean",
			description: "Show only changed variables vs previous stop",
			default: false,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(
			args,
			async (client, _sessionId, mode) => {
				const breakpoints = args.break ? [parseBreakpointString(args.break)] : undefined;
				const env = args.env ? parseEnvString(args.env) : undefined;
				const viewportConfig = buildViewportConfig(args);

				if (!args.config && !args["config-name"]) {
					if (!args.command) {
						throw new Error('Usage: krometrail debug launch "<command>" or krometrail debug launch --config-name "<name>"');
					}
					const result = await client.call<LaunchResultPayload>("session.launch", {
						command: args.command,
						language: args.language,
						framework: args.framework,
						breakpoints: breakpoints?.map((fb) => ({
							file: fb.file,
							breakpoints: fb.breakpoints,
						})),
						stopOnEntry: args["stop-on-entry"],
						cwd: args.cwd,
						env,
						viewportConfig,
					});
					process.stdout.write(`${formatLaunch(result, mode)}\n`);
					return;
				}

				// Load from launch.json
				const configPath = args.config ? resolvePath(args.config) : resolvePath(process.cwd(), ".vscode/launch.json");
				const launchJson = await parseLaunchJson(configPath);
				if (!launchJson) {
					throw new Error(`launch.json not found at: ${configPath}`);
				}

				let configEntry: (typeof launchJson.configurations)[0];
				if (args["config-name"]) {
					const found = launchJson.configurations.find((c) => c.name === args["config-name"]);
					if (!found) {
						const available = listConfigurations(launchJson)
							.map((c) => `  "${c.name}"`)
							.join("\n");
						throw new Error(`Configuration "${args["config-name"]}" not found. Available:\n${available}`);
					}
					configEntry = found;
				} else {
					if (launchJson.configurations.length === 1) {
						configEntry = launchJson.configurations[0];
					} else {
						const available = listConfigurations(launchJson)
							.map((c) => `  "${c.name}"`)
							.join("\n");
						throw new Error(`Multiple configurations found. Use --config-name to select one:\n${available}`);
					}
				}

				const converted = configToOptions(configEntry, process.cwd());
				if (converted.type === "attach") {
					const result = await client.call<LaunchResultPayload>("session.attach", {
						language: args.language ?? converted.options.language,
						pid: converted.options.pid,
						port: converted.options.port,
						host: converted.options.host,
						breakpoints: breakpoints?.map((fb) => ({ file: fb.file, breakpoints: fb.breakpoints })),
					});
					process.stdout.write(`${formatLaunch(result, mode)}\n`);
				} else {
					const result = await client.call<LaunchResultPayload>("session.launch", {
						command: args.command ?? converted.options.command,
						language: args.language ?? converted.options.language,
						framework: args.framework,
						breakpoints: breakpoints?.map((fb) => ({ file: fb.file, breakpoints: fb.breakpoints })),
						stopOnEntry: args["stop-on-entry"],
						cwd: args.cwd ?? converted.options.cwd,
						env: env ?? converted.options.env,
						viewportConfig,
					});
					process.stdout.write(`${formatLaunch(result, mode)}\n`);
				}
			},
			{ needsSession: false },
		);
	},
});

export const stopCommand = defineCommand({
	meta: { name: "stop", description: "Terminate a debug session" },
	args: { ...globalArgs },
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<StopResultPayload>("session.stop", { sessionId });
			process.stdout.write(`${formatStop(result, sessionId, mode)}\n`);
		});
	},
});

export const statusCommand = defineCommand({
	meta: { name: "status", description: "Check session status" },
	args: { ...globalArgs },
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<StatusResultPayload>("session.status", { sessionId });
			process.stdout.write(`${formatStatus(result, mode)}\n`);
		});
	},
});

// --- Execution Control ---

export const continueCommand = defineCommand({
	meta: { name: "continue", description: "Resume execution to next breakpoint" },
	args: {
		timeout: {
			type: "string",
			description: "Max wait time in ms",
		},
		thread: {
			type: "string",
			description: "Thread ID to continue (for multi-threaded debugging)",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<ViewportPayload>("session.continue", {
				sessionId,
				timeoutMs: args.timeout ? Number.parseInt(args.timeout, 10) : undefined,
				threadId: args.thread ? Number.parseInt(args.thread, 10) : undefined,
			});
			process.stdout.write(`${formatViewport(result.viewport, mode)}\n`);
		});
	},
});

export const stepCommand = defineCommand({
	meta: { name: "step", description: "Step execution (over, into, or out)" },
	args: {
		direction: {
			type: "positional",
			description: "Step direction: over, into, or out",
			required: true,
		},
		count: {
			type: "string",
			description: "Number of steps",
		},
		thread: {
			type: "string",
			description: "Thread ID to step (for multi-threaded debugging)",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const direction = args.direction as StepDirection;
			if (!(STEP_DIRECTIONS as readonly string[]).includes(direction)) {
				throw new Error(`Invalid step direction: ${direction}. Must be 'over', 'into', or 'out'.`);
			}
			const result = await client.call<ViewportPayload>("session.step", {
				sessionId,
				direction,
				count: args.count ? Number.parseInt(args.count, 10) : undefined,
				threadId: args.thread ? Number.parseInt(args.thread, 10) : undefined,
			});
			process.stdout.write(`${formatViewport(result.viewport, mode)}\n`);
		});
	},
});

export const runToCommand = defineCommand({
	meta: { name: "run-to", description: "Run to a specific file:line" },
	args: {
		location: {
			type: "positional",
			description: "Target location, e.g. 'order.py:150'",
			required: true,
		},
		timeout: {
			type: "string",
			description: "Max wait time in ms",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const { file, line } = parseLocation(args.location);
			const result = await client.call<ViewportPayload>("session.runTo", {
				sessionId,
				file,
				line,
				timeoutMs: args.timeout ? Number.parseInt(args.timeout, 10) : undefined,
			});
			process.stdout.write(`${formatViewport(result.viewport, mode)}\n`);
		});
	},
});

// --- Breakpoints ---

export const breakCommand = defineCommand({
	meta: {
		name: "break",
		description: "Set breakpoints, exception breakpoints, or clear breakpoints",
	},
	args: {
		breakpoint: {
			type: "string",
			description: "Breakpoint spec: 'file:line[,line] [when cond] [hit cond] [log msg]'",
		},
		exceptions: {
			type: "string",
			description: "Set exception breakpoint filter (e.g. 'uncaught', 'raised')",
		},
		clear: {
			type: "string",
			description: "Clear all breakpoints in a file",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			if (args.exceptions) {
				await client.call("session.setExceptionBreakpoints", {
					sessionId,
					filters: [args.exceptions],
				});
				if (mode === "json") {
					process.stdout.write(`${successEnvelope({ filters: [args.exceptions] })}\n`);
				} else {
					process.stdout.write(`Exception breakpoints set: ${args.exceptions}\n`);
				}
			} else if (args.clear) {
				await client.call("session.setBreakpoints", {
					sessionId,
					file: args.clear,
					breakpoints: [],
				});
				if (mode === "json") {
					process.stdout.write(`${successEnvelope({ cleared: args.clear })}\n`);
				} else {
					process.stdout.write(`Breakpoints cleared: ${args.clear}\n`);
				}
			} else if (args.breakpoint) {
				const parsed = parseBreakpointString(args.breakpoint);
				const result = await client.call<BreakpointsResultPayload>("session.setBreakpoints", {
					sessionId,
					file: parsed.file,
					breakpoints: parsed.breakpoints,
				});
				process.stdout.write(`${formatBreakpointsSet(parsed.file, result, mode)}\n`);
			} else {
				throw new Error("Usage: krometrail debug break --breakpoint <file:line> | --exceptions <filter> | --clear <file>");
			}
		});
	},
});

export const breakpointsCommand = defineCommand({
	meta: { name: "breakpoints", description: "List all active breakpoints" },
	args: { ...globalArgs },
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<BreakpointsListPayload>("session.listBreakpoints", { sessionId });
			process.stdout.write(`${formatBreakpointsList(result, mode)}\n`);
		});
	},
});

// --- State Inspection ---

export const evalCommand = defineCommand({
	meta: { name: "eval", description: "Evaluate an expression" },
	args: {
		expression: {
			type: "positional",
			description: "Expression to evaluate, e.g. 'cart.items[0].__dict__'",
			required: true,
		},
		frame: {
			type: "string",
			description: "Stack frame index (0 = current)",
		},
		depth: {
			type: "string",
			description: "Object expansion depth",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<string>("session.evaluate", {
				sessionId,
				expression: args.expression,
				frameIndex: args.frame ? Number.parseInt(args.frame, 10) : undefined,
				maxDepth: args.depth ? Number.parseInt(args.depth, 10) : undefined,
			});
			process.stdout.write(`${formatEvaluate(args.expression, result, mode)}\n`);
		});
	},
});

export const varsCommand = defineCommand({
	meta: { name: "vars", description: "Show variables" },
	args: {
		scope: {
			type: "string",
			description: "Variable scope: local, global, closure, or all",
		},
		filter: {
			type: "string",
			description: "Regex filter on variable names",
		},
		frame: {
			type: "string",
			description: "Stack frame index (0 = current)",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<string>("session.variables", {
				sessionId,
				scope: args.scope,
				frameIndex: args.frame ? Number.parseInt(args.frame, 10) : undefined,
				filter: args.filter,
			});
			process.stdout.write(`${formatVariables(result, mode)}\n`);
		});
	},
});

export const stackCommand = defineCommand({
	meta: { name: "stack", description: "Show call stack" },
	args: {
		frames: {
			type: "string",
			description: "Maximum frames to show",
		},
		source: {
			type: "boolean",
			description: "Include source context per frame",
			default: false,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<string>("session.stackTrace", {
				sessionId,
				maxFrames: args.frames ? Number.parseInt(args.frames, 10) : undefined,
				includeSource: args.source,
			});
			process.stdout.write(`${formatStackTrace(result, mode)}\n`);
		});
	},
});

export const sourceCommand = defineCommand({
	meta: { name: "source", description: "View source code" },
	args: {
		target: {
			type: "positional",
			description: "File path, optionally with line range: 'file.py:15-30'",
			required: true,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const { file, startLine, endLine } = parseSourceRange(args.target);
			const result = await client.call<string>("session.source", {
				sessionId,
				file,
				startLine,
				endLine,
			});
			process.stdout.write(`${formatSource(file, result, mode)}\n`);
		});
	},
});

// --- Session Intelligence ---

export const watchCommand = defineCommand({
	meta: { name: "watch", description: "Add watch expressions" },
	args: {
		expressions: {
			type: "positional",
			description: "Expression(s) to watch",
			required: true,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const expressions = collectExpressions(args.expressions, args as Record<string, unknown>);
			const result = await client.call<string[]>("session.watch", { sessionId, expressions });
			process.stdout.write(`${formatWatchExpressions(result, mode)}\n`);
		});
	},
});

export const unwatchCommand = defineCommand({
	meta: { name: "unwatch", description: "Remove watch expressions" },
	args: {
		expressions: {
			type: "positional",
			description: "Expression(s) to stop watching",
			required: true,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const expressions = collectExpressions(args.expressions, args as Record<string, unknown>);
			const result = await client.call<string[]>("session.unwatch", { sessionId, expressions });
			process.stdout.write(`${formatWatchExpressions(result, mode)}\n`);
		});
	},
});

export const logCommand = defineCommand({
	meta: { name: "log", description: "View session investigation log" },
	args: {
		detailed: {
			type: "boolean",
			description: "Show detailed log with timestamps",
			default: false,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<string>("session.sessionLog", {
				sessionId,
				format: args.detailed ? "detailed" : "summary",
			});
			process.stdout.write(`${formatLog(result, mode)}\n`);
		});
	},
});

export const outputCommand = defineCommand({
	meta: { name: "output", description: "View captured program output" },
	args: {
		stderr: {
			type: "boolean",
			description: "Show only stderr",
			default: false,
		},
		stdout: {
			type: "boolean",
			description: "Show only stdout",
			default: false,
		},
		"since-action": {
			type: "string",
			description: "Only show output since action N",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const stream = args.stderr ? "stderr" : args.stdout ? "stdout" : "both";
			const result = await client.call<string>("session.output", {
				sessionId,
				stream,
				sinceAction: args["since-action"] ? Number.parseInt(args["since-action"], 10) : undefined,
			});
			process.stdout.write(`${formatOutput(result, stream, mode)}\n`);
		});
	},
});

// --- Attach ---

export const attachCommand = defineCommand({
	meta: { name: "attach", description: "Attach to a running process" },
	args: {
		language: {
			type: "string",
			description: languageDescription(),
			required: true,
		},
		pid: {
			type: "string",
			description: "Process ID",
		},
		port: {
			type: "string",
			description: "Debug server port",
		},
		host: {
			type: "string",
			description: "Debug server host",
		},
		break: {
			type: "string",
			description: "Set breakpoint(s), e.g. 'app.py:10'",
			alias: "b",
		},
		cwd: {
			type: "string",
			description: "Working directory for the debug target",
		},
		"source-lines": {
			type: "string",
			description: "Lines of source context above/below current line (default: 15)",
		},
		"stack-depth": {
			type: "string",
			description: "Max call stack frames to show (default: 5)",
		},
		"locals-depth": {
			type: "string",
			description: "Object expansion depth for locals (default: 1)",
		},
		"token-budget": {
			type: "string",
			description: "Approximate token budget for viewport output (default: 8000)",
		},
		"diff-mode": {
			type: "boolean",
			description: "Show only changed variables vs previous stop",
			default: false,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(
			args,
			async (client, _sessionId, mode) => {
				const breakpoints = args.break ? [parseBreakpointString(args.break)] : undefined;
				const viewportConfig = buildViewportConfig(args);
				const result = await client.call<LaunchResultPayload>("session.attach", {
					language: args.language,
					pid: args.pid ? Number.parseInt(args.pid, 10) : undefined,
					port: args.port ? Number.parseInt(args.port, 10) : undefined,
					host: args.host,
					cwd: args.cwd,
					viewportConfig,
					breakpoints: breakpoints?.map((fb) => ({
						file: fb.file,
						breakpoints: fb.breakpoints,
					})),
				});
				process.stdout.write(`${formatLaunch(result, mode)}\n`);
			},
			{ needsSession: false },
		);
	},
});

// --- Threads ---

export const threadsCommand = defineCommand({
	meta: { name: "threads", description: "List all threads in the debug session" },
	args: { ...globalArgs },
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const threads = await client.call<ThreadInfoPayload[]>("session.threads", { sessionId });
			process.stdout.write(`${formatThreads(threads, mode)}\n`);
		});
	},
});

// --- Debug Command Group ---

export const debugCommand = defineCommand({
	meta: { name: "debug", description: "Debug commands (launch, step, eval, ...)" },
	subCommands: {
		launch: launchCommand,
		attach: attachCommand,
		stop: stopCommand,
		status: statusCommand,
		continue: continueCommand,
		step: stepCommand,
		"run-to": runToCommand,
		break: breakCommand,
		breakpoints: breakpointsCommand,
		eval: evalCommand,
		vars: varsCommand,
		stack: stackCommand,
		source: sourceCommand,
		watch: watchCommand,
		unwatch: unwatchCommand,
		log: logCommand,
		output: outputCommand,
		threads: threadsCommand,
	},
});
