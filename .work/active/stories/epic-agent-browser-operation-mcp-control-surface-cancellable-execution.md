---
id: epic-agent-browser-operation-mcp-control-surface-cancellable-execution
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-generated-contracts-and-sdk]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Cancellable Browser Execution

## Checkpoint

Add an infrastructure-neutral per-request cancellation context to `BrowserSessionPort::execute` and carry it through the existing production supervisor/cancellation machinery. This makes MCP cancellation honest before handlers ship: a cancelled request cannot return early while a hidden browser mutation continues.

## Likely files

- `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/control/navigation.rs` and focused cancellation tests
- existing core/CDP fake `BrowserSessionPort` implementations and call sites

## Acceptance evidence

- Core exposes `CancellationSignal` and `BrowserOperationContext`; no Tokio or rmcp type enters `krometrail-core`.
- `BrowserSessionPort::execute(request, context)` has no default implementation. Every production/fake caller deliberately passes a context.
- Production `SupervisorCommand::Execute` carries the context, and existing operation cancellation races combine request cancellation with session stop and disconnect for observation, lifecycle/page, interaction, wait, screenshot, and batch paths.
- Cancellation before dispatch produces no CDP command. Cancellation during execution returns stable `cancelled` with available target context and cancels batch children through the shared path.
- Cancelling one request does not stop the browser session or another request; session stop/disconnect behavior remains authoritative and green.

## Out of scope

Do not alter operation semantics, add MCP routes, expose Tokio cancellation in core, replay actions, or create a second executor.
