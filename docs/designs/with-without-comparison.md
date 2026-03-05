# With vs Without Debugging Tools — Comparison Runs

Extension to the [agent test harness](agent-test-harness.md). Every scenario runs in three modes per agent, producing results that show the value of each agent-lens interface.

## Motivation

The harness today answers "can an agent fix this bug using agent-lens?" but not "would it have fixed it anyway?" Without a baseline, a 100% pass rate could mean agent-lens is essential or that the bugs are too easy. Multi-mode runs answer the real question: **which bugs does runtime debugging actually help with, and does the interface matter?**

This also directly supports [showcase narrative #6](showcase-narratives.md) (tool usage patterns) and the "difficulty progression" story — showing that easy bugs don't need debugging tools but hard ones do.

## Scope

Starting with Claude Code as the first agent under test. All three modes (mcp, cli, baseline) are proven here first. Other agent drivers (Codex, etc.) are stubbed in the harness and will be enabled as we expand to cross-agent comparison.

## Design

### Run Modes

Each scenario x agent combination produces three runs:

| Mode | MCP Config | Skill File | Label |
|------|-----------|------------|-------|
| **mcp** | agent-lens MCP server configured | yes | `mcp` |
| **cli** | none | yes (teaches CLI usage) | `cli` |
| **baseline** | none | none | `baseline` |

- **mcp** — agent uses `debug_*` MCP tools (Claude Code's primary path)
- **cli** — agent calls `agent-lens` CLI via bash (Codex's primary path, also works with Claude Code)
- **baseline** — no agent-lens at all; agent relies on code reading, test output, and shell

The prompt is identical across all three modes. The skill file (which teaches debugging strategy) is only injected in `mcp` and `cli` modes.

### Prompt Rules

The prompt must work for all modes. This means:
- **No mention of agent-lens or debugging tools** (already a rule in [scenario guidelines](scenario-guidelines.md))
- The agent should be free to use whatever approach it wants — reading code, running tests, adding print statements, or using debug tools if available
- The prompt describes the symptom and points to the relevant files, nothing more

This is already the convention. No prompt changes needed.

### Runner Changes

The test matrix becomes `scenarios x agents x modes`:

```typescript
type RunMode = "mcp" | "cli" | "baseline";
const modes = ["baseline", "cli", "mcp"] as const;

describe.each(agents)("Agent: $name", (agent) => {
  describe.each(scenarios)("Scenario: $name", (scenario) => {
    describe.each(modes)("Mode: %s", (mode) => {
      it("agent fixes the bug", async () => {
        const result = await runScenario(agent, scenario, suiteDir, mode);
        expect(result.validation.passed).toBe(true);
      });
    });
  });
});
```

What each mode controls:
- **`mcp`**: MCP config generated and passed via `--mcp-config`; skill file injected
- **`cli`**: no MCP config; skill file injected (teaches agent to use bash commands)
- **`baseline`**: no MCP config; no skill file; agent has only its built-in tools

### Driver Interface

```typescript
interface AgentRunOptions {
  // ... existing fields ...

  /** Run mode — controls what agent-lens interfaces are available */
  mode: "mcp" | "cli" | "baseline";
}
```

Each driver interprets the mode:

**Claude Code** (`claude-code.ts`):
- `mcp`: adds `--mcp-config <path>` flag; skill file via `--append-system-prompt`
- `cli`: no MCP config; skill file via `--append-system-prompt`
- `baseline`: neither

**Codex** (`codex.ts`, stubbed):
- `mcp`/`cli`: skill content prepended to prompt (MCP support to be wired up when Codex testing begins)
- `baseline`: no skill content

### Trace Directory Structure

Traces include the mode in the path:

```
.traces/
  2026-03-04T14-30-00Z/
    claude-code/
      python-discount-bug/
        mcp/
          result.json
          agent-stdout.txt
          workspace-diff.patch
        cli/
          result.json
          ...
        baseline/
          result.json
          ...
    report.json
```

### Result File Extension

Each `result.json` includes the mode:

```json
{
  "scenario": "python-discount-bug",
  "agent": "claude-code",
  "mode": "mcp",
  "passed": true,
  "durationMs": 45200,
  "metrics": { "numTurns": 8, "toolCalls": { "debug_launch": 1, "..." : "..." } }
}
```

## Report Changes

### Summary Table

The agent summary table groups by agent + mode:

```markdown
| Agent | Mode | Scenarios | Passed | Pass Rate | Avg Duration |
|-------|------|-----------|--------|-----------|--------------|
| claude-code | mcp | 5 | 5 | 100% | 42s |
| claude-code | cli | 5 | 4 | 80% | 55s |
| claude-code | baseline | 5 | 3 | 60% | 78s |
| codex | cli | 5 | 4 | 80% | 62s |
| codex | baseline | 5 | 2 | 40% | 95s |
```

### Per-Scenario Comparison

Each scenario shows one column per mode present in the data:

```markdown
### python-closure-capture
*python — Late-binding closure over loop variable*

| Agent | baseline | cli | mcp |
|-------|----------|-----|-----|
| claude-code | FAIL (120s, 22t) | PASS (55s, 12t) | PASS (45s, 8t) |
| codex | FAIL (95s, 18t) | PASS (68s, 14t) | — |
```

### Aggregate Analysis

```markdown
## Debugging Tools Impact

| Level | baseline | cli | mcp |
|-------|----------|-----|-----|
| 1-2 (read the code) | 100% | 100% | 100% |
| 3 (inspect state) | 50% | 80% | 100% |
| 4 (multi-component) | 20% | 60% | 80% |
| 5 (subtle/adversarial) | 0% | 40% | 60% |
```

The narrative: **MCP gives the richest debugging interface, CLI is a solid middle ground, and baseline shows what agents can do without any debugging tools.**

## Run Ordering

Modes run in order: baseline, cli, mcp. Rationale:
- Avoids information leakage (workspaces are independent, but ordering makes it unambiguous)
- If you abort mid-suite, you get baseline data for everything — the most useful missing data point

## Filtering

```bash
# Run only MCP mode (useful during development)
MODE=mcp bun run test:agent

# Run only baseline
MODE=baseline bun run test:agent

# Run MCP + baseline (skip CLI)
MODE=mcp,baseline bun run test:agent

# All three (default)
bun run test:agent
```

## Cost Implications

This triples the number of runs vs single-mode. Mitigations:
- **Filtering:** `MODE=mcp` during development, full comparison for publishable results
- **Scenario selection:** `SCENARIO=python-closure-capture MODE=mcp,baseline` to compare one scenario
- **Budget scaling:** Baseline runs may use more tokens (agents without tools tend to flail more). Consider giving baseline runs 1.5x the budget ceiling to avoid timeouts masking capability differences

## What This Does NOT Do

- **A/B test statistical rigor.** LLM runs are non-deterministic. A single run per mode is directional, not statistically significant. For publishable claims, run the suite multiple times and aggregate across `index.json`.

- **Test different tool subsets.** We don't test "agent-lens minus eval" or "only breakpoints, no stepping." The comparison is at the interface level: MCP vs CLI vs nothing.

- **Control for agent strategy.** An agent without debug tools might still succeed by adding print statements, reading tracebacks carefully, or making educated guesses. That's fine — it's what we're measuring. The question is outcome, not method.

## Open Questions

1. **Timeout asymmetry.** Baseline runs might need more time because the agent takes more turns without tools. Should baseline get a longer timeout, or should we keep it equal to make the comparison fair? Leaning toward equal timeouts — if the agent can't solve it in time without tools, that's a valid data point.

2. **Should we capture the agent's strategy?** Beyond pass/fail, it would be interesting to categorize *how* the agent approached the bug in each mode (read code, add prints, use debugger, guess-and-check). This is hard to automate but could be done manually for showcase scenarios.
