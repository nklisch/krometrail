/**
 * Codex agent driver.
 *
 * NOTE: Stubbed — not yet actively tested. Starting with Claude Code first.
 * This driver will be expanded when we begin cross-agent testing.
 *
 * Codex uses the CLI (bash commands) rather than MCP tools — the agent-lens
 * skill file is included in the system prompt to tell Codex how to use the CLI.
 *
 * Flags used:
 *   --approval-mode full-auto   — skip approval prompts for all actions
 *   --quiet                     — suppress interactive UI
 */

import { registerDriver } from "../lib/agents.js";
import type { AgentDriver, AgentMetrics, AgentRunOptions, AgentRunResult } from "../lib/config.js";
import { spawnCapture } from "../lib/spawn.js";

const codex: AgentDriver = {
	name: "codex",

	async available() {
		try {
			const result = await spawnCapture("codex", ["--version"]);
			return result.exitCode === 0;
		} catch {
			return false;
		}
	},

	async version() {
		try {
			const result = await spawnCapture("codex", ["--version"]);
			return result.stdout.trim().split("\n")[0] ?? "unknown";
		} catch {
			return "unknown";
		}
	},

	async run(options: AgentRunOptions): Promise<AgentRunResult> {
		const start = Date.now();
		const fullPrompt = options.skillContent ? `${options.skillContent}\n\n---\n\n${options.prompt}` : options.prompt;

		const args: string[] = ["--approval-mode", "full-auto", "--quiet", fullPrompt];

		const result = await spawnCapture("codex", args, {
			cwd: options.workDir,
			env: options.env,
			timeoutMs: options.timeoutMs,
			cleanEnv: true,
		});

		return {
			exitCode: result.exitCode,
			stdout: result.stdout,
			stderr: result.stderr,
			timedOut: result.timedOut,
			durationMs: Date.now() - start,
		};
	},

	parseMetrics(result: AgentRunResult): AgentMetrics {
		const toolCallMatches = result.stdout.matchAll(/agent-lens\s+([\w-]+)/g);
		const toolCalls: Record<string, number> = {};
		for (const m of toolCallMatches) {
			const tool = `agent-lens-${m[1]}`;
			toolCalls[tool] = (toolCalls[tool] ?? 0) + 1;
		}

		return {
			numTurns: null,
			tokens: null,
			model: null,
			agentVersion: null,
			toolCalls,
		};
	},
};

registerDriver(() => codex);
