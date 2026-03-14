---
title: Prior Art
description: Comparison with other MCP-DAP projects and what Krometrail does differently.
---

# Prior Art

Several projects bridge MCP to DAP. All solve the plumbing problem. None address agent ergonomics — which is Krometrail's primary focus.

## Projects

| Project | Language | Debuggers | Key contribution |
|---------|----------|-----------|-----------------|
| [mcp-debugger](https://github.com/debugmcp/mcp-debugger) | TypeScript | Python, JS, Rust, Go | Dynamic adapter loading, clean architecture |
| [mcp-dap-server](https://github.com/go-delve/mcp-dap-server) | Go | Go/Delve | Auto context return, capability-gated tools |
| [AIDB](https://github.com/ai-debugger-inc/aidb) | Python | Python, JS, Java | Framework auto-detection, launch.json reuse |
| [debugger-mcp](https://github.com/Govinda-Fichtner/debugger-mcp) | Rust | 5 languages | Real agent integration tests (Claude Code, Codex) |
| [dap-mcp](https://github.com/KashunCheng/dap_mcp) | Python | debugpy, lldb | Config-driven adapter selection |

## What Each Got Right

**mcp-dap-server's automatic context return.** After every `step` or `continue`, it calls `getFullContext` — stack, scopes, and variables in one response. No separate tool calls required. Krometrail follows this pattern: every execution control tool returns the viewport automatically.

**mcp-debugger's `get_local_variables`.** Collapses the standard DAP dance (stack → scope → variables) into one call. Also filters out noisy `__builtins__`/`__proto__` entries. Krometrail's viewport does this automatically on every stop.

**AIDB's framework auto-detection.** Auto-identifies pytest, jest, django, spring so agents don't need to configure debug adapters for common cases. Krometrail implements the same pattern.

**debugger-mcp's agent integration tests.** Tests run against real Claude Code and Codex agents (not mocked clients). 10 test matrices across 5 languages. Proves the approach works end-to-end. Krometrail's agent harness follows this model.

**mcp-dap-server's capability-gated tools.** Tools only appear when the underlying debugger supports them (`restart`, `set-variable`, `disassemble`). Agents never see tools that won't work.

## What No One Solved

**The token problem.** Every existing project returns raw DAP state with no token awareness. mcp-dap-server's `getFullContext` dumps *all* scopes and *all* variables with no truncation — a moderately complex program produces thousands of tokens per stop.

**Session intelligence.** No project maintains an investigation log, implements viewport diffing, or progressively compresses output over long sessions.

**A CLI interface.** All existing projects require MCP server configuration. The Krometrail CLI (session daemon, full command parity with MCP) is unique and addresses environments where MCP setup is inconvenient.

**Browser observation.** No existing MCP-DAP project captures browser activity. Krometrail's CDP recording, session investigation tools, and framework state observation are entirely novel in this space.

## Key Lessons Applied

1. Automatic context return after every execution control operation
2. Hide DAP internals — agents should not need to know `variablesReference`, `frameId`, or scope hierarchies
3. Source context must be automatic, not a separate tool call
4. Minimal tool surface is sufficient — debugger-mcp proves 7 tools can work
5. Framework detection reduces friction for common test frameworks
6. Keep adapters thin — the proxy complexity in mcp-debugger isn't necessary
