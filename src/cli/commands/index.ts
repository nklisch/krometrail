import { defineCommand } from "citty";
import { DaemonClient, ensureDaemon } from "../../daemon/client.js";
import type { BreakpointsListPayload, BreakpointsResultPayload, LaunchResultPayload, StatusResultPayload, StopResultPayload, ViewportPayload } from "../../daemon/protocol.js";
import { getDaemonSocketPath } from "../../daemon/protocol.js";
import {
	formatBreakpointsList,
	formatBreakpointsSet,
	formatError,
	formatEvaluate,
	formatLaunch,
	formatStackTrace,
	formatStatus,
	formatStop,
	formatVariables,
	formatViewport,
	resolveOutputMode,
} from "../format.js";
import { parseBreakpointString, parseLocation, parseSourceRange } from "../parsers.js";

// --- Shared Args ---

const globalArgs = {
	json: {
		type: "boolean" as const,
		description: "Output as JSON instead of viewport text",
		default: false,
	},
	quiet: {
		type: "boolean" as const,
		description: "Viewport only, no banners or hints",
		default: false,
	},
	session: {
		type: "string" as const,
		description: "Target a specific session (required when multiple active)",
		alias: "s",
	},
};

type OutputMode = "text" | "json" | "quiet";

/**
 * Helper: create a DaemonClient, ensuring daemon is running first.
 */
async function getClient(): Promise<DaemonClient> {
	const socketPath = getDaemonSocketPath();
	await ensureDaemon(socketPath);
	return new DaemonClient({ socketPath, requestTimeoutMs: 60_000 });
}

/**
 * Helper: resolve session ID. If --session is provided, use it.
 * Otherwise, call daemon.sessions to auto-resolve if exactly one session exists.
 */
async function resolveSessionId(client: DaemonClient, explicitSession?: string): Promise<string> {
	if (explicitSession) {
		return explicitSession;
	}

	const sessions = await client.call<Array<{ id: string; status: string; language: string; actionCount: number }>>("daemon.sessions");

	if (sessions.length === 0) {
		throw new Error('No active sessions. Launch one with: agent-lens launch "<command>"');
	}

	if (sessions.length === 1) {
		return sessions[0].id;
	}

	const sessionList = sessions.map((s) => `  ${s.id} (${s.language}, ${s.status})`).join("\n");
	throw new Error(`Multiple active sessions. Use --session to specify one:\n${sessionList}`);
}

/**
 * Helper: wrap a CLI command with standard mode resolution, client lifecycle,
 * session resolution, error handling, and cleanup.
 *
 * For commands that don't need a session ID upfront (e.g. launch), pass
 * `{ needsSession: false }` and the handler receives `null` as sessionId.
 */
async function runCommand(
	args: { json?: boolean; quiet?: boolean; session?: string },
	handler: (client: DaemonClient, sessionId: string, mode: OutputMode) => Promise<void>,
	opts?: { needsSession: false },
): Promise<void>;
async function runCommand(args: { json?: boolean; quiet?: boolean; session?: string }, handler: (client: DaemonClient, sessionId: string, mode: OutputMode) => Promise<void>): Promise<void>;
async function runCommand(
	args: { json?: boolean; quiet?: boolean; session?: string },
	handler: (client: DaemonClient, sessionId: string, mode: OutputMode) => Promise<void>,
	opts?: { needsSession?: false },
): Promise<void> {
	const mode = resolveOutputMode(args) as OutputMode;
	const client = await getClient();
	try {
		const sessionId = opts?.needsSession === false ? "" : await resolveSessionId(client, args.session);
		await handler(client, sessionId, mode);
	} catch (err) {
		process.stderr.write(`${formatError(err as Error, mode)}\n`);
		process.exit(1);
	} finally {
		client.dispose();
	}
}

// --- Session Lifecycle ---

