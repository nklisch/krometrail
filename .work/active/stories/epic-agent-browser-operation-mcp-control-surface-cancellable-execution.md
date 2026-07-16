---
id: epic-agent-browser-operation-mcp-control-surface-cancellable-execution
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-generated-contracts-and-sdk]
release_binding: 1.0.0
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

## Implementation notes

- Execution capability: highest from the autopilot caller; cancellation crosses the public core port and every production operation family.
- Review weight: `standard` from the autopilot caller; this child advances directly to done.
- Files changed: core browser port/export contracts; CDP session supervisor, operation cancellation view, read-only dispatch, batch propagation; existing CDP call sites and focused lifecycle tests.
- Tests added: pre-dispatch cancellation proves zero CDP commands; in-flight navigation cancellation proves stable failure and that another request/session remain usable.
- Simplification: a request-scoped `OperationCancellation` view composes the existing shared stop/disconnect state rather than creating another executor or Tokio-facing core type.
- Discrepancies from design: an already-sent CDP future may be dropped by a cancellation race but cannot be unsent; guarantees are before dispatch and at cancellable round-trip, wait, batch-loop, and observation boundaries.
- Adjacent issues parked: none.

## Completion evidence

- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passed.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 test -p krometrail-core -p krometrail-cdp --all-targets --locked` passed: 71 core tests, 93 CDP unit tests, and all enabled deterministic integration suites including 15 page-lifecycle tests.
