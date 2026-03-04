# Agent Lens

**Runtime debugging viewport for AI coding agents.**

Agent Lens is an MCP server and CLI that gives AI coding agents the ability to set breakpoints, step through code, and inspect runtime state. It bridges the Model Context Protocol (MCP) to the Debug Adapter Protocol (DAP), wrapping raw debugger state in a compact viewport optimized for LLM consumption (~400 tokens per stop).

## Documentation

| Document | Contents |
|----------|----------|
| [VISION.md](docs/VISION.md) | Why this exists, prior art, problem statement, roadmap, open questions |
| [ARCH.md](docs/ARCH.md) | System layers, data flow, context compression, process isolation, viewport rendering |
| [UX.md](docs/UX.md) | Viewport abstraction, value rendering, agent interaction patterns, skill file |
| [SPEC.md](docs/SPEC.md) | Adapter contract, type definitions, reference adapters, resource limits |
| [INTERFACE.md](docs/INTERFACE.md) | MCP tool reference, CLI command reference, session daemon, example sessions |
| [TESTING.md](docs/TESTING.md) | Testing philosophy, test tiers, fixtures, debugger setup |
| [PRIOR_ART.md](docs/PRIOR_ART.md) | Analysis of existing MCP-DAP projects, approaches, and key lessons |
| [ROADMAP.md](docs/ROADMAP.md) | Phased implementation plan with detailed steps per phase |

## Quick Start

```bash
# MCP path — add to your agent's MCP config
agent-lens --mcp

# CLI path — use directly from bash
agent-lens launch "python app.py" --break order.py:147
agent-lens continue
agent-lens eval "discount"
agent-lens stop
```

## Status

Design phase. See [ROADMAP.md](docs/ROADMAP.md) for the detailed implementation plan.