export const launchCommand = defineCommand({
	meta: { name: "launch", description: "Launch a debug session" },
	args: {
		command: {
			type: "positional",
			description: "Command to debug, e.g. 'python app.py'",
			required: true,
		},
		break: {
			type: "string",
			description: "Set breakpoint(s), e.g. 'order.py:147' or 'order.py:147 when discount < 0'",
			alias: "b",
		},
		language: {
			type: "string",
			description: "Override language detection",
		},
		"stop-on-entry": {
			type: "boolean",
			description: "Pause on first executable line",
			default: false,
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(
			args,
			async (client, _sessionId, mode) => {
				const breakpoints = args.break ? [parseBreakpointString(args.break)] : undefined;
				const result = await client.call<LaunchResultPayload>("session.launch", {
					command: args.command,
					language: args.language,
					breakpoints: breakpoints?.map((fb) => ({
						file: fb.file,
						breakpoints: fb.breakpoints,
					})),
					stopOnEntry: args["stop-on-entry"],
				});
				process.stdout.write(`${formatLaunch(result, mode)}\n`);
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
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const result = await client.call<ViewportPayload>("session.continue", {
				sessionId,
				timeoutMs: args.timeout ? Number.parseInt(args.timeout, 10) : undefined,
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
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			const direction = args.direction as "over" | "into" | "out";
			if (!["over", "into", "out"].includes(direction)) {
				throw new Error(`Invalid step direction: ${direction}. Must be 'over', 'into', or 'out'.`);
			}
			const result = await client.call<ViewportPayload>("session.step", {
				sessionId,
				direction,
				count: args.count ? Number.parseInt(args.count, 10) : undefined,
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
			type: "positional",
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
				process.stdout.write(mode === "json" ? `${JSON.stringify({ filters: [args.exceptions] }, null, 2)}\n` : `Exception breakpoints set: ${args.exceptions}\n`);
			} else if (args.clear) {
				await client.call("session.setBreakpoints", {
					sessionId,
					file: args.clear,
					breakpoints: [],
				});
				process.stdout.write(mode === "json" ? `${JSON.stringify({ cleared: args.clear }, null, 2)}\n` : `Breakpoints cleared: ${args.clear}\n`);
			} else if (args.breakpoint) {
				const parsed = parseBreakpointString(args.breakpoint);
				const result = await client.call<BreakpointsResultPayload>("session.setBreakpoints", {
					sessionId,
					file: parsed.file,
					breakpoints: parsed.breakpoints,
				});
				process.stdout.write(`${formatBreakpointsSet(parsed.file, result, mode)}\n`);
			} else {
				throw new Error("Usage: agent-lens break <file:line> | --exceptions <filter> | --clear <file>");
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
			if (mode === "json") {
				process.stdout.write(`${JSON.stringify({ file, source: result }, null, 2)}\n`);
			} else {
				process.stdout.write(`${result}\n`);
			}
		});
	},
});

// --- Session Intelligence ---

function collectExpressions(firstExpr: string, args: Record<string, unknown>): string[] {
	const extraArgs = args._ as string[] | undefined;
	return [firstExpr, ...(extraArgs ?? [])];
}

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
			if (mode === "json") {
				process.stdout.write(`${JSON.stringify({ watchExpressions: result }, null, 2)}\n`);
			} else {
				process.stdout.write(`Watch expressions (${result.length} total):\n`);
				for (const expr of result) process.stdout.write(`  ${expr}\n`);
			}
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
			if (mode === "json") {
				process.stdout.write(`${JSON.stringify({ watchExpressions: result }, null, 2)}\n`);
			} else {
				process.stdout.write(`Watch expressions (${result.length} total):\n`);
				for (const expr of result) process.stdout.write(`  ${expr}\n`);
			}
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
			if (mode === "json") {
				process.stdout.write(`${JSON.stringify({ log: result }, null, 2)}\n`);
			} else {
				process.stdout.write(`${result}\n`);
			}
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
			if (mode === "json") {
				process.stdout.write(`${JSON.stringify({ output: result, stream }, null, 2)}\n`);
			} else {
				process.stdout.write(result || "No output captured.\n");
			}
		});
	},
});

export const skillCommand = defineCommand({
	meta: { name: "skill", description: "Print the agent skill file to stdout" },
	args: {},
	async run() {
		const skillPath = new URL("../../../skill.md", import.meta.url);
		const content = await Bun.file(skillPath).text();
		process.stdout.write(content);
	},
});

// --- Doctor (see doctor.ts) ---

export { doctorCommand } from "./doctor.js";
