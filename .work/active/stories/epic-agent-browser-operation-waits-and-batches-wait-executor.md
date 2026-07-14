---
id: epic-agent-browser-operation-waits-and-batches-wait-executor
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-waits-and-batches
depends_on: [epic-agent-browser-operation-waits-and-batches-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Explicit Wait Executor

## Checkpoint

Implement the CDP adapter for elapsed, text, element-state, navigation, page-condition, and explicitly requested network-quiet waits. Reuse `PageControl`, the snapshot resolver, evaluation/navigation projections, transport event subscriptions, `OperationCancellation`, and one absolute monotonic deadline. Do not implement batch coordination or MCP/storage integration in this checkpoint.

## Files

- `crates/krometrail-cdp/src/control/wait.rs` — wait dispatch, bounded probes, lifecycle/network event handling, timeout/cancellation mapping, and wait tests.
- `crates/krometrail-cdp/src/control/mod.rs` — route `Wait` through the existing page-control executor.
- `crates/krometrail-cdp/src/session.rs` and existing domain restoration seams only as needed to pass an internal deadline-aware execution context; preserve the single supervisor owner.

## Acceptance evidence

- Elapsed waits use cancellable sleep; all other conditions use bounded polling, with navigation lifecycle events as wakeups plus document polling as authority.
- Text and element waits preserve selector re-query versus snapshot-reference stale semantics; element state derives from the existing actionability/reference resolver and does not invent a second resolver.
- Navigation waits check readiness and optional exact/prefix URL against the existing document projection; page waits use the existing side-effect-free evaluation path and accept only JSON boolean `true`.
- Network quiet is operation-scoped and opt-in: finite request ids are tracked from subscription onward through request/finish/failure events, the quiet interval resets on a new tracked request, and WebSocket/EventSource limitations and pre-subscription blind spots are reported. Missing event capability fails explicitly instead of claiming quiet.
- A single absolute deadline covers probes, event waits, and command calls. Cancellation/disconnect win deterministically; a condition completing after the deadline is not accepted. `WaitTimedOut` carries target/context and bounded last-probe evidence.
- Event subscriptions are released at completion; no implicit global network-idle behavior or second event broker is added.
- Deterministic fake-transport/fake-clock tests cover every condition, immediate/delayed/timeout paths, event loss with polling fallback, stale references, cancellation, disconnect, malformed evaluation, network lifecycle reset, and no post-deadline probe. Focused CDP tests and locked format/check pass.

## Implementation notes

Use the existing `TransportEvents` named-event contract and domain enablement/restoration path. Keep raw CDP request ids and page text private. Do not replay a wait or refresh an explicit reference after reconnect.
