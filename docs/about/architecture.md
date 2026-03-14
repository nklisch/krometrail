---
title: Architecture
description: System layers, data flow, and key modules.
---

# Architecture

## System Layers

Four layers, each with a single responsibility:

**Agent Layer** — The AI coding agent (Claude Code, Codex, or any MCP-compatible client). Connects via MCP (discovers tools automatically) or via CLI (uses bash commands with a skill file). Both paths produce identical viewport output.

**MCP Transport Layer** — Standard MCP communication (stdio or SSE). The server registers tools on startup and responds to tool invocations. Blocking calls deliver events synchronously as return values.

**Debug Server Core** — Central orchestration. Manages session lifecycle, translates tool calls into DAP requests, maintains the viewport abstraction, enforces safety limits, and handles context compression.

**Adapter Layer** — Thin, language-specific modules that implement `DebugAdapter`: launch the debugger, return a DAP connection. Each adapter encapsulates its debugger's setup quirks while exposing a uniform connection to the core.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                     AI Coding Agent                      │
│              (Claude Code / Codex / etc.)                │
└──────────┬──────────────────────────────┬───────────────┘
           │ MCP (stdio / SSE)            │ bash / shell
           ▼                              ▼
┌─────────────────────────────────────────────────────────┐
│                       krometrail                         │
│  ┌────────────────────┐    ┌─────────────────────────┐  │
│  │  MCP Server        │    │  CLI                     │  │
│  │  (tool interface)  │    │  krometrail launch ...   │  │
│  └─────────┬──────────┘    └────────────┬────────────┘  │
│            └──────────┬─────────────────┘               │
│  ┌────────────────────┴──────────────────────────────┐  │
│  │              Debug Server Core                     │  │
│  │   Session Manager · Viewport Renderer              │  │
│  │   Context Compressor · Safety Limits               │  │
│  └──────────────────────┬────────────────────────────┘  │
│  ┌──────────────────────┴────────────────────────────┐  │
│  │             Adapter Registry                       │  │
│  │   Python   Node.js   Go   Rust   Java   C/C++     │  │
│  └───────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
                           │ DAP (TCP or stdio)
                           ▼
                       Debugee process
```

## Source Layout

```
src/
  mcp/          MCP server + tool handlers
  cli/          CLI entry point + commands (citty)
  core/         Session manager, viewport renderer, DAP client, compression
  adapters/     Language-specific debugger adapters (6 languages)
  browser/      Chrome CDP recording, investigation engine, framework observers
  daemon/       Session persistence over Unix socket
  frameworks/   Auto-detection for test/web frameworks
```

## Data Flow

1. Agent calls `debug_launch` with a target command and optional initial breakpoints
2. Core selects the appropriate adapter from the registry (by file extension or explicit language)
3. Adapter launches the debugger process and establishes a DAP connection
4. Core sets initial breakpoints via DAP `setBreakpoints` and issues `configurationDone`
5. Debugee runs until a breakpoint fires
6. Core receives the DAP `stopped` event and constructs the viewport snapshot
7. Snapshot is returned to the agent
8. Agent issues further commands; repeat until `debug_stop`

## Key Design Choices

**MCP and CLI share the same core.** The CLI communicates with the same session manager as the MCP server, via a Unix socket daemon. This ensures identical behavior regardless of which interface the agent uses.

**Viewport on every stop.** Every execution control operation (`continue`, `step`, `run_to`) automatically returns the viewport without requiring a separate state query. This eliminates the 3–4 round-trip pattern common in other MCP-DAP bridges.

**Thin adapters.** The adapter boundary is intentionally narrow — adapters only handle debugger launch and connection setup. All DAP protocol communication, viewport construction, and session intelligence live in the core.

**Browser and debug tools are separate.** Browser observation (CDP) and runtime debugging (DAP) are independent subsystems in the same binary. They can be used independently or together.
