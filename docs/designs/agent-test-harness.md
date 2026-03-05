# Design: Agent Test Harness

## Problem

Agent Lens has three test tiers today: unit, integration, and e2e. All three validate the tool chain from the inside — they call MCP tools directly via a test client and assert on viewport output. None of them answer the real question: **can an actual agent use agent-lens to autonomously debug and fix a bug?**

debugger-mcp proved this kind of testing is viable (5 languages x 2 agents, 10/10 pass rate). But their tests only verify the agent can step through the SBCTED sequence — they don't verify the agent can actually *fix* a bug using the debugger. That's the gap.

We need a harness that:
1. Gives an agent buggy code and a failing test
2. Makes agent-lens available as an MCP server
3. Lets the agent autonomously investigate and fix the bug
4. Validates the fix against a hidden test the agent never saw

---

## Design Principles

**Scenarios are data, not code.** A scenario is a directory of files plus a config. Adding a new test case means creating a folder, not writing test infrastructure.

**Agent-agnostic.** The harness runs against any agent binary that supports MCP or CLI. Agent-specific details (how to spawn, how to pass MCP config, how to set permissions) are isolated in thin driver modules. Starting with Claude Code as the first agent under test; other drivers (Codex, etc.) will follow once scenarios and reporting are proven.

**Cheap to run, expensive to skip.** These tests cost real money (LLM API calls). They must be opt-in, never in default CI. But skipping them entirely means shipping blind — so the harness should make it trivial to run a quick smoke test (one scenario, one agent) during development.

**The hidden test is the only oracle.** We don't assert on what the agent did, what tools it called, or how many turns it took. We only ask: does the hidden test pass? This keeps the harness robust against changes in agent behavior, model versions, and tool interfaces.

**Observable but not prescriptive.** The harness captures a full trace (agent stdout/stderr, tool calls if available, timing, cost) for debugging and analysis. But none of this is part of the pass/fail gate.

---

## Scenario Structure

Each scenario is a self-contained directory:

```
tests/agent-harness/scenarios/
  python-discount-bug/
    scenario.toml         # Scenario metadata + config
    prompt.md             # The prompt given to the agent
    src/                  # Buggy source code (copied into workspace)
      discount.py
      test_discount.py    # Visible failing test (agent sees this)
    hidden/               # Hidden validation (agent never sees this)
      test_validation.py  # The real oracle test
```

### scenario.toml

```toml
[scenario]
name = "python-discount-bug"
language = "python"
description = "Gold tier discount multiplier is 1.0 instead of 0.1"
timeout_seconds = 120
max_budget_usd = 0.50

[setup]
# Commands to run in the workspace before the agent starts
commands = [
  "python3 -m venv .venv",
  "source .venv/bin/activate && pip install -q pytest",
]

[visible_test]
# The test the agent can see and run — it should fail before the fix
command = "python3 -m pytest test_discount.py -x"

[validation]
# The hidden test — copied in after the agent finishes, must pass
command = "python3 -m pytest test_validation.py -x"
# Glob pattern for files to copy from hidden/ into workspace before validation
files = "hidden/*"
```

### prompt.md

The prompt is the exact text given to the agent. It should describe the problem without giving away the answer, and mention that agent-lens is available:

```markdown
The discount calculation in `discount.py` is producing incorrect totals
for gold-tier customers. The test in `test_discount.py` demonstrates the
failure.

Debug the issue using the agent-lens debugging tools available to you.
Fix the bug so that `test_discount.py` passes.
```

### Visible test (src/test_discount.py)

The agent can see and run this test. It fails before the fix and passes after. This gives the agent a concrete signal to work toward:

```python
from discount import process_order

def test_gold_discount():
    # Gold tier should get 10% discount, not 100%
    total = process_order("gold", [100.0])
    assert total == 100.0, f"Expected 100.0, got {total}"
```

### Hidden test (hidden/test_validation.py)

The agent never sees this. It tests the fix more thoroughly — edge cases, other tiers, boundary conditions:

```python
from discount import calculate_discount, process_order

def test_gold_discount_rate():
    assert calculate_discount("gold", 100.0) == 10.0

def test_all_tiers_reasonable():
    for tier in ["bronze", "silver", "gold", "platinum"]:
        rate = calculate_discount(tier, 100.0)
        assert 0 <= rate <= 25, f"{tier} discount {rate} is unreasonable"

def test_gold_order_total():
    total = process_order("gold", [49.99, 49.99, 49.99])
    assert 120 < total < 160, f"Gold order total {total} is wrong"
```

