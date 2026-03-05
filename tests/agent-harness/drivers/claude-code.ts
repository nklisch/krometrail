import { registerDriver } from "../lib/agents.js";
import type { AgentDriver, AgentMetrics, AgentRunOptions, AgentRunResult, TokenUsage, ToolEvent } from "../lib/config.js";
import { spawnCapture } from "../lib/spawn.js";

/**
 * Format a stream-json event as a compact log line.
 */
function formatEvent(data: Record<string, unknown>): string | null {
	switch (data.type) {
		case "system":
			if (data.subtype === "init") return `[init] model=${data.model} tools=${(data.tools as string[])?.length ?? 0}`;
			return null;
		case "assistant": {
			const msg = data.message as { content?: Array<{ type: string; text?: string; name?: string }> } | undefined;
			for (const block of msg?.content ?? []) {
				if (block.type === "tool_use" && block.name) return `[tool] ${block.name}`;
				if (block.type === "text" && block.text) {
					const preview = block.text.slice(0, 120).replace(/\n/g, " ");
					return `[text] ${preview}${block.text.length > 120 ? "…" : ""}`;
				}
			}
			return null;
		}
		case "result":
			return `[done] turns=${data.num_turns} tokens_out=${(data.usage as Record<string, number>)?.output_tokens ?? "?"}`;
		default:
			return null;
	}
}

/**
 * Parse the full stream-json stdout into metrics, result summary, and tool timeline.
 */
function parseClaudeStream(stdout: string): {
	metrics: Partial<AgentMetrics>;
	resultSummary: string | null;
	toolTimeline: ToolEvent[];
} {
	const lines = stdout.split("\n").filter((l) => l.trim().startsWith("{"));
	let model: string | null = null;
	let resultSummary: string | null = null;
	const toolCalls: Record<string, number> = {};

	// Pending tool calls by ID (waiting for their result)
	const pending = new Map<string, { tool: string; input: unknown }>();
	const toolTimeline: ToolEvent[] = [];

	for (const line of lines) {
		try {
			const data = JSON.parse(line) as Record<string, unknown>;

			if (data.type === "system" && data.subtype === "init" && typeof data.model === "string") {
				model = data.model;
			}

			// Tool call from assistant
			if (data.type === "assistant" && data.message) {
				const msg = data.message as { content?: Array<{ type: string; name?: string; id?: string; input?: unknown }> };
				for (const block of msg.content ?? []) {
					if (block.type === "tool_use" && block.name) {
						toolCalls[block.name] = (toolCalls[block.name] ?? 0) + 1;
						if (block.id) {
							pending.set(block.id, { tool: block.name, input: block.input });
						}
					}
				}
			}

			// Tool result from user message
			if (data.type === "user" && data.message) {
				const msg = data.message as { content?: Array<{ type: string; tool_use_id?: string; content?: unknown }> };
				for (const block of msg.content ?? []) {
					if (block.type === "tool_result" && block.tool_use_id) {
						const call = pending.get(block.tool_use_id);
						if (call) {
							pending.delete(block.tool_use_id);
							// Extract text from content
							let output: string | null = null;
							if (typeof block.content === "string") {
								output = block.content;
							} else if (Array.isArray(block.content)) {
								const texts = (block.content as Array<{ type: string; text?: string }>)
									.filter((c) => c.type === "text" && c.text)
									.map((c) => c.text!);
								if (texts.length > 0) output = texts.join("\n");
							}

							toolTimeline.push({
								tool: call.tool,
								input: call.input,
								output,
								toolUseId: block.tool_use_id,
							});
						}
					}
				}
			}

			if (data.type === "result") {
				const usage = data.usage as Record<string, number> | undefined;

				let tokens: TokenUsage | null = null;
				if (usage) {
					const input = usage.input_tokens ?? 0;
					const cacheRead = usage.cache_read_input_tokens ?? 0;
					const cacheWrite = usage.cache_creation_input_tokens ?? 0;
					const output = usage.output_tokens ?? 0;
					tokens = { input, cacheRead, cacheWrite, output, total: input + cacheRead + cacheWrite + output };
				}

				if (typeof data.result === "string") {
					resultSummary = data.result;
				}

				return {
					metrics: { numTurns: typeof data.num_turns === "number" ? data.num_turns : null, tokens, model, toolCalls },
					resultSummary,
					toolTimeline,
				};
			}
		} catch {
			// Skip malformed lines
		}
	}

	return { metrics: { model, toolCalls }, resultSummary: null, toolTimeline };
}

const claudeCode: AgentDriver = {
	name: "claude-code",

	async available() {
		try {
			const result = await spawnCapture("claude", ["--version"]);
			return result.exitCode === 0;
		} catch {
			return false;
		}
	},

	async version() {
		try {
			const result = await spawnCapture("claude", ["--version"]);
			return result.stdout.trim().split("\n")[0] ?? "unknown";
		} catch {
			return "unknown";
		}
	},

	async run(options: AgentRunOptions): Promise<AgentRunResult> {
		const start = Date.now();
		const args: string[] = ["-p", options.prompt, "--dangerously-skip-permissions", "--output-format", "stream-json", "--verbose"];

		if (options.mode === "tools") {
			args.push("--mcp-config", options.mcpConfigPath);
		}

		if (options.skillContent) {
			args.push("--append-system-prompt", options.skillContent);
		}

		if (options.maxBudgetUsd !== undefined) {
			args.push("--max-budget-usd", String(options.maxBudgetUsd));
		}

		const sessionLog: string[] = [];

		const result = await spawnCapture("claude", args, {
			cwd: options.workDir,
			env: options.env,
			timeoutMs: options.timeoutMs,
			cleanEnv: true,
			onStdoutLine(line) {
				try {
					const data = JSON.parse(line) as Record<string, unknown>;
					const formatted = formatEvent(data);
					if (formatted) {
						sessionLog.push(formatted);
						console.error(`  claude-code │ ${formatted}`);
					}
				} catch {
					// not JSON, ignore
				}
			},
		});

		// Parse the tool timeline from collected stdout
		const { toolTimeline } = parseClaudeStream(result.stdout);

		return {
			exitCode: result.exitCode,
			stdout: result.stdout,
			stderr: result.stderr,
			timedOut: result.timedOut,
			durationMs: Date.now() - start,
			sessionLog,
			toolTimeline,
		};
	},

	parseMetrics(result: AgentRunResult): AgentMetrics {
		const { metrics: parsed } = parseClaudeStream(result.stdout);
		return {
			numTurns: parsed.numTurns ?? null,
			tokens: parsed.tokens ?? null,
			model: parsed.model ?? null,
			agentVersion: null,
			toolCalls: parsed.toolCalls ?? {},
		};
	},
};

registerDriver(() => claudeCode);
