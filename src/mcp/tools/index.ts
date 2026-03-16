import { resolve as resolvePath } from "node:path";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { listAdapters } from "../../adapters/registry.js";
import { OutputStreamSchema, SessionLogFormatSchema, StepDirectionSchema, VariableScopeSchema } from "../../core/enums.js";
import { configToOptions, parseLaunchJson } from "../../core/launch-json.js";
import type { SessionManager } from "../../core/session-manager.js";
import { BreakpointSchema, FileBreakpointsSchema, mapViewportConfig } from "../../core/types.js";
import { listDetectors } from "../../frameworks/index.js";
import { errorResponse, textResponse, toolHandler } from "./utils.js";

const ViewportConfigSchema = z
	.object({
		source_context_lines: z.number().optional().describe("Lines of source shown above/below the current line. Default: 5"),
		stack_depth: z.number().optional().describe("Max call stack frames shown. Default: 10"),
		locals_max_depth: z.number().optional().describe("Object expansion depth for local variables. Default: 3"),
		locals_max_items: z.number().optional().describe("Max items shown per collection/object. Default: 20"),
		string_truncate_length: z.number().optional().describe("Max string length before truncation. Default: 200"),
		collection_preview_items: z.number().optional().describe("Items shown in inline collection previews. Default: 5"),
	})
	.optional()
	.describe("Override default viewport rendering parameters");

/**
 * Build a human-readable summary of supported languages and frameworks
 * from the live adapter/detector registries (single source of truth).
 */
function buildLanguageDescription(): string {
	const parts = listAdapters().map((a) => {
		const names = [a.id, ...(a.aliases ?? [])].join("/");
		const exts = a.fileExtensions.join(" ");
		return `${names} (${exts})`;
	});
	return `Supported: ${parts.join(", ")}. Omit to auto-detect from the file extension in the command.`;
}

function buildFrameworkDescription(): string {
	const ids = listDetectors().map((d) => d.id);
	return `Override framework auto-detection. Known frameworks: ${ids.join(", ")}. Use 'none' to disable auto-detection.`;
}

/**
 * Register all debug tools with the MCP server.
 * Each tool:
 * 1. Validates input with Zod schema
 * 2. Delegates to SessionManager
 * 3. Returns viewport text as MCP TextContent
 * 4. Handles errors with descriptive messages
 */