---

## Agent Drivers

An agent driver is a module that knows how to spawn a specific agent with an MCP config. Each driver exports a single interface:

```typescript
interface AgentDriver {
	/** Human-readable name for logs */
	name: string;

	/** Check if the agent binary is available */
	available(): Promise<boolean>;

	/** Spawn the agent with a prompt and MCP config */
	run(options: AgentRunOptions): Promise<AgentRunResult>;
}

interface AgentRunOptions {
	/** Working directory (the temp workspace) */
	workDir: string;
	/** Path to MCP config JSON file */
	mcpConfigPath: string;
	/** The prompt text */
	prompt: string;
	/** Timeout in ms */
	timeoutMs: number;
	/** Max budget in USD (if the agent supports it) */
	maxBudgetUsd?: number;
	/** Environment variables to pass through */
	env?: Record<string, string>;
}

interface AgentRunResult {
	exitCode: number | null;
	stdout: string;
	stderr: string;
	timedOut: boolean;
	durationMs: number;
}
```

### Claude Code Driver

```typescript
import { spawn } from "node:child_process";

export const claudeCode: AgentDriver = {
	name: "claude-code",

	async available() {
		try {
			const proc = Bun.spawn(["claude", "--version"], { stdout: "pipe" });
			await proc.exited;
			return proc.exitCode === 0;
		} catch {
			return false;
		}
	},

	async run(options) {
		const start = Date.now();
		const args = [
			"-p", options.prompt,
			"--mcp-config", options.mcpConfigPath,
			"--allowedTools", "mcp__agent-lens__*",
			"--max-turns", "50",
			"--permission-mode", "bypassPermissions",
		];

		if (options.maxBudgetUsd) {
			args.push("--max-budget-usd", String(options.maxBudgetUsd));
		}

		const proc = Bun.spawn(["claude", ...args], {
			cwd: options.workDir,
			stdout: "pipe",
			stderr: "pipe",
			env: { ...process.env, ...options.env },
		});

		const timeout = setTimeout(() => proc.kill(), options.timeoutMs);
		const exitCode = await proc.exited;
		clearTimeout(timeout);

		return {
			exitCode,
			stdout: await new Response(proc.stdout).text(),
			stderr: await new Response(proc.stderr).text(),
			timedOut: exitCode === null,
			durationMs: Date.now() - start,
		};
	},
};
```

### Codex Driver (stubbed)

A Codex driver is included for future cross-agent testing. See the source at `drivers/codex.ts`.

New agents are added by creating a new driver module. The harness discovers drivers from a registry — no framework code changes needed.

---

## Harness Core

### MCP Config Generation

The harness generates an MCP config JSON file that points at agent-lens, configured to run in the scenario's workspace:

```typescript
function generateMcpConfig(workDir: string): McpConfig {
	return {
		mcpServers: {
			"agent-lens": {
				command: "bun",
				args: [
					"run",
					resolve(__dirname, "../../src/mcp/index.ts"),
				],
				cwd: workDir,
			},
		},
	};
}
```

This means agent-lens runs from source (not a compiled binary), making it easy to test changes during development.

### Workspace Setup

For each test run, the harness:

1. Creates a temp directory
2. Copies scenario `src/` files into it
3. Runs setup commands from `scenario.toml`
4. Generates the MCP config file
5. Writes it to the temp directory

```typescript
async function prepareWorkspace(scenario: Scenario): Promise<Workspace> {
	const workDir = await mkdtemp(join(tmpdir(), "agent-lens-test-"));

	// Copy source files
	await cp(scenario.srcDir, workDir, { recursive: true });

	// Run setup commands
	for (const cmd of scenario.setup.commands) {
		const proc = Bun.spawn(["bash", "-c", cmd], {
			cwd: workDir,
			stdout: "pipe",
			stderr: "pipe",
		});
		const exitCode = await proc.exited;
		if (exitCode !== 0) {
			const stderr = await new Response(proc.stderr).text();
			throw new Error(`Setup command failed: ${cmd}\n${stderr}`);
		}
	}

	// Generate MCP config
	const mcpConfigPath = join(workDir, ".mcp-config.json");
	await writeFile(mcpConfigPath, JSON.stringify(generateMcpConfig(workDir)));

	return { workDir, mcpConfigPath };
}
```

