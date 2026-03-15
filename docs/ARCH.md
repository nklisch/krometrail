---
head:
  - - meta
    - name: robots
      content: noindex, nofollow
---

# Krometrail — Architecture

---

## System Layers

The system consists of four layers, each with a single responsibility:

**Agent Layer.** The AI coding agent (Claude Code, Codex, or any MCP-compatible client). It reasons about bugs and invokes debug tools when runtime inspection would help. No modifications to the agent are required — it can connect via MCP (discovering tools automatically) or via the CLI (using bash commands with a loaded skill file). Both paths produce identical viewport output.

**MCP Transport Layer.** Standard MCP communication (stdio or SSE). The debug server registers its tools on startup and responds to tool invocations. Event notifications (breakpoint hit, exception thrown) are delivered as the return value of blocking wait calls.

**Debug Server Core.** The central orchestration layer. It manages session lifecycle, translates tool calls into DAP requests, maintains the viewport abstraction, enforces safety limits (timeouts, step budgets), and handles session compression. This layer is language-agnostic.

**Adapter Layer.** Thin, language-specific modules that implement a standard interface for launching a debug target and connecting to its DAP server. Each adapter encapsulates the setup quirks of a specific debugger (debugpy for Python, node-inspect for Node.js, delve for Go, etc.) while exposing a uniform connection surface to the core. See [SPEC.md](SPEC.md) for the adapter contract.

> **Prior art note:** All existing MCP-DAP projects (see [PRIOR_ART.md](PRIOR_ART.md)) use some form of this four-layer architecture. The key difference is complexity: mcp-debugger's proxy layer alone spans ~15 files with worker processes per session, while mcp-dap-server achieves the same result in ~3 files. Krometrail targets the simpler end of this spectrum — thin adapters, no proxy layer, direct DAP communication.

---

## Data Flow

A typical interaction follows this sequence:

1. Agent calls `debug_launch` with a target command and optional initial breakpoints.
2. Debug Server Core selects the appropriate adapter based on file extension or explicit language parameter.
3. Adapter launches the debugee process and establishes a DAP connection.
4. Core sets initial breakpoints via DAP `setBreakpoints` and issues `configurationDone`.
5. The debugee runs until a breakpoint is hit or an exception occurs.
6. Core receives the DAP `stopped` event and constructs a **Viewport Snapshot** (see step detail below).
7. The snapshot is returned to the agent as structured text.

> **Prior art note:** mcp-dap-server's `getFullContext` function follows this same pattern — on every `stopped` event, it queries stack trace, scopes, and variables in sequence. The critical difference: mcp-dap-server returns *all* variables from *all* scopes with no truncation, while Krometrail renders a token-budgeted viewport (~400 tokens). See [PRIOR_ART.md](PRIOR_ART.md) for the full analysis.
8. The agent reasons about the state and issues further debug commands.
9. Repeat until the agent has enough information, then call `debug_stop`.

```
┌─────────────────────────────────────────────────────────┐
│                     AI Coding Agent                      │
│              (Claude Code / Codex / etc.)                 │
└──────────┬──────────────────────────────┬───────────────┘
           │ MCP (stdio / SSE)            │ bash / shell
           ▼                              ▼
┌─────────────────────────────────────────────────────────┐
│                       krometrail                         │
│  ┌────────────────────┐    ┌─────────────────────────┐  │
│  │  MCP Server        │    │  CLI                     │  │
│  │  (tool interface)  │    │  krometrail debug launch ...   │  │
│  │                    │    │  krometrail debug step ...     │  │
│  └─────────┬──────────┘    └────────────┬────────────┘  │
│            └──────────┬─────────────────┘               │
│  ┌────────────────────┴──────────────────────────────┐  │
│  │              Debug Server Core                     │  │
│  │   Session Manager · Viewport Renderer              │  │
│  │   Context Compressor · Safety Limits               │  │
│  └──────────────────────┬────────────────────────────┘  │
│  ┌──────────────────────┴────────────────────────────┐  │
│  │                    Adapter Registry                              │  │
│  │  ┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐···   │  │
│  │  │Python││ Node ││  Go  ││ Rust ││ Java ││ C/C++││ Ruby │      │  │
│  │  └──┬───┘└──┬───┘└──┬───┘└──┬───┘└──┬───┘└──┬───┘└──┬───┘      │  │
│  │     │       │       │       │       │       │       │            │  │
│  │     + C#, Swift, Kotlin                                         │  │
│  └─────┼───────┼───────┼───────┼───────┼───────┼───────┼───────────┘  │
└────────┼───────┼───────┼───────┼───────┼───────┼───────┼──────────────┘
         │ DAP   │ DAP   │ DAP   │ DAP   │ DAP   │ DAP   │ DAP
         ▼       ▼       ▼       ▼       ▼       ▼       ▼
     Debugee  Debugee Debugee Debugee Debugee Debugee Debugee
```

