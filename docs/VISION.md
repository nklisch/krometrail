# Agent Lens — Vision

**Runtime Debugging Viewport for AI Coding Agents**

---

## Executive Summary

AI coding agents such as Claude Code and Codex currently debug software through static code analysis and trial-and-error test execution. They lack the ability to inspect runtime state, set breakpoints, or step through executing code. This makes entire categories of bugs—incorrect runtime values, unexpected mutations, race conditions, off-by-one errors deep in call chains—significantly harder to diagnose.

Agent Lens is a Model Context Protocol (MCP) server that exposes a language-agnostic debugging interface to AI agents. The server translates MCP tool calls into Debug Adapter Protocol (DAP) messages, enabling any DAP-compatible debugger to be used by any MCP-compatible agent without either side needing awareness of the other.

The design prioritizes three qualities: a **compact default viewport** that minimizes token consumption per debug step, a **drill-down-on-demand** pattern that lets the agent selectively expand its view, and a **pluggable adapter layer** that makes adding new languages a bounded, well-defined task.

### Prior Art & Differentiation

Several projects have emerged in this space, all converging on the same MCP-over-DAP architecture:

- **AIDB** (ai-debugger-inc) — Python, JS/TS, Java. Supports `launch.json` configs, framework auto-detection (pytest, jest, django), conditional breakpoints. Most polished positioning as a "debugging standard for AI."
- **mcp-debugger** (debugmcp) — TypeScript. Clean adapter pattern, Python/JS/Rust/Go support, 1000+ tests. Expression evaluation and conditional breakpoints still in progress.
- **mcp-dap-server** (go-delve) — Go. From the Delve team. Generic DAP bridge with demos of autonomous agentic debugging.
- **debugger-mcp** (Govinda-Fichtner) — Rust/Tokio. Python, Ruby, Node.js, Go, Rust. Integration tests using real Claude Code and Codex agents.
- **dap-mcp** (Kashun Cheng) — Python. Config-driven, one of the earlier entries.

All of these solve the **plumbing** problem: bridging MCP to DAP. None of them address the **agent ergonomics** problem, which is the primary focus of Agent Lens. See [UX.md](UX.md) for the detailed gap analysis and how the viewport abstraction addresses each gap. See [PRIOR_ART.md](PRIOR_ART.md) for a deep technical analysis of each project's architecture, tool interfaces, and key lessons.

---

## Problem Statement

Today's coding agents operate in a fundamentally limited debugging loop:

1. Read static source code and error output.
2. Form a hypothesis about the bug.
3. Edit the code based on that hypothesis.
4. Run the test suite and observe pass/fail.
5. Repeat until tests pass or the agent gives up.

This loop works for many surface-level bugs but fails for problems where the root cause is only visible at runtime. A negative discount value, an unexpectedly null reference three frames deep, a loop that executes one too many times—these require observing actual program state during execution. Human developers reach for debuggers in exactly these situations. Agents currently cannot.

The gap is not in reasoning capability but in tooling. Agents already know how to form hypotheses, test them, and iterate. They simply lack the instruments to observe runtime behavior directly.

---

## Implementation Roadmap

### Phase 1: Foundation

Core server with viewport abstraction and the Python adapter, built in **TypeScript on Bun**. Bun is chosen for direct access to the DAP/MCP TypeScript ecosystem (`@vscode/debugadapter`, `@modelcontextprotocol/sdk`), single-file compiled binaries via `bun build --compile`, and dual distribution via npm and standalone binary. Minimum viable product: an agent can launch a Python script, set breakpoints, step, inspect state, and evaluate expressions.

- MCP server scaffold with tool registration and session management
- **CLI with full command parity** — every MCP tool available as a shell command
- DAP client library in TypeScript, leveraging `@vscode/debugadapter` ecosystem
- Viewport renderer with configurable parameters
- Python adapter using debugpy
- Session daemon for CLI state persistence
- Agent skill file for CLI-based integration
- Compiled binary distribution via `bun build --compile` (Linux, macOS, Windows)
- Integration test suite for the full agent-to-debugger path (both MCP and CLI)
- Tool descriptions optimized for agent discovery and usage

### Phase 2: Multi-Language + Intelligence

- Node.js and Go adapters
- Session intelligence: watch expressions, session logging, viewport diffing
- Conditional breakpoint support verified across all adapters
- Context compression (automatic summarization, diff mode)

### Phase 3: Advanced Capabilities

- Rust, Java, and C/C++ adapters
- Attach-to-process for debugging running services
- Multi-threaded debugging with thread selection in viewport
- Remote debugging via DAP over TCP
- Progressive compression tied to action count

### Phase 4: Ecosystem

- Community adapter SDK with documentation and templates
- Adapter contribution guidelines and test harness
- Performance benchmarking: tokens per session, time to diagnosis, fix rate improvement
- Integration guides for Claude Code, Codex, and other MCP clients
- Published tool description patterns for optimal agent behavior

---

## Open Questions

1. **Event delivery model.** Should the server use MCP notifications for async events (breakpoint hit while agent is thinking), or deliver all events synchronously via blocking tool calls? Blocking is simpler but prevents the agent from doing other work while waiting.

2. **Multi-threaded debugging.** How should the viewport handle multiple threads stopped at different locations? Show all threads? Only the active one? A thread selector tool?

3. **Attach vs. launch.** For long-running processes, attach mode is essential. How should the agent discover available processes? PID? Port? Service name?

4. **Security boundaries.** `debug_evaluate` can execute arbitrary code in the debugee's context. Should there be restrictions, or is this acceptable given agents already have code execution capability?

5. **Token budget awareness.** Should the server be aware of the agent's remaining token budget and proactively compress viewports as budget decreases? This would require a non-standard MCP extension.

6. **Integration with existing projects.** Given the active ecosystem (AIDB, mcp-debugger, mcp-dap-server, etc.), should the viewport/compression layer be built as a standalone middleware that wraps any existing MCP debug server, or as contributions to a specific project?

7. **Viewport format.** Should the viewport output be plain text (as shown in this document), structured JSON that the agent parses, or a hybrid? Plain text is more natural for LLMs; JSON is more precise for programmatic consumption.