### Validation

After the agent finishes, the harness:

1. Copies hidden test files into the workspace
2. Runs the validation command
3. Reports pass/fail

```typescript
async function validate(workspace: Workspace, scenario: Scenario): Promise<ValidationResult> {
	// Copy hidden files into workspace
	await cp(scenario.hiddenDir, workspace.workDir, { recursive: true });

	// Run validation command
	const proc = Bun.spawn(["bash", "-c", scenario.validation.command], {
		cwd: workspace.workDir,
		stdout: "pipe",
		stderr: "pipe",
	});
	const exitCode = await proc.exited;

	return {
		passed: exitCode === 0,
		stdout: await new Response(proc.stdout).text(),
		stderr: await new Response(proc.stderr).text(),
	};
}
```

### Trace Capture

Every run produces a structured trace under `.traces/` (gitignored). See the **Metrics & Reporting** section for the full trace structure, result format, and report generation.

---

## Test Runner

The runner is a vitest test file that iterates scenarios x agents:

**File:** `tests/agent-harness/runner.test.ts`

```typescript
import { describe, it, expect } from "vitest";
import { discoverScenarios } from "./lib/scenarios";
import { discoverAgents } from "./lib/agents";
import { runScenario } from "./lib/harness";

const scenarios = await discoverScenarios();
const agents = await discoverAgents();

describe.each(agents)("Agent: $name", (agent) => {
	describe.each(scenarios)("Scenario: $name", (scenario) => {
		it(
			"agent fixes the bug",
			async () => {
				const result = await runScenario(agent, scenario);

				// The only assertion: did the hidden test pass?
				expect(result.validation.passed, [
					`Agent: ${agent.name}`,
					`Scenario: ${scenario.name}`,
					`Duration: ${result.durationMs}ms`,
					`Exit code: ${result.agentResult.exitCode}`,
					result.validation.stderr,
				].join("\n")).toBe(true);
			},
			scenario.timeoutSeconds * 1000 + 30_000, // scenario timeout + buffer
		);
	});
});
```

### Running

```bash
# All scenarios, all available agents
bun run test:agent

# Specific agent
AGENT=claude-code bun run test:agent

# Specific scenario
SCENARIO=python-discount-bug bun run test:agent

# Both filters
AGENT=claude-code SCENARIO=python-discount-bug bun run test:agent
```

Package.json:
```json
{
  "scripts": {
    "test:agent": "vitest run tests/agent-harness/"
  }
}
```

Vitest config override for agent tests:
```typescript
// tests/agent-harness/vitest.config.ts
export default defineConfig({
	test: {
		testTimeout: 300_000,  // 5 min default — agents are slow
		hookTimeout: 60_000,
		include: ["tests/agent-harness/**/*.test.ts"],
	},
});
```

---

## Initial Scenarios

Start with three scenarios covering different languages and bug types:

### 1. python-discount-bug (warm-up)

The canonical example from agent-lens docs. Gold tier multiplier is `1.0` instead of `0.1`. Simple, deterministic, fast to debug.

- **Language:** Python
- **Bug type:** Wrong constant value
- **Expected agent strategy:** Set breakpoint in `calculate_discount`, inspect `rate`, see it's 1.0 for gold, trace to `tier_multipliers` dict
- **Timeout:** 120s
- **Budget:** $0.50

### 2. python-off-by-one

A loop processes items but skips the last one due to `range(len(items) - 1)`. The visible test shows one item missing from output.

- **Language:** Python
- **Bug type:** Off-by-one error
- **Expected agent strategy:** Set breakpoint in loop, step through iterations, notice the last item is never processed
- **Timeout:** 120s
- **Budget:** $0.50

### 3. node-async-race

An async function writes to a file but doesn't await the write before reading it back. Intermittent failure — the read sometimes returns stale data.

- **Language:** Node.js
- **Bug type:** Missing await
- **Expected agent strategy:** Set breakpoints around the write/read, inspect timing, notice the write hasn't completed
- **Timeout:** 180s
- **Budget:** $0.75

---

## File Layout

