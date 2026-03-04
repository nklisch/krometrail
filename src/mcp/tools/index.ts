import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";

/**
 * Registers all debug tools with the MCP server.
 * Each tool validates its input with Zod and delegates to the session manager.
 */
export function registerTools(server: McpServer): void {
	server.tool(
		"debug_launch",
		"Launch a debug target process. Sets initial breakpoints and returns a session handle. The viewport shows source, locals, and call stack at each stop.",
		{
			command: z.string().describe("Command to execute, e.g. 'python app.py'"),
			language: z
				.enum(["python", "javascript", "typescript", "go", "rust", "java", "cpp"])
				.optional()
				.describe("Override language detection"),
			cwd: z.string().optional().describe("Working directory for the debug target"),
			stop_on_entry: z.boolean().optional().describe("Pause on first executable line"),
		},
		async ({ command, language, cwd, stop_on_entry }) => {
			// TODO: delegate to session manager
			void command;
			void language;
			void cwd;
			void stop_on_entry;
			return { content: [{ type: "text" as const, text: "Not implemented" }] };
		},
	);

	server.tool(
		"debug_stop",
		"Terminate a debug session and clean up all resources.",
		{
			session_id: z.string().describe("The session to terminate"),
		},
		async ({ session_id }) => {
			void session_id;
			return { content: [{ type: "text" as const, text: "Not implemented" }] };
		},
	);

	// TODO: register remaining tools:
	// debug_status, debug_continue, debug_step, debug_run_to,
	// debug_set_breakpoints, debug_set_exception_breakpoints, debug_list_breakpoints,
	// debug_evaluate, debug_variables, debug_stack_trace, debug_source,
	// debug_watch, debug_session_log, debug_output
}
