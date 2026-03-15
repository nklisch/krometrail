---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Krometrail — Prior Art Reference

Analysis of existing MCP-over-DAP projects, their approaches, and key insights for Krometrail.

---

## Project Overview

| Project | Language | Stars | License | Created | Last Updated |
|---------|----------|-------|---------|---------|--------------|
| [mcp-debugger](https://github.com/debugmcp/mcp-debugger) | TypeScript | 76 | MIT | Jun 2025 | Mar 2026 |
| [mcp-dap-server](https://github.com/go-delve/mcp-dap-server) | Go | 53 | MIT | Jul 2025 | Mar 2026 |
| [dap-mcp](https://github.com/KashunCheng/dap_mcp) | Python | 34 | AGPL-3.0 | Mar 2025 | Mar 2026 |
| [AIDB](https://github.com/ai-debugger-inc/aidb) | Python | 13 | Apache-2.0 | Dec 2025 | Feb 2026 |
| [debugger-mcp](https://github.com/Govinda-Fichtner/debugger-mcp) | Rust | 4 | MIT | Oct 2025 | Jan 2026 |

---

## 1. AIDB (ai-debugger-inc)

### Architecture

Python monorepo with a clean module split:
- `aidb/` — Core debugging API, language adapters, session management
- `aidb_mcp/` — MCP server layer exposing tools
- `aidb_cli/` — Developer CLI for testing and adapter builds
- `aidb_common/`, `aidb_logging/` — Shared utilities

Debug adapters are **built during CI and published as release artifacts**, then automatically downloaded on first run. This is notable — instead of requiring users to install debugpy/node-inspector/etc separately, AIDB bundles pre-built adapters.

### Language Support

| Language | Adapter | Min Version |
|----------|---------|-------------|
| Python | debugpy (Microsoft) | 3.10+ |
| JavaScript/TypeScript | vscode-js-debug (Microsoft) | Node 18+ |
| Java | java-debug (Microsoft) | JDK 17+ |

### Key Insights

**Framework auto-detection.** AIDB auto-identifies test frameworks (pytest, jest, django, spring, flask) and configures the debug adapter accordingly. This eliminates a major friction point — agents don't need to figure out how to configure the debugger for a specific framework.

**launch.json reuse.** Can consume VS Code `launch.json` configurations without VS Code installed. This is pragmatic — many projects already have launch configs, and reusing them avoids duplicating debug configuration.

**Minimal dependencies.** Only 3 Python packages: `aiofiles`, `mcp`, `psutil`. Everything else (adapters) is downloaded on demand.

**Live code patching.** Supports modifying code during execution — goes beyond standard DAP inspection.

### Gaps

- Tool interface details are sparse in public docs. The tools are described as "agent-optimized" but specific tool schemas aren't well documented.
- No viewport abstraction — returns raw DAP state to the agent.
- No context compression or session intelligence.

---

## 2. mcp-debugger (debugmcp)

### Architecture

TypeScript, pnpm workspace, heavily factored into modules:

```
src/
  adapters/        Adapter loader + registry (dynamic discovery)
  cli/             CLI commands (stdio, sse)
  container/       Dependency injection
  dap-core/        DAP protocol handling, state machine, types
  factories/       Factory pattern for proxy/session creation
  proxy/           DAP proxy layer — message parsing, request tracking, connection mgmt
  session/         Session manager (core, data, operations split)
  server.ts        MCP server with tool registration
```

The proxy layer is the most complex component — it sits between the MCP server and the DAP server, parsing and routing DAP messages. The proxy runs as a separate worker process per session.

### Tool Interface (19 tools)

| Tool | Description |
|------|-------------|
| `create_debug_session` | Create session (launch or attach mode) |
| `list_supported_languages` | Enumerate available adapters |
| `list_debug_sessions` | Show active sessions |
| `set_breakpoint` | Set breakpoint with optional condition |
| `start_debugging` | Launch a script for debugging |
| `attach_to_process` | Attach to running process |
| `detach_from_process` | Detach without terminating |
| `close_debug_session` | Terminate session |
| `step_over` | Step over |
| `step_into` | Step into |
| `step_out` | Step out |
| `continue_execution` | Continue |
| `pause_execution` | Pause (not implemented) |
| `get_variables` | Get variables by scope reference |
| `get_local_variables` | Convenience: locals for current frame |
| `get_stack_trace` | Stack trace (with `includeInternals` filter) |
| `get_scopes` | Scopes for a stack frame |
| `evaluate_expression` | Expression evaluation |
| `get_source_context` | Source code around a line |

### Key Insights

**Two-step session model.** Sessions are created first (`create_debug_session`), then scripts are started (`start_debugging`). This separation lets agents configure the session (set breakpoints, etc.) before execution begins. Contrast with mcp-dap-server's single `debug` tool that does everything.

**`get_local_variables` convenience tool.** Standard DAP requires a multi-step dance: stack trace → scope → variables request. This tool collapses it into one call. Has an `includeSpecial` flag to filter out `__builtins__`, `__proto__`, etc. — a pragmatic UX choice for agents.

**`get_source_context` tool.** Returns source code around a line with configurable context (`linesContext` parameter). This is the closest any existing project comes to a viewport — but it's a separate tool call, not automatic.

**Detailed breakpoint guidance.** The `set_breakpoint` tool description explicitly warns about non-executable lines: *"Setting breakpoints on non-executable lines (structural, declarative) may lead to unexpected behavior."* This kind of agent guidance in tool descriptions is valuable.

**Internal frame filtering.** `get_stack_trace` has `includeInternals` flag to hide Node.js/runtime internals. Reduces noise for agents.

**Dynamic adapter loading.** Adapters are discovered at runtime, lazily loaded, and cached. New adapters can be added without touching core code. Adapters for js-debug and CodeLLDB are vendored (downloaded) during install.

**Proxy architecture.** Each session spawns a separate DAP proxy worker process. This provides isolation but adds complexity — there's extensive code for proxy lifecycle, message parsing, orphan detection, and signal handling.

### Gaps

- `pause_execution` and `evaluate_expression` are listed but marked as not implemented or in-progress.
- No viewport abstraction — each piece of state (stack, variables, source) requires a separate tool call.
- No session logging or context compression.
- The tool interface requires agents to understand DAP concepts like `variablesReference` and `frameId` — leaky abstraction.
- Very complex codebase (proxy layer alone is ~15 files) for what it does.

---

## 3. mcp-dap-server (go-delve)

### Architecture

Single-file Go project (~1000 lines in `tools.go`, plus `dap.go` and `main.go`). Remarkably simple. The entire MCP-to-DAP bridge fits in three files.

Dependencies: only `github.com/google/go-dap` and `github.com/modelcontextprotocol/go-sdk/mcp`.

### Tool Interface (13 tools, capability-gated)

The most interesting design in the group — tools are **dynamically registered based on DAP server capabilities**:

| Tool | Description | Registration |
|------|-------------|-------------|
| `debug` | Unified launcher (source/binary/core/attach) | Always (pre-session) |
| `stop` | End session | Session-active |
| `breakpoint` | Set by file:line or function name | Session-active |
| `clear-breakpoints` | Remove by file or clear all | Session-active |
| `continue` | Resume, with optional run-to-cursor | Session-active |
| `step` | Over/in/out | Session-active |
| `pause` | Pause thread | Session-active |
| `context` | Full debugging context dump | Session-active |
| `evaluate` | Expression evaluation | Session-active |
| `info` | Sources and modules | Session-active |
| `restart` | Restart session | **Only if DAP supports it** |
| `set-variable` | Modify variable value | **Only if DAP supports it** |
| `disassemble` | Disassemble at address | **Only if DAP supports it** |

### Key Insights

**Dynamic tool registration.** This is the standout design pattern. Before a debug session starts, only `debug` is available. Once the session starts, `debug` is **removed** and session tools are **registered**. When the session ends, session tools are removed and `debug` reappears. This means the agent's tool list always reflects exactly what's currently possible.

Capability-gated tools go further: `restart`, `set-variable`, and `disassemble` only appear if the underlying DAP server advertises support. The agent never sees tools that won't work.

```go
// registerSessionTools removes the debug tool and registers session tools
func (ds *debuggerSession) registerSessionTools() {
    ds.server.RemoveTools("debug")
    // ... register session tools ...
    if ds.capabilities.SupportsRestartRequest {
        mcp.AddTool(ds.server, &mcp.Tool{Name: "restart", ...}, ds.restartDebugger)
    }
}
```

**`getFullContext` — proto-viewport.** The `context` tool (and the return value of `step`/`continue`) dumps a structured text block with:
- Current location (function, file:line)
- Full stack trace (with frame IDs)
- All scopes with all variables (name, type, value)

This is the closest any project comes to our viewport concept. The key difference: it dumps **everything** (all scopes, all variables, no truncation) rather than a token-budgeted compact view. It uses markdown-like formatting (`## Current Location`, `## Stack Trace`, `## Variables`).

**Automatic context return.** `step` and `continue` automatically call `getFullContext` when the program stops, returning the full state without a separate tool call. The agent gets location + stack + variables in one response. This eliminates the multi-tool-call pattern that plagues mcp-debugger.

**Run-to-cursor via `continue`.** The `continue` tool accepts a `to` parameter (file:line or function name), setting a temporary breakpoint. This eliminates a separate `run_to` tool.

**Single-session model.** Only one debug session at a time. When `debug` is called, it replaces the pre-session state entirely. Simplicity over flexibility.

**Delve-specific.** Despite the generic name, the implementation hardcodes Delve (`dlv dap`). The DAP abstraction is there in theory but the launch logic is Delve-only.

### Gaps

- Go/Delve only. No adapter abstraction for other languages.
- No breakpoint conditions or logpoints.
- No source context (doesn't show source code around the current line).
- Full variable dump with no truncation — will blow up context for complex programs.
- Single session only.

---

## 4. debugger-mcp (Govinda-Fichtner)

### Architecture

Rust/Tokio async MCP server. Supports 5 languages via Docker containers (one Dockerfile per language with the debugger pre-installed).

### Tool Interface (7 tools, SBCTED mnemonic)

| Tool | Description |
|------|-------------|
| `debugger_start` | Start debug session |
| `debugger_set_breakpoint` | Set a breakpoint |
| `debugger_continue` | Continue execution |
| `debugger_stack_trace` | Get call stack |
| `debugger_evaluate` | Evaluate expression |
| `debugger_wait_for_stop` | Block until program stops |
| `debugger_disconnect` | End session |

### Key Insights

**Real agent integration testing.** The standout contribution: integration tests run against **actual Claude Code and Codex agents**, not mocked clients. 5 languages x 2 agents = 10 test matrices. Each test has the agent autonomously debug a program through the full SBCTED sequence. This proves end-to-end viability.

**Docker-per-language.** Each language has a dedicated Dockerfile with the debugger pre-installed. This solves the "install debugpy/delve/etc" friction but makes the project heavy — you need Docker to use any language.

**`debugger_wait_for_stop` — blocking event delivery.** Instead of callbacks or notifications, the agent calls `wait_for_stop` which blocks until the debugee hits a breakpoint or exception. This is the synchronous event model our design doc discusses.

**Minimal tool surface.** Only 7 tools vs 19 (mcp-debugger) or 13 (mcp-dap-server). The philosophy is fewer, more composable tools.

### Gaps

- Very small community (4 stars). Limited activity since Jan 2026.
- No variable inspection tool — only expression evaluation.
- No conditional breakpoints or logpoints.
- Docker requirement for all languages.
- No source context viewing.

---

## 5. dap-mcp (KashunCheng)

### Architecture

Python, config-driven. One of the earliest entries (Mar 2025). Uses Pydantic for configuration validation.

### Tool Interface (12 tools)

| Tool | Description |
|------|-------------|
| `launch` | Start debuggee |
| `set_breakpoint` | Create breakpoint with optional condition |
| `remove_breakpoint` | Delete breakpoint |
| `list_all_breakpoints` | Enumerate breakpoints |
| `continue_execution` | Resume |
| `step_in` | Step into |
| `step_out` | Step out |
| `next` | Step over |
| `evaluate` | Expression evaluation |
| `change_frame` | Switch stack frames |
| `view_file_around_line` | Source code context |
| `terminate` | End session |

### Key Insights

**Config-driven adapter selection.** Instead of auto-detection, the user provides a JSON config specifying the debugger:

```json
{
  "type": "debugpy",
  "debuggerPath": "/usr/bin/python3",
  "debuggerArgs": ["-m", "debugpy.adapter"],
  "sourceDirs": ["/home/user/project/src"]
}
```

The `type` field is a discriminated union (`"debugpy"` | `"lldb"`). Adding a new debugger means creating a new Pydantic `DAPConfig` subclass with a unique `type` literal.

**`view_file_around_line`** — dedicated source viewing tool. Returns source code around a line, similar to mcp-debugger's `get_source_context`. Remembers the last-accessed file for repeated viewing.

**`change_frame` tool.** Explicit frame switching as a tool call. Other projects handle this via parameters on inspection tools (`frameId`). Having it as a separate tool makes frame navigation a first-class operation.

**Source directory mapping.** The `sourceDirs` config enables relative-to-absolute path resolution, making configurations portable across environments.

**XML output.** Returns XML-rendered output instead of JSON or plain text. Unusual choice — presumably for structured parsing by agents.

### Gaps

- AGPL license is restrictive for some use cases.
- Single session model.
- Only debugpy and lldb adapters.
- No automatic framework detection.
- XML output format is unusual and potentially token-heavy.

---

## Cross-Project Patterns & Lessons

### What everyone does the same way
- **DAP as the protocol bridge.** No project reimplements debugger internals. All use existing DAP-compatible debuggers.
- **MCP tool per operation.** Step, continue, breakpoint, evaluate are always separate tools.
- **Session-based model.** Launch creates a session, operations reference it, stop destroys it.

### Where they diverge

| Decision | Approaches |
|----------|-----------|
| **Session creation** | Single combined `debug`/`launch` (dap-server, dap-mcp) vs two-step create-then-start (mcp-debugger) |
| **State return** | Automatic full context on every stop (dap-server) vs separate tool calls for each piece (mcp-debugger, debugger-mcp) |
| **Adapter management** | Bundled/vendored (AIDB, mcp-debugger) vs config-driven (dap-mcp) vs Docker (debugger-mcp) |
| **Tool registration** | Static full list (most) vs dynamic capability-gated (dap-server) |
| **Output format** | Plain text (dap-server) vs JSON (mcp-debugger) vs XML (dap-mcp) |

### Key lessons for Krometrail

1. **Automatic context return is critical.** mcp-dap-server's pattern of returning full state after every `step`/`continue` eliminates unnecessary round trips. Our viewport snapshot should always be returned with every execution control operation.

2. **Dynamic tool registration is elegant but not essential.** The go-delve approach of only showing available tools is clean, but adds complexity. Our approach (always show all tools, return errors for invalid states) is simpler and still works — agents handle errors well.

3. **DAP concepts should not leak into the agent interface.** mcp-debugger requires agents to understand `variablesReference`, `frameId`, and scope hierarchies. Our viewport abstraction hides these behind a flat, readable snapshot.

4. **Source context must be automatic.** Most projects require a separate tool call to see source code. Only mcp-dap-server returns it automatically (as part of `getFullContext`), and even then it doesn't include actual source lines — just file:line references. Our viewport includes source with every stop.

5. **Agent integration testing is viable.** debugger-mcp proves you can test against real agents. We should do this for e2e tests (and their 10/10 pass rate with only 7 tools suggests a minimal tool surface is sufficient).

6. **Framework detection reduces friction.** AIDB's auto-detection of pytest/jest/django means agents don't need to figure out debug configurations. Worth considering for Phase 2.

7. **The proxy/adapter complexity spectrum.** mcp-debugger's 15-file proxy layer vs mcp-dap-server's single file shows the same result can be achieved with vastly different complexity. Lean toward simplicity.

8. **No one has solved the token problem.** Every project returns raw DAP state with no token awareness. This is the gap Krometrail exists to fill.