```
tests/agent-harness/
  runner.test.ts                    # Vitest entry point
  vitest.config.ts                  # Extended timeouts for agent tests
  lib/
    harness.ts                      # Core: prepareWorkspace, runAgent, validate
    scenarios.ts                    # Discover and parse scenario directories
    agents.ts                       # Agent driver registry and discovery
    config.ts                       # Types for scenario.toml, agent options
    trace.ts                        # Trace capture and storage
  drivers/
    claude-code.ts                  # Claude Code agent driver
    codex.ts                        # Codex agent driver (stubbed)
  scenarios/
    python-discount-bug/
      scenario.toml
      prompt.md
      src/
        discount.py
        test_discount.py
      hidden/
        test_validation.py
    python-off-by-one/
      scenario.toml
      prompt.md
      src/
        process_items.py
        test_items.py
      hidden/
        test_validation.py
    node-async-race/
      scenario.toml
      prompt.md
      src/
        file-cache.js
        test-cache.js
      hidden/
        test_validation.js
  .traces/                          # Gitignored — run traces for debugging
```

---

## Open Questions

1. **TOML vs TypeScript config?** TOML keeps scenarios declarative and language-agnostic. TypeScript config would allow computed values and type checking. Leaning TOML for simplicity — scenarios are data, not programs.

2. **Agent permission model.** Claude Code has `--permission-mode bypassPermissions` for non-interactive use. Other agents may handle this differently. The driver abstraction handles this, but we should document the security implications (agent runs with full file access in a temp dir).

3. **Flakiness budget.** LLM-based tests are inherently non-deterministic. Should we retry on failure? Run N times and require M passes? Initial stance: no retries, accept flakiness, track pass rates over time in traces. A scenario that fails >30% of the time is either too hard or poorly designed.

4. **Cost tracking.** Claude Code reports cost via `--output-format stream-json`. Should the harness parse this and enforce budget limits, or just rely on `--max-budget-usd`? Leaning toward the latter — let the agent enforce its own budget.

5. **Scenario difficulty levels.** Should we tag scenarios by difficulty (smoke / standard / hard) so developers can run a quick smoke test without burning through the full suite?

6. **Multi-file bugs.** Some real bugs span multiple files. The scenario structure supports this (entire `src/` directory is copied), but do we need any special handling for the prompt to orient the agent?

---

## Metrics & Reporting

The harness captures rich metrics per run and produces publishable reports. This is not CI — it's a tool for generating results you can share, blog about, or use in documentation.

### Metrics Collected Per Run

Every scenario x agent run captures:

| Metric | Source | Description |
|--------|--------|-------------|
| `passed` | Hidden test exit code | Did the agent fix the bug? |
| `duration_ms` | Wall clock | Total time from agent spawn to exit |
| `agent_exit_code` | Process | How the agent exited (0, non-zero, null=killed) |
| `timed_out` | Harness | Did the agent hit the timeout? |
| `cost_usd` | Agent stdout (parsed) | Cost of the run (agent-reported, if available) |
| `num_turns` | Agent stdout (parsed) | Number of agent turns (if available) |
| `tool_calls` | Agent stdout (parsed) | List of MCP tools called, with counts |
| `tokens_input` | Agent stdout (parsed) | Input tokens consumed (if available) |
| `tokens_output` | Agent stdout (parsed) | Output tokens consumed (if available) |
| `model` | Agent stdout (parsed) | Model used (if available) |
| `agent_version` | `--version` output | Agent binary version |
| `agent_lens_version` | package.json | Agent-lens version under test |
| `timestamp` | System clock | ISO 8601 run timestamp |
| `visible_test_before` | Pre-run test | Did the visible test fail before the agent ran? (sanity check) |
| `visible_test_after` | Post-run test | Does the visible test pass after the agent ran? |
| `validation_stdout` | Hidden test | Raw output from the hidden test |
| `files_changed` | git diff in workspace | Which files the agent modified |
| `diff` | git diff in workspace | The actual patch the agent produced |

### Run Result File

Each run produces a `result.json`:

```json
{
  "scenario": "python-discount-bug",
  "agent": "claude-code",
  "model": "claude-sonnet-4-6",
  "agent_version": "1.2.3",
  "agent_lens_version": "0.1.0",
  "timestamp": "2026-03-04T14:30:00Z",
  "passed": true,
  "duration_ms": 45200,
  "cost_usd": 0.12,
  "num_turns": 8,
  "tokens_input": 32000,
  "tokens_output": 4500,
  "timed_out": false,
  "tool_calls": {
    "debug_launch": 1,
    "debug_set_breakpoints": 2,
    "debug_continue": 3,
    "debug_evaluate": 2,
    "debug_variables": 1,
    "debug_stop": 1
  },
  "files_changed": ["discount.py"],
  "visible_test_before": false,
  "visible_test_after": true,
  "diff": "--- a/discount.py\n+++ b/discount.py\n@@ -4,1 +4,1 @@\n-    \"gold\": 1.0,\n+    \"gold\": 0.1,"
}
```

