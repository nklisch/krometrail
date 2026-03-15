---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Krometrail — Vision

**Runtime Debugging Viewport for AI Coding Agents**

---

## Executive Summary

AI coding agents such as Claude Code and Codex currently debug software through static code analysis and trial-and-error test execution. They lack the ability to inspect runtime state, set breakpoints, or step through executing code. This makes entire categories of bugs—incorrect runtime values, unexpected mutations, race conditions, off-by-one errors deep in call chains—significantly harder to diagnose.

Krometrail is a Model Context Protocol (MCP) server that exposes a language-agnostic debugging interface to AI agents. The server translates MCP tool calls into Debug Adapter Protocol (DAP) messages, enabling any DAP-compatible debugger to be used by any MCP-compatible agent without either side needing awareness of the other.

The design prioritizes three qualities: a **compact default viewport** that minimizes token consumption per debug step, a **drill-down-on-demand** pattern that lets the agent selectively expand its view, and a **pluggable adapter layer** that makes adding new languages a bounded, well-defined task.

### Prior Art & Differentiation

Several projects have emerged in this space, all converging on the same MCP-over-DAP architecture:

- **AIDB** (ai-debugger-inc) — Python, JS/TS, Java. Supports `launch.json` configs, framework auto-detection (pytest, jest, django), conditional breakpoints. Most polished positioning as a "debugging standard for AI."
- **mcp-debugger** (debugmcp) — TypeScript. Clean adapter pattern, Python/JS/Rust/Go support, 1000+ tests. Expression evaluation and conditional breakpoints still in progress.
- **mcp-dap-server** (go-delve) — Go. From the Delve team. Generic DAP bridge with demos of autonomous agentic debugging.
- **debugger-mcp** (Govinda-Fichtner) — Rust/Tokio. Python, Ruby, Node.js, Go, Rust. Integration tests using real Claude Code and Codex agents.
- **dap-mcp** (Kashun Cheng) — Python. Config-driven, one of the earlier entries.

All of these solve the **plumbing** problem: bridging MCP to DAP. None of them address the **agent ergonomics** problem, which is the primary focus of Krometrail. See [UX.md](UX.md) for the detailed gap analysis and how the viewport abstraction addresses each gap. See [PRIOR_ART.md](PRIOR_ART.md) for a deep technical analysis of each project's architecture, tool interfaces, and key lessons.

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

## What's Implemented

All planned phases are complete. The system ships with:

- **Runtime debugging** — 10 language adapters (Python, Node.js, Go, Rust, Java, C/C++, Ruby, C#, Swift, Kotlin), viewport abstraction with token-budgeted rendering, context compression, watch expressions, session logging, multi-threaded debugging, attach mode, framework auto-detection
- **Browser observation** — CDP-based passive recording (network, console, DOM, storage, screenshots, user input), SQLite persistence, investigation tools (search, inspect, diff, replay context), HAR export, marker/screenshot system, lightweight annotation API (`window.__krometrail.mark()`)
- **Framework state observation** — React and Vue state observers with component tree walking, state diffing, store integration (Pinia/Vuex), and bug pattern detection (infinite re-renders, stale closures, context floods, lost reactivity)
- **Dual interface** — MCP server and CLI with full parity, namespaced under `debug` and `browser` subcommands, JSON envelope output, semantic exit codes
- **Testing** — Unit, integration, e2e, and agent harness test suites with real debuggers and real browser fixtures

See `docs/designs/completed/` for historical design documents covering each phase.

---

## Resolved Design Questions

These questions from the original design have all been decided:

1. **Event delivery model** — Synchronous blocking. Every execution control tool returns the viewport on stop. No MCP notifications needed.
2. **Multi-threaded debugging** — Active thread shown in viewport by default. `debug_threads` tool lists all threads. Thread selection via `thread_id` parameter.
3. **Attach vs. launch** — Both supported. Attach by PID or port via `debug_attach` / `krometrail debug attach`.
4. **Security boundaries** — No restrictions on `debug_evaluate`. Agents already have code execution capability.
5. **Token budget awareness** — Progressive compression based on action count. No MCP extension needed.
6. **Integration** — Built as a standalone project with its own viewport/compression layer.
7. **Viewport format** — Plain text by default, JSON via `--json` flag or MCP structured output. Both produce the same information.
