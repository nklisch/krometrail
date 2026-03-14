# Using Krometrail with Claude Code

Krometrail gives Claude Code runtime debugging — it can set breakpoints, inspect variables, and trace execution to find bugs.

## Setup: MCP Server (Recommended)

Add via the CLI or edit the project `.mcp.json` directly:

```bash
claude mcp add --transport stdio --scope project krometrail -- npx krometrail mcp
```

Or add to `.mcp.json` in your project root (shared with your team):

```json
{
  "mcpServers": {
    "krometrail": {
      "command": "npx",
      "args": ["krometrail", "mcp"]
    }
  }
}
```

Or with a compiled binary:

```json
{
  "mcpServers": {
    "krometrail": {
      "command": "/path/to/krometrail",
      "args": ["mcp"]
    }
  }
}
```

Claude discovers the `debug_*` tools automatically — no CLAUDE.md changes needed.

## Setup: CLI with Skill

The CLI provides the same debugging capabilities as the MCP tools, accessed through bash commands. Some setups prefer CLI tools for their transparency and scriptability.

Install the krometrail skill:

```bash
npx skills add nklisch/krometrail --skill krometrail
```

## Verification

1. Start Claude Code
2. Ask: "What debug tools do you have available?"
3. Claude should list the `debug_*` tools (MCP) or describe the CLI commands (skill)

## Example Workflow

Ask Claude Code:

> The `test_gold_discount` test is failing with an assertion error. Use the debugger to find the root cause.

Claude Code will:

1. Identify the test file and set a breakpoint at the assertion
2. Launch the test under the debugger: `debug_launch` with `pytest tests/test_discount.py::test_gold_discount`
3. Continue to the breakpoint: `debug_continue`
4. Inspect variables: `debug_evaluate` on `tier_multipliers`, `discount`, etc.
5. Trace back to find the incorrect value
6. Report the root cause with the specific line and variable that is wrong

## Tips

- **MCP path is zero-config** — Claude discovers tools automatically from the server
- **CLI path** exposes the same tools as bash commands — transparent, scriptable, and preferred by some agent setups
- **Let Claude choose breakpoints** — it knows the code better after reading it
- **Conditional breakpoints** are powerful for loops: `krometrail break app.py:25 when discount < 0`
- **The viewport is compact** (~400 tokens per stop) so Claude can take many debug steps without exhausting context
- **Framework auto-detection** works for pytest, jest, go test, Django, Flask, etc.

## Checking Installation

```bash
krometrail doctor
```

This shows which language adapters are installed and their versions.
