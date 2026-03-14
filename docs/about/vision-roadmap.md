---
title: Vision & Roadmap
description: Project vision and development roadmap.
---

# Vision & Roadmap

## The Problem

AI coding agents debug software through static code analysis and trial-and-error test execution. This loop works for many surface-level bugs but fails for problems visible only at runtime: incorrect values, unexpected mutations, race conditions, off-by-one errors deep in call chains.

The gap is not in reasoning capability but in tooling. Agents know how to form hypotheses and iterate. They simply lack the instruments to observe runtime behavior directly.

Krometrail provides two instruments:

1. **Browser observation** — passive recording of everything happening in Chrome (network, console, DOM, storage, screenshots, framework state) for investigation after the fact
2. **Runtime debugging** — breakpoint-level debugging across 6 languages via DAP, with a token-efficient viewport designed specifically for LLM context windows

## Design Priorities

**Compact default viewport.** Every debug stop returns ~400 tokens of structured context — source, locals, and call stack. Sustainable over dozens of steps without exhausting context.

**Drill-down on demand.** The default view is shallow; agents expand selectively via `debug_evaluate` and `debug_variables`. No tool call required just to see state.

**Session intelligence.** Investigation logs, viewport diffing, and progressive compression keep long debug sessions manageable.

**CLI parity.** Every MCP tool has a corresponding shell command. Agents with bash access can debug as effectively as those with MCP support.

## Roadmap

### Phase 1: Core Debug Loop (Complete)

- Python adapter with full DAP support
- Viewport renderer with configurable parameters
- MCP server + CLI with full command parity
- Session daemon for CLI state persistence
- Compiled binary distribution (Linux, macOS, Windows)

### Phase 2: Multi-Language + Intelligence (Complete)

- Node.js, Go, Rust, Java, C/C++ adapters
- Watch expressions, session logging, viewport diffing
- Conditional breakpoints across all adapters
- Context compression (auto-summarization, diff mode)

### Phase 3: Browser Observation (Complete)

- Chrome CDP recording (network, console, DOM, storage, screenshots)
- Session investigation tools (search, inspect, diff, replay-context)
- React and Vue framework state observation
- Bug pattern detection (stale closures, infinite re-renders, etc.)

### Phase 4: Ecosystem (In Progress)

- Svelte and Solid framework observers
- Adapter SDK documentation and contribution guidelines
- Performance benchmarking: tokens per session, time to diagnosis, fix rate
- Agent harness expansion: more scenarios, more languages
- Remote debugging via DAP over TCP
- Ruby, C#, Swift, Kotlin adapters

## Open Questions

- **Bun debugging** — Bun uses WebKit JSC protocol, not V8 CDP. The adapter exists but is not supported until Bun's CDP implementation fires `Debugger.paused` events correctly.
- **Token budget awareness** — Should the server be aware of the agent's remaining context budget and proactively compress? Would require a non-standard MCP extension.
- **Attach-to-process** — Implemented but less tested than launch mode. Feedback welcome on attach workflows.
- **Multi-session coordination** — When two sessions are active simultaneously, how should the agent choose? Current approach: require `--session <id>`. Better UX ideas welcome.