export function registerDebugTools(server: McpServer, sessionManager: SessionManager): void {
	const frameworkIds = listDetectors().map((d) => d.id);

	// Tool 1: debug_launch
	server.tool(
		"debug_launch",
		`Launch a debug target process. Sets initial breakpoints and returns a session handle. The viewport shows source, locals, and call stack at each stop. ` +
			`Automatically detects test/web frameworks (${frameworkIds.join(", ")}) to configure the debugger appropriately.`,
		{
			command: z
				.string()
				.optional()
				.describe(
					`Command to execute, e.g. 'python app.py' or '${frameworkIds[0]} tests/'. Required unless launch_config is provided. Test and web frameworks are auto-detected and configured for debugging.`,
				),
			language: z.string().optional().describe(buildLanguageDescription()),
			framework: z.string().optional().describe(buildFrameworkDescription()),
			breakpoints: z
				.array(FileBreakpointsSchema)
				.optional()
				.describe(
					"Initial breakpoints to set before execution begins. " +
						"Note: breakpoints on non-executable lines (comments, blank lines, decorators) " +
						"may be adjusted by the debugger to the nearest executable line.",
				),
			cwd: z.string().optional().describe("Working directory for the debug target"),
			env: z.record(z.string(), z.string()).optional().describe("Additional environment variables for the debug target"),
			viewport_config: ViewportConfigSchema,
			stop_on_entry: z.boolean().optional().describe("Pause on the first executable line. Default: false"),
			launch_config: z
				.object({
					path: z.string().optional().describe("Path to launch.json file (default: .vscode/launch.json)"),
					name: z.string().optional().describe("Configuration name to use from launch.json"),
				})
				.optional()
				.describe("Use a VS Code launch.json configuration instead of a command string"),
		},
		async ({ command, language, framework, breakpoints, cwd, env, viewport_config, stop_on_entry, launch_config }) => {
			try {
				let resolvedCommand = command;
				let resolvedLanguage = language;
				let resolvedCwd = cwd;
				let resolvedEnv = env;

				// Guard: URLs are not debuggable processes — redirect to browser tools
				if (resolvedCommand && /^https?:\/\//i.test(resolvedCommand.trim())) {
					return textResponse(
						"Error: debug_launch runs code debuggers — it cannot open URLs in a browser.\n\n" +
							"To record browser activity at a URL, use chrome_start instead:\n" +
							`  chrome_start(url: "${resolvedCommand}")\n\n` +
							"chrome_start launches Chrome, opens the URL, and records network, console, and user input events.",
					);
				}

				if (launch_config) {
					const configPath = launch_config.path ? resolvePath(launch_config.path) : resolvePath(process.cwd(), ".vscode/launch.json");
					const launchJson = await parseLaunchJson(configPath);
					if (!launchJson) {
						return errorResponse(new Error(`launch.json not found at: ${configPath}`));
					}

					let configEntry = launchJson.configurations[0];
					if (launch_config.name) {
						const found = launchJson.configurations.find((c) => c.name === launch_config.name);
						if (!found) {
							const available = launchJson.configurations.map((c) => `  "${c.name}"`).join("\n");
							return errorResponse(new Error(`Configuration "${launch_config.name}" not found. Available:\n${available}`));
						}
						configEntry = found;
					} else if (launchJson.configurations.length === 0) {
						return errorResponse(new Error("No configurations found in launch.json"));
					}

					if (!configEntry) {
						return errorResponse(new Error("No configuration found in launch.json"));
					}

					const converted = configToOptions(configEntry, process.cwd());
					if (converted.type === "attach") {
						const result = await sessionManager.attach({
							...converted.options,
							language: language ?? converted.options.language,
						});
						return textResponse(`Session: ${result.sessionId}\nStatus: ${result.status}\nAttached via launch.json config.`);
					}
					resolvedCommand = command ?? converted.options.command;
					resolvedLanguage = language ?? (converted.options.language as typeof language);
					resolvedCwd = cwd ?? converted.options.cwd;
					resolvedEnv = env ?? converted.options.env;
				}

				if (!resolvedCommand) {
					return errorResponse(new Error("Either 'command' or 'launch_config' must be provided"));
				}

				const result = await sessionManager.launch({
					command: resolvedCommand,
					language: resolvedLanguage,
					framework,
					breakpoints,
					cwd: resolvedCwd,
					env: resolvedEnv,
					viewportConfig: mapViewportConfig(viewport_config),
					stopOnEntry: stop_on_entry,
				});

				const parts: string[] = [`Session: ${result.sessionId}`];
				if (result.framework) {
					parts.push(`Framework: ${result.framework}`);
				}
				if (result.frameworkWarnings?.length) {
					for (const w of result.frameworkWarnings) {
						parts.push(`Warning: ${w}`);
					}
				}
				if (result.viewport) {
					parts.push("");
					parts.push(result.viewport);
				} else {
					parts.push(`Status: ${result.status}`);
				}
				return textResponse(parts.join("\n"));
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 2: debug_stop
	server.tool(
		"debug_stop",
		"Terminate a debug session and kill the target process. This terminates (kills) the process — it does not detach. If the session was created with debug_attach, the attached process is also killed. Cleans up all session resources.",
		{
			session_id: z.string().describe("The session to terminate"),
		},
		toolHandler(async ({ session_id }) => {
			const result = await sessionManager.stop(session_id);
			return `Session ${session_id} terminated.\nDuration: ${result.duration}ms\nActions: ${result.actionCount}`;
		}),
	);

	// Tool 3: debug_status
	server.tool(
		"debug_status",
		"Get the current status of a debug session. Returns viewport if stopped. Includes token stats, action count, and adapter capabilities.",
		{
			session_id: z.string().describe("The session to query"),
		},
		async ({ session_id }) => {
			try {
				const result = await sessionManager.getStatus(session_id);
				let text = result.viewport
					? `Status: ${result.status}\nActions: ${result.actionCount ?? 0}, Elapsed: ${result.elapsedMs ?? 0}ms, Viewport tokens: ${result.tokenStats?.viewportTokensConsumed ?? 0}\n\n${result.viewport}`
					: `Status: ${result.status}\nActions: ${result.actionCount ?? 0}, Elapsed: ${result.elapsedMs ?? 0}ms, Viewport tokens: ${result.tokenStats?.viewportTokensConsumed ?? 0}`;

				// Hint for running sessions (e.g. servers waiting for traffic)
				if (result.status === "running") {
					text += "\n\nThe program is running but hasn't hit a breakpoint yet. If debugging a server, send it a request (e.g. curl) via Bash, then call debug_continue to catch the breakpoint hit.";
				}

				// Append capabilities summary
				try {
					const caps = sessionManager.getCapabilities(session_id);
					const capLines = [
						`Conditional breakpoints: ${caps.supportsConditionalBreakpoints ? "yes" : "no"}`,
						`Hit count breakpoints: ${caps.supportsHitConditionalBreakpoints ? "yes" : "no"}`,
						`Logpoints: ${caps.supportsLogPoints ? "yes" : "no"}`,
						`Exception info: ${caps.supportsExceptionInfo ? "yes" : "no"}`,
					];
					if (caps.exceptionFilters.length > 0) {
						capLines.push(`Exception filters: ${caps.exceptionFilters.map((f) => f.filter).join(", ")}`);
					}
					text += `\n\nCapabilities:\n${capLines.map((l) => `  ${l}`).join("\n")}`;
				} catch {
					// Capabilities not available (e.g. not yet initialized)
				}

				return textResponse(text);
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 4: debug_continue
	server.tool(
		"debug_continue",
		"Resume execution until the next breakpoint or program end. Returns the viewport at the next stop point.",
		{
			session_id: z.string().describe("The active debug session"),
			timeout_ms: z.number().optional().describe("Max wait time for next stop in ms. Default: 30000"),
			thread_id: z.number().optional().describe("Thread ID to continue. Default: the thread that last stopped. Use debug_threads to list available threads."),
		},
		toolHandler(({ session_id, timeout_ms, thread_id }) => sessionManager.continue(session_id, timeout_ms, thread_id)),
	);

	// Tool 5: debug_step
	server.tool(
		"debug_step",
		"Step execution: 'over' steps over function calls, 'into' steps into them, 'out' steps out to the caller. Returns the viewport after stepping.",
		{
			session_id: z.string().describe("The active debug session"),
			direction: StepDirectionSchema.describe("Step granularity: 'over' skips function calls, 'into' enters them, 'out' runs to parent frame"),
			count: z.number().optional().describe("Number of steps to take. Default: 1. Useful for stepping through loops without setting breakpoints."),
			thread_id: z.number().optional().describe("Thread ID to step. Default: the thread that last stopped. Use debug_threads to list available threads."),
		},
		toolHandler(({ session_id, direction, count, thread_id }) => sessionManager.step(session_id, direction, count, thread_id)),
	);

	// Tool 6: debug_run_to
	server.tool(
		"debug_run_to",
		"Run execution to a specific file and line number, then pause. If the target line is never reached, a timeout error is returned (controlled by timeout_ms). Unlike setting a breakpoint, this is a one-time temporary stop.",
		{
			session_id: z.string().describe("The active debug session"),
			file: z.string().describe("Target file path"),
			line: z.number().describe("Target line number"),
			timeout_ms: z.number().optional().describe("Max wait time in ms. Default: 30000"),
		},
		toolHandler(({ session_id, file, line, timeout_ms }) => sessionManager.runTo(session_id, file, line, timeout_ms)),
	);

	// Tool 7: debug_set_breakpoints
	server.tool(
		"debug_set_breakpoints",
		"Set breakpoints in a source file. REPLACES all existing breakpoints in that file. " +
			"Supports conditions ('discount < 0'), hit counts ('>=100'), and logpoints ('discount={discount}'). " +
			"Logpoints log a message when hit instead of breaking. Not all debuggers support logpoints — " +
			"if unsupported, the breakpoint will be set as a regular breakpoint. " +
			"Note: breakpoints on non-executable lines may be adjusted by the debugger.",
		{
			session_id: z.string().describe("The active debug session"),
			file: z.string().describe("Source file path"),
			breakpoints: z
				.array(BreakpointSchema)
				.describe("Breakpoint definitions. REPLACES all existing breakpoints in this file. " + "To add a breakpoint without removing existing ones, include them all."),
		},
		async ({ session_id, file, breakpoints }) => {
			try {
				const verified = await sessionManager.setBreakpoints(session_id, file, breakpoints);
				const lines = verified
					.map((bp) => {
						const parts = [`Line ${bp.requestedLine}`];
						if (bp.verifiedLine !== null && bp.verifiedLine !== bp.requestedLine) {
							parts.push(`→ adjusted to line ${bp.verifiedLine}`);
						}
						parts.push(bp.verified ? "verified" : "UNVERIFIED");
						if (bp.message) parts.push(`(${bp.message})`);
						if (bp.conditionAccepted === false) parts.push("WARNING: condition may not be supported");
						return `  ${parts.join(" ")}`;
					})
					.join("\n");
				return textResponse(`Set ${breakpoints.length} breakpoints in ${file}:\n${lines}`);
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 8: debug_set_exception_breakpoints
	server.tool(
		"debug_set_exception_breakpoints",
		"Configure exception breakpoint filters. Controls which exceptions pause execution. " +
			"Python filters: 'raised' (all exceptions), 'uncaught' (unhandled only), 'userUnhandled'. " +
			"Node.js filters: 'all' (all exceptions), 'uncaught' (unhandled only). " +
			"Go/Delve: 'panic' (runtime panics). " +
			"Use debug_status after launch to see exact available filters for the current adapter.",
		{
			session_id: z.string().describe("The active debug session"),
			filters: z
				.array(z.string())
				.describe(
					"Exception filter IDs. Python: 'raised' (all exceptions), 'uncaught' (unhandled only), 'userUnhandled'. " +
						"Node.js: 'all' (all exceptions), 'uncaught' (unhandled only). " +
						"Go/Delve: 'panic' (runtime panics). " +
						"Use debug_status to see available filters for the current adapter.",
				),
		},
		toolHandler(async ({ session_id, filters }) => {
			await sessionManager.setExceptionBreakpoints(session_id, filters);
			const available = sessionManager.getExceptionBreakpointFilters(session_id);
			const filterList = available.map((f) => `  ${f.filter}: ${f.label}`).join("\n");
			return `Exception breakpoints set: ${filters.join(", ")}${filterList ? `\n\nAvailable filters:\n${filterList}` : ""}`;
		}),
	);

	// Tool 9: debug_list_breakpoints
	server.tool(
		"debug_list_breakpoints",
		"List all breakpoints currently set in the debug session.",
		{
			session_id: z.string().describe("The active debug session"),
		},
		async ({ session_id }) => {
			try {
				const bpMap = sessionManager.listBreakpoints(session_id);
				if (bpMap.size === 0) {
					return textResponse("No breakpoints set.");
				}
				const lines: string[] = [];
				for (const [file, bps] of bpMap) {
					lines.push(`${file}:`);
					for (const bp of bps) {
						const extras = [bp.condition ? `condition: ${bp.condition}` : "", bp.hitCondition ? `hitCondition: ${bp.hitCondition}` : "", bp.logMessage ? `logMessage: ${bp.logMessage}` : ""]
							.filter(Boolean)
							.join(", ");
						lines.push(`  Line ${bp.line}${extras ? ` (${extras})` : ""}`);
					}
				}
				return textResponse(lines.join("\n"));
			} catch (err) {
				return errorResponse(err);
			}
		},
	);

	// Tool 10: debug_evaluate
	server.tool(
		"debug_evaluate",
		"Evaluate an expression in the current debug context. Can access variables, call functions, and inspect nested objects.",
		{
			session_id: z.string().describe("The active debug session"),
			expression: z
				.string()
				.describe("Expression to evaluate in the debugee's context. " + "E.g., 'cart.items[0].__dict__', 'len(results)', 'discount < 0'. " + "Can call methods and access nested attributes."),
			frame_index: z.number().optional().describe("Stack frame context: 0 = current frame (default), 1 = caller, 2 = caller's caller, etc."),
			max_depth: z.number().optional().describe("Object expansion depth for the result. Default: 2"),
		},
		toolHandler(async ({ session_id, expression, frame_index, max_depth }) => {
			const result = await sessionManager.evaluate(session_id, expression, frame_index, max_depth);
			return `${expression} = ${result}`;
		}),
	);

	// Tool 11: debug_variables
	server.tool(
		"debug_variables",
		"Get variables from a specific scope and stack frame.",
		{
			session_id: z.string().describe("The active debug session"),
			scope: VariableScopeSchema.optional().describe("Variable scope to retrieve. Default: 'local'. Note: 'closure' is Node.js only — not available in Python or Go."),
			frame_index: z.number().optional().describe("Stack frame context (0 = current). Default: 0"),
			filter: z.string().optional().describe("Regex filter on variable names. E.g., '^user' to show only user-prefixed vars"),
			max_depth: z.number().optional().describe("Object expansion depth. Default: 1"),
		},
		toolHandler(async ({ session_id, scope, frame_index, filter, max_depth }) => {
			return (await sessionManager.getVariables(session_id, scope ?? "local", frame_index, filter, max_depth)) || "No variables found.";
		}),
	);

	// Tool 12: debug_stack_trace
	server.tool(
		"debug_stack_trace",
		"Get the current call stack showing the execution path to the current point.",
		{
			session_id: z.string().describe("The active debug session"),
			max_frames: z.number().optional().describe("Maximum frames to return. Default: 20"),
			include_source: z.boolean().optional().describe("Include source context around each frame. Default: false"),
		},
		toolHandler(({ session_id, max_frames, include_source }) => sessionManager.getStackTrace(session_id, max_frames, include_source)),
	);

	// Tool 13: debug_source
	server.tool(
		"debug_source",
		"Read source code from a file with line numbers. Use this instead of a plain file read when you need line numbers that match the debugger's view, or to read source-mapped or virtual files that don't exist at a literal path on disk.",
		{
			session_id: z.string().describe("The active debug session"),
			file: z.string().describe("Source file path"),
			start_line: z.number().optional().describe("Start of range. Default: 1"),
			end_line: z.number().optional().describe("End of range. Default: start_line + 40"),
		},
		toolHandler(({ session_id, file, start_line, end_line }) => sessionManager.getSource(session_id, file, start_line, end_line)),
	);

	// Tool 14: debug_watch — add or remove watch expressions
	server.tool(
		"debug_watch",
		"Manage watch expressions. Watched expressions are automatically evaluated and shown in every viewport snapshot.",
		{
			session_id: z.string().describe("The active debug session"),
			action: z.enum(["add", "remove"]).optional().describe("Whether to add or remove expressions. Default: 'add'"),
			expressions: z.array(z.string()).describe("Expressions to add or remove from the watch list. " + "E.g., ['len(cart.items)', 'user.tier', 'total > 0']"),
		},
		toolHandler(({ session_id, action, expressions }) => {
			const op = action ?? "add";
			const confirmed = op === "remove" ? sessionManager.removeWatchExpressions(session_id, expressions) : sessionManager.addWatchExpressions(session_id, expressions);
			return Promise.resolve(`Watch expressions (${confirmed.length} total):\n${confirmed.map((e) => `  ${e}`).join("\n")}`);
		}),
	);

	// Tool 15: debug_action_log — enriched session log with observations and token stats
	server.tool(
		"debug_action_log",
		"Get the investigation log for the current session. " +
			"Shows actions taken, key observations (unexpected values, variable changes), " +
			"and cumulative viewport token consumption. Older entries are automatically " +
			"compressed into summaries. Use this to reconstruct your reasoning chain " +
			"without re-reading old viewports.",
		{
			session_id: z.string().describe("The active debug session"),
			format: SessionLogFormatSchema.optional().describe("Level of detail. 'summary' compresses older entries. 'detailed' includes timestamps and full observations. Default: 'summary'"),
		},
		toolHandler(async ({ session_id, format }) => sessionManager.getSessionLog(session_id, format) || "No actions logged."),
	);

	// Tool 16: debug_output
	server.tool(
		"debug_output",
		"Get captured stdout/stderr output from the debug target.",
		{
			session_id: z.string().describe("The active debug session"),
			stream: OutputStreamSchema.optional().describe("Which output stream. Default: 'both'"),
			since_action: z.number().optional().describe("Only show output captured since action N. Default: 0 (all)"),
		},
		toolHandler(async ({ session_id, stream, since_action }) => sessionManager.getOutput(session_id, stream ?? "both", since_action ?? 0) || "No output captured."),
	);

	// Tool 17: debug_attach
	server.tool(
		"debug_attach",
		"Attach to an already-running process for debugging. " +
			"Use when the target is a long-running service (web server, daemon) or when you need to debug a process you didn't launch. " +
			"The process must already be listening for a debugger: python needs 'python -m debugpy --listen 5678 app.py', " +
			"node/typescript needs '--inspect' flag (node --inspect app.js), go attaches by PID via Delve.",
		{
			language: z.string().describe(`Language of the target process. Required for attach (no command to infer from). ${buildLanguageDescription()}`),
			pid: z.number().optional().describe("Process ID to attach to (Go/Delve)."),
			port: z.number().optional().describe("Debug server port. Python debugpy default: 5678. Node.js inspector default: 9229."),
			host: z.string().optional().describe("Debug server host. Default: '127.0.0.1'"),
			cwd: z.string().optional().describe("Working directory for source file resolution"),
			breakpoints: z.array(FileBreakpointsSchema).optional().describe("Breakpoints to set after attaching"),
			viewport_config: ViewportConfigSchema,
		},
		toolHandler(async ({ language, pid, port, host, cwd, breakpoints, viewport_config }) => {
			const result = await sessionManager.attach({
				language,
				pid,
				port,
				host,
				cwd,
				breakpoints,
				viewportConfig: mapViewportConfig(viewport_config),
			});
			return `Session: ${result.sessionId}\nStatus: ${result.status}\nAttached to ${language} process.`;
		}),
	);

	// Tool 18: debug_threads
	server.tool(
		"debug_threads",
		"List all threads in the debug session. " +
			"Useful for multi-threaded programs (Go goroutines, Python threads). " +
			"Shows thread IDs and names. Use thread_id on step/continue/evaluate " +
			"to operate on a specific thread.",
		{
			session_id: z.string().describe("The active debug session"),
		},
		toolHandler(async ({ session_id }) => {
			const threads = await sessionManager.getThreads(session_id);
			const lines = threads.map((t) => `  ${t.stopped ? "→" : " "} Thread ${t.id}: ${t.name}${t.stopped ? " (stopped)" : " (running)"}`);
			return `Threads (${threads.length}):\n${lines.join("\n")}`;
		}),
	);
}
