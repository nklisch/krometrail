# With vs Without Debugging Tools — Comparison Runs

Extension to the [agent test harness](agent-test-harness.md). Every scenario runs twice per agent: once with agent-lens MCP tools available, once without. This produces paired results that show the value of runtime debugging for each scenario.

## Motivation

The harness today answers "can an agent fix this bug using agent-lens?" but not "would it have fixed it anyway?" Without a baseline, a 100% pass rate could mean agent-lens is essential or that the bugs are too easy. Paired runs answer the real question: **which bugs does runtime debugging actually help with?**

This also directly supports [showcase narrative #6](showcase-narratives.md) (tool usage patterns) and the "difficulty progression" story — showing that easy bugs don't need debugging tools but hard ones do.

## Design

### Run Modes

Each scenario x agent combination produces two runs:

| Mode | MCP Config | Label |
|------|-----------|-------|
| **with-tools** | agent-lens MCP server configured | `tools` |
| **without-tools** | No MCP config (or empty) | `baseline` |

The prompt is identical in both modes. The only difference is whether the agent has access to `debug_*` tools.

### Prompt Adjustments

The prompt must work for both modes. This means:
- **No mention of agent-lens or debugging tools** (already a rule in [scenario guidelines](scenario-guidelines.md))
- The agent should be free to use whatever approach it wants — reading code, running tests, adding print statements, or using debug tools if available
- The prompt describes the symptom and points to the relevant files, nothing more

This is already the convention. No prompt changes needed.

### Runner Changes

The test matrix becomes `scenarios x agents x modes`:

```typescript
const modes = ["tools", "baseline"] as const;
type RunMode = typeof modes[number];

describe.each(agents)("Agent: $name", (agent) => {
  describe.each(scenarios)("Scenario: $name", (scenario) => {
    describe.each(modes)("Mode: %s", (mode) => {
      it("agent fixes the bug", async () => {
        const result = await runScenario(agent, scenario, { mode });
        // Still the same assertion — did the hidden test pass?
        expect(result.validation.passed).toBe(true);
      });
    });
  });
});
```

When `mode === "baseline"`:
- No MCP config file is generated
- The `--mcp-config` flag is omitted from the agent spawn args
- The `--allowedTools` flag is omitted (or set to exclude `mcp__agent-lens__*`)
- Everything else is identical: same workspace, same prompt, same timeout, same budget

### Driver Interface Extension

```typescript
interface AgentRunOptions {
  // ... existing fields ...

  /** Run mode — "tools" includes agent-lens, "baseline" does not */
  mode: "tools" | "baseline";
}
```

Each driver decides how to handle the mode. For Claude Code:

```typescript
async run(options) {
  const args = [
    "-p", options.prompt,
    "--max-turns", "50",
    "--permission-mode", "bypassPermissions",
  ];

  if (options.mode === "tools") {
    args.push("--mcp-config", options.mcpConfigPath);
    args.push("--allowedTools", "mcp__agent-lens__*");
  }

  // ... rest unchanged ...
}
```

### Trace Directory Structure

Traces include the mode in the path:

```
.traces/
  2026-03-04T14-30-00Z/
    claude-code/
      python-discount-bug/
        tools/
          result.json
          agent-stdout.txt
          workspace-diff.patch
        baseline/
          result.json
          agent-stdout.txt
          workspace-diff.patch
    report.json
```

### Result File Extension

Each `result.json` includes the mode:

```json
{
  "scenario": "python-discount-bug",
  "agent": "claude-code",
  "mode": "tools",
  "passed": true,
  "duration_ms": 45200,
  "num_turns": 8,
  "tool_calls": { "debug_launch": 1, "debug_set_breakpoints": 2, "..." : "..." }
}
```

Baseline results have `"mode": "baseline"` and `"tool_calls": {}` (no debug tools available).

## Report Changes

### Summary Table

The summary gains a column showing the delta:

```markdown
## Summary

| Agent | Mode | Passed | Failed | Pass Rate |
|-------|------|--------|--------|-----------|
| claude-code | tools | 5 | 0 | 100% |
| claude-code | baseline | 3 | 2 | 60% |
| codex | tools | 4 | 1 | 80% |
| codex | baseline | 2 | 3 | 40% |
```

### Per-Scenario Comparison

Each scenario shows paired results side by side:

```markdown
### python-closure-capture (Level 3)

| Agent | With Tools | Without Tools | Delta |
|-------|-----------|---------------|-------|
| claude-code | PASS (45s, 8 turns) | FAIL (120s, 22 turns) | +1 |
| codex | PASS (68s, 14 turns) | FAIL (95s, 18 turns) | +1 |

**Tools advantage:** Both agents needed runtime state inspection to find the
late-binding closure bug. Without tools, both attempted fixes based on code
reading alone and patched the wrong location.
```

### Aggregate Analysis

```markdown
## Debugging Tools Impact

| Level | With Tools | Without Tools | Lift |
|-------|-----------|---------------|------|
| 1-2 (read the code) | 100% | 100% | +0% |
| 3 (inspect state) | 100% | 50% | +50% |
| 4 (multi-component) | 80% | 20% | +60% |
| 5 (subtle/adversarial) | 60% | 0% | +60% |

**Key finding:** Debugging tools provide no advantage for bugs visible in the
source code (Levels 1-2) but become essential as bugs depend on runtime state
(Level 3+).
```

This is the core narrative for agent-lens: **runtime debugging transforms agent capability on non-trivial bugs.**

## Run Ordering

Run baseline first, then tools. Rationale:
- Avoids information leakage — the agent's tools run is not influenced by a prior baseline run (workspaces are independent anyway, but this ordering makes it unambiguous)
- If you're short on time/budget and abort mid-suite, you get baseline data for everything and tools data for some, which is more useful than the reverse

## Filtering

```bash
# Run only tools mode (skip baseline — useful during development)
MODE=tools bun run test:agent

# Run only baseline
MODE=baseline bun run test:agent

# Both (default)
bun run test:agent
```

## Cost Implications

This doubles the number of runs. Mitigations:
- **Filtering:** `MODE=tools` during development, full comparison for publishable results
- **Scenario selection:** `SCENARIO=python-closure-capture` to run one scenario in both modes
- **Level gating:** Skip baseline for Level 1-2 scenarios since we expect no difference — focus comparison budget on Level 3+
- **Budget scaling:** Baseline runs may use more tokens (agents without tools tend to flail more). Consider giving baseline runs 1.5x the budget ceiling to avoid timeouts masking capability differences

```typescript
const budgetMultiplier = mode === "baseline" ? 1.5 : 1.0;
const effectiveBudget = scenario.maxBudgetUsd * budgetMultiplier;
```

## What This Does NOT Do

- **A/B test statistical rigor.** LLM runs are non-deterministic. A single paired run per scenario is directional, not statistically significant. For publishable claims, run each pair N times and report pass rates. The harness supports this (run the suite multiple times, aggregate across `index.json`), but doesn't enforce it.

- **Test different tool subsets.** We don't test "agent-lens minus eval" or "only breakpoints, no stepping." That's interesting research but out of scope — the comparison is all-or-nothing.

- **Control for agent strategy.** An agent without debug tools might still succeed by adding print statements, reading tracebacks carefully, or making educated guesses. That's fine — it's what we're measuring. The question is outcome, not method.

## Open Questions

1. **Should baseline runs get a different system prompt?** Some agents have skill files or system prompts that reference debugging tools. If those tools aren't available, does the agent waste turns trying to call them? Probably not (agents handle missing tools gracefully), but worth verifying.

2. **Timeout asymmetry.** Baseline runs might need more time because the agent takes more turns without tools. Should baseline get a longer timeout, or should we keep it equal to make the comparison fair? Leaning toward equal timeouts — if the agent can't solve it in time without tools, that's a valid data point.

3. **Should we capture the agent's strategy?** Beyond pass/fail, it would be interesting to categorize *how* the agent approached the bug in each mode (read code, add prints, use debugger, guess-and-check). This is hard to automate but could be done manually for showcase scenarios.
