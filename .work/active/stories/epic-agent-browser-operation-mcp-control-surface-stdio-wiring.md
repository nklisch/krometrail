---
id: epic-agent-browser-operation-mcp-control-surface-stdio-wiring
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-response-mapping]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Stdio and Binary Wiring

## Checkpoint

Serve the completed router over rmcp stdio and root-wire the truthful public `krometrail mcp` command. Make stdout protocol-only and converge EOF/signals/explicit stop through one ownership-aware session shutdown path.

## Likely files

- `crates/krometrail-mcp/src/{server,lib}.rs`
- `src/{cli,app,main}.rs`
- `tests/rust-runtime-smoke.rs`
- `docs/guide/mcp-configuration.md`, `docs/reference/runtime.md` after the command exists

## Acceptance evidence

- `McpService::serve_stdio` uses `ServiceExt::serve(rmcp::transport::stdio())`; no custom framing or alternate transport/runtime is introduced.
- `krometrail mcp` is present in Clap help and starts with the root-injected production `BrowserConnector`. It emits no banner, status, help, or tracing text on stdout.
- stdin EOF and SIGINT/SIGTERM terminate the rmcp loop, clean up the signal waiter, and invoke `BrowserSessionOwner::shutdown` once. Managed browser close versus attached-browser detach remains the existing `BrowserSessionPort::stop` decision.
- Startup, serve, join, and shutdown failures map to safe root-reported errors on stderr. No private source error, browser content, image data, or protocol message is logged.
- Existing `--version`, no-argument help, and `doctor` runtime contracts remain green; current docs roll forward only after the executable command is real.

## Out of scope

Do not add a second executable, daemon mode, network listener, HTTP/SSE/WebSocket transport, authentication, direct browser construction inside MCP, or non-protocol stdout output.