### Report Generation

The harness includes a report command that aggregates results across runs:

```bash
# Generate a report from all traces
bun run test:agent:report

# Generate from a specific run directory
bun run test:agent:report --dir .traces/2026-03-04
```

This produces a **markdown report** suitable for publishing:

```markdown
# Agent Lens — Agent Test Report

**Date:** 2026-03-04
**Agent Lens version:** 0.1.0

## Summary

| Agent | Scenarios | Passed | Failed | Pass Rate | Avg Duration | Avg Cost |
|-------|-----------|--------|--------|-----------|--------------|----------|
| claude-code (sonnet-4-6) | 3 | 3 | 0 | 100% | 42s | $0.15 |

## Results by Scenario

### python-discount-bug

| Agent | Result | Duration | Cost | Turns | Debug Tools Used |
|-------|--------|----------|------|-------|------------------|
| claude-code | PASS | 45s | $0.12 | 8 | launch, breakpoints, continue(3), evaluate(2), stop |

### python-off-by-one

| Agent | Result | Duration | Cost | Turns | Debug Tools Used |
|-------|--------|----------|------|-------|------------------|
| claude-code | PASS | 38s | $0.10 | 6 | launch, breakpoints, step(4), variables, stop |

### node-async-race

| Agent | Result | Duration | Cost | Turns | Debug Tools Used |
|-------|--------|----------|------|-------|------------------|
| claude-code | PASS | 62s | $0.22 | 11 | launch, breakpoints, continue(4), evaluate(3), variables(2), stop |

## Tool Usage Patterns

| Tool | Total Calls | Avg per Scenario |
|------|-------------|-----------------|
| debug_launch | 3 | 1.0 |
| debug_continue | 10 | 3.3 |
| debug_set_breakpoints | 3 | 1.0 |
| debug_evaluate | 5 | 1.7 |
| debug_variables | 3 | 1.0 |
| debug_step | 4 | 1.3 |
| debug_stop | 3 | 1.0 |
```

The report command also outputs the same data as JSON for programmatic consumption:

```bash
bun run test:agent:report --format json > report.json
```

### Trace Directory Structure

```
tests/agent-harness/.traces/
  2026-03-04T14-30-00Z/                    # One directory per suite run
    meta.json                               # Suite-level metadata
    claude-code/
      python-discount-bug/
        result.json                         # Structured metrics
        agent-stdout.txt                    # Raw agent output
        agent-stderr.txt                    # Agent errors
        workspace-diff.patch                # Git diff of agent's changes
        validation-stdout.txt               # Hidden test output
      python-off-by-one/
        ...
    report.md                               # Generated report
    report.json                             # Machine-readable report
```

### Workspace as Git Repo

To capture diffs cleanly, the harness initializes each workspace as a git repo:

```typescript
// In prepareWorkspace:
await exec("git init", { cwd: workDir });
await exec("git add -A", { cwd: workDir });
await exec('git commit -m "initial"', { cwd: workDir });
```

After the agent finishes:

```typescript
// Capture what the agent changed
const diff = await exec("git diff HEAD", { cwd: workDir });
const filesChanged = await exec("git diff --name-only HEAD", { cwd: workDir });
```

This gives us a clean patch showing exactly what the agent modified, perfect for including in reports.

---

## Non-Goals

- **Benchmarking agents against each other.** The reports show per-agent results side by side, but the purpose is to validate agent-lens, not to rank agents. We don't draw conclusions about which agent is "better" — model versions, prompts, and configurations all affect outcomes.

- **Testing without agent-lens.** ~~We don't run scenarios without agent-lens to establish a baseline.~~ *Update: baseline runs are now implemented — see [with-without-comparison.md](with-without-comparison.md).*

- **Covering every language.** Start with Python and Node.js. Add languages as agent-lens gains adapter support. The harness itself is language-agnostic — new languages are just new scenario directories.

- **Complex real-world codebases.** Scenarios should be small, focused, and deterministic. A 500-line file with a subtle concurrency bug is a benchmark, not a test.
