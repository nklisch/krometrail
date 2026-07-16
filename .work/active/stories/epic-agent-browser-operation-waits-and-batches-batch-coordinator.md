---
id: epic-agent-browser-operation-waits-and-batches-batch-coordinator
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-waits-and-batches
depends_on: [epic-agent-browser-operation-waits-and-batches-wait-executor]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Ordered Per-Target Batch Coordinator

## Checkpoint

Add batch execution to the existing session operation path. Admit one target, allocate one batch correlation id, execute each child sequentially through the exact standalone dispatcher, preserve normal results/anchors/timing, implement default stop-on-failure and explicit continuation, collect optional per-step screenshots, and always attempt one final live observation within the shared deadline.

## Files

- `crates/krometrail-cdp/src/control/batch.rs` — coordinator, child normalization, failure/skip policy, screenshot extraction, and finalization tests.
- `crates/krometrail-cdp/src/session.rs` — route `Batch`, propagate internal parent/deadline context, and retain single-writer target ownership.
- `crates/krometrail-cdp/src/control/interaction.rs` and lifecycle result helpers — additive parent-batch propagation only; no duplicated action routing or completion policy.

## Acceptance evidence

- Children run in request order and remain on the admitted target; selected children cannot drift, explicit mismatches fail, browser-scoped/close/nested operations are rejected, and different targets never acquire an implicit order.
- Default `StopOnFailure` stops at the first ordinary child failure; `ContinueOnFailure` runs later independent children. Cancellation, disconnect, target failure, and global deadline always stop and mark remaining children skipped with explicit reasons. No rollback or replay occurs.
- State-changing children return their existing `InteractionAnchor` and `InteractionTiming`; child `InteractionRecord`s receive `parent_batch: Some(batch_id)`. Read-only waits/inspection have no fabricated anchor. A wait timeout and anchored page failure are represented as child failures rather than top-level guessed success.
- The batch uses one absolute deadline, allocates the existing `InteractionId` correlation type, obtains optional screenshots through the existing screenshot/live-evidence path, and performs exactly one final `observe_live` unless the target/deadline makes it unavailable. Degraded evidence remains an explicit `ObservationPart`.
- Batch results contain every attempted/skipped step in order, concrete standalone results where available, bounded errors and skip reasons, monotonic child timing, final outcome, and final observation. Already-applied browser state is not undone.
- Deterministic session/transport tests cover order, target admission, stop/continue, skip reasons, anchors/parent correlation, timing, screenshots, exactly-one final observation, final-observation failure, cancellation, disconnect, and no cross-target/nested execution. Focused tests and locked check pass.

## Implementation notes

The coordinator is a thin sequential wrapper over `execute_operation`; it must not call `Input.*`, `Page.*`, `Runtime.*`, screenshot composition, or locator resolution directly. Public requests remain unchanged; deadline and parent id travel in private execution context.

## Implementation notes

- Added `crates/krometrail-cdp/src/control/batch.rs` as a sequential coordinator over the existing `session::execute_operation` dispatcher. It contains no CDP action, navigation, evaluation, screenshot-composition, or locator command logic.
- Added private `OperationExecutionContext` carrying only the shared absolute deadline and optional parent batch id. Standalone calls use the default context; batch children pass the same deadline and `Some(batch_id)`. Verified interaction records now receive that id without changing public action requests, and `InteractionResult::anchor()` derives the normal public child anchor from its existing record timing.
- Admission binds one target once, then re-resolves every child selection immediately before dispatch and refuses drift. Registry validation remains the public first boundary; runtime validation protects `Selected` semantics and target loss.
- Implemented stop/continue policy, terminal cancellation/deadline/disconnect handling, explicit skipped reasons with absent execution timing, wait-timeout/page-failure classification, preserved concrete standalone results, and no rollback or replay.
- Optional per-step screenshots reuse an existing child live screenshot when present or dispatch the ordinary `TakeScreenshot` operation. Finalization dispatches exactly one ordinary `ObserveLive` unless cancellation, deadline, or target loss makes that impossible; all degradation remains an `ObservationPart`.
- Added scripted production-port coverage in `crates/krometrail-cdp/tests/waits_and_batches.rs` for sequential dispatch, parent correlation/anchors, stop versus continue, preserved failed results, screenshot policy, and exactly-one final observation.
- Verification: `cargo fmt --all`; `cargo test -p krometrail-cdp --all-targets` (202 passed across 18 suites); `cargo test -p krometrail-core --all-targets` (70 passed); `cargo check -p krometrail-cdp --all-targets --locked` (passed).
