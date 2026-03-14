# Using Krometrail with OpenAI Codex

Krometrail gives Codex runtime debugging via MCP or the CLI — the same tools, two interfaces.

## Setup: MCP (Recommended)

Add to your Codex config at `~/.codex/config.toml` (global) or `.codex/config.toml` (per-project):

```toml
[mcp_servers.krometrail]
command = "npx"
args = ["krometrail", "mcp"]
```

Or add via the CLI:

```bash
codex mcp add krometrail -- npx krometrail mcp
```

Codex discovers the `debug_*`, `chrome_*`, and `session_*` tools automatically.

## Setup: CLI with Skill

For CLI-based usage, install the krometrail skill:

```bash
npx skills add nklisch/krometrail --skill krometrail-debug krometrail-chrome
```

## Example Workflow

Ask Codex:

> The `calculate_discount` function returns wrong values for gold tier customers. Debug it.

Codex will:

1. `debug_launch` (or `krometrail debug launch`) with `python3 -m pytest tests/ -k test_gold` and a breakpoint at `discount.py:42`
2. Continue to the breakpoint
3. Evaluate `tier` and `tier_multipliers['gold']`
4. Step into the function
5. Identify the bug and explain it

## Tips

- **MCP is zero-config** — Codex discovers tools automatically from the server
- **CLI path** is transparent and scriptable — Codex can run multiple commands in parallel via bash
- **Session persistence** — sessions are managed by a background daemon across multiple turns
- **Multiple sessions** — use `--session <id>` to target a specific session when multiple are active

## Verifying Setup

```bash
krometrail doctor
```

This checks which language adapters are installed.