---

## Context Compression

Over a long debug session, accumulated viewport snapshots can consume significant context. The server provides three mechanisms:

**Automatic summarization.** The server maintains a running investigation log summarizing each action and its key observation. After every 10 actions, a compressed summary is appended. The agent can retrieve this via `debug_action_log` at any time, allowing earlier raw viewports to be dropped from context while preserving the reasoning chain.

**Viewport diffing.** When consecutive stops are in the same function, the viewport can optionally show only what changed (modified variables, new stack frames) rather than the full snapshot. Controlled by the `diff_mode` session parameter.

```
── STEP at order.py:148 (same frame) ──
Changed:
  charge_result = <ChargeResult: success=False, error="card_declined">
  (5 locals unchanged)
```

**Progressive compression.** As the action count increases, the viewport automatically reduces detail: fewer stack frames, shorter string previews, more aggressive object summarization. The agent can override this by explicitly requesting full detail via the drill-down tools.

> **Prior art note:** No existing MCP-DAP project implements any form of context compression. All return the same level of detail regardless of session length. See [PRIOR_ART.md](PRIOR_ART.md) lesson #8.

---

## Process Isolation

The debugee runs as a child of the MCP server. Key isolation considerations:

- The debugee inherits server permissions. For untrusted code, run inside a container or sandbox.
- Debugee stdout/stderr is captured up to the configured limit, available via `debug_output`.
- If the debugee crashes or hangs, the session transitions to `terminated`/`error` state with diagnostics.
- The server cleans up all child processes on shutdown, even if sessions are not explicitly stopped.

---

## Viewport Rendering

The viewport is rendered by the Debug Server Core on every stop event. The following pseudocode describes the rendering logic:

```typescript
function renderViewport(
  session: DebugSession,
  config: ViewportConfig
): string {
  const frame = session.currentFrame;
  const lines: string[] = [];

  // Header
  lines.push(`── STOPPED at ${frame.file}:${frame.line} (${frame.function}) ──`);
  lines.push(`Reason: ${session.stopReason}`);
  lines.push('');

  // Call stack (truncated)
  const frames = session.stackFrames.slice(0, config.stack_depth);
  const totalFrames = session.stackFrames.length;
  lines.push(`Call Stack (${frames.length} of ${totalFrames} frames):`);
  for (const [i, f] of frames.entries()) {
    const marker = i === 0 ? '→' : ' ';
    const args = renderArgs(f.arguments, config);
    lines.push(`  ${marker} ${f.shortFile}:${f.line}  ${f.function}(${args})`);
  }
  lines.push('');

  // Source context
  const halfCtx = Math.floor(config.source_context_lines / 2);
  const startLine = Math.max(1, frame.line - halfCtx);
  const endLine = frame.line + halfCtx;
  const source = session.getSource(frame.file, startLine, endLine);
  lines.push(`Source (${startLine}–${endLine}):`);
  for (const sl of source) {
    const marker = sl.line === frame.line ? '→' : ' ';
    lines.push(`${marker}${String(sl.line).padStart(4)}│ ${sl.text}`);
  }
  lines.push('');

  // Locals
  const locals = session.getLocals(0, config.locals_max_items);
  lines.push('Locals:');
  for (const v of locals) {
    const rendered = renderValue(v.value, config.locals_max_depth, config);
    lines.push(`  ${v.name.padEnd(12)} = ${rendered}`);
  }

  // Watch expressions
  if (session.watchExpressions.length > 0) {
    lines.push('');
    lines.push('Watch:');
    for (const expr of session.watchExpressions) {
      const val = session.evaluate(expr, 0);
      const rendered = renderValue(val, 1, config);
      lines.push(`  ${expr.padEnd(20)} = ${rendered}`);
    }
  }

  return lines.join('\n');
}
```
