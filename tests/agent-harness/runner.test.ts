/**
 * Agent test harness runner.
 *
 * Runs real agent binaries against buggy code scenarios and validates the fix
 * using a hidden test the agent never saw.
 *
 * Each scenario runs in three modes:
 *   - baseline: no agent-lens at all — agent relies on code reading, tests, shell
 *   - cli:      agent-lens CLI + skill file available — agent uses bash commands
 *   - mcp:      agent-lens MCP server configured — agent can use debug_* tools
 *
 * Starting with Claude Code as the first agent under test. Other drivers
 * (Codex, etc.) are stubbed and will be enabled for cross-agent comparison.
 *
 * This test suite is NOT run in CI. Run it manually:
 *
 *   bun run test:agent                                                  # all agents × all scenarios × all modes
 *   AGENT=claude-code bun run test:agent                                # one agent
 *   SCENARIO=python-discount-bug bun run test:agent                     # one scenario
 *   MODE=mcp bun run test:agent                                          # one mode only
 *   AGENT=claude-code SCENARIO=python-discount-bug MODE=mcp bun run test:agent
 *   TRACE_DIR=./results bun run test:agent                              # custom output dir
 *
 * Results are saved as structured traces (default: tests/agent-harness/.traces/).
 * Trace path: <suiteDir>/<agent>/<scenario>/<mode>/result.json
 * Generate a report:
 *
 *   bun run test:agent:report
 */

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { discoverAgents } from "./lib/agents.js";
import type { AgentDriver, RunMode, Scenario } from "./lib/config.js";
import { runScenario } from "./lib/harness.js";
import { discoverScenarios } from "./lib/scenarios.js";
import { initSuiteDir, writeSuiteMeta } from "./lib/trace.js";

const scenarios: Scenario[] = await discoverScenarios();
const agents: AgentDriver[] = await discoverAgents();

// Mode selection — baseline runs first, then cli, then mcp.
// Configurable via MODE env var (comma-separated); defaults to all three modes.
const ALL_MODES = ["baseline", "cli", "mcp"] as const satisfies RunMode[];
const modeFilter = process.env.MODE;
const modes: RunMode[] = modeFilter ? (ALL_MODES.filter((m) => modeFilter.split(",").includes(m)) as RunMode[]) : [...ALL_MODES];

if (modes.length === 0) {
	throw new Error(`MODE="${modeFilter}" has no valid modes. Use "mcp", "cli", "baseline" (comma-separated).`);
}

let suiteDir: string;

beforeAll(async () => {
	suiteDir = await initSuiteDir();
	await writeSuiteMeta(suiteDir, {
		timestamp: new Date().toISOString(),
		scenarios: scenarios.map((s) => s.name),
		agents: agents.map((a) => a.name),
		modes,
	});
	console.log(`[agent-harness] Modes: ${modes.join(", ")}  Traces → ${suiteDir}`);
});

afterAll(() => {
	console.log(`[agent-harness] Run complete. Generate report: bun run test:agent:report`);
});

describe.each(agents)("Agent: $name", (agent) => {
	describe.each(scenarios)("Scenario: $name", (scenario) => {
		describe.each(modes)("Mode: %s", (mode) => {
			it(
				"fixes the bug (hidden test passes)",
				async () => {
					const result = await runScenario(agent, scenario, suiteDir, mode);

					const failMsg = [
						`Agent:    ${agent.name}`,
						`Scenario: ${scenario.name} (${scenario.language})`,
						`Mode:     ${mode}`,
						`Duration: ${(result.durationMs / 1000).toFixed(1)}s`,
						`Turns:    ${result.metrics.numTurns ?? "n/a"}`,
						`Tokens:   ${result.metrics.tokens ? `${result.metrics.tokens.total} (in: ${result.metrics.tokens.input + result.metrics.tokens.cacheRead + result.metrics.tokens.cacheWrite}, out: ${result.metrics.tokens.output})` : "n/a"}`,
						`Exit code: ${result.agentExitCode ?? "killed"}`,
						`Timed out: ${result.timedOut}`,
						`Visible test passed: ${result.visibleTestAfter}`,
						`Files changed: ${result.filesChanged.join(", ") || "none"}`,
						"",
						"--- Session log ---",
						result.sessionLog.join("\n") || "(empty)",
						"",
						"--- Validation output ---",
						result.validation.stdout,
						result.validation.stderr,
					].join("\n");

					expect(result.passed, failMsg).toBe(true);
				},
				scenario.timeoutSeconds * 1000 + 60_000,
			);
		});
	});
});
