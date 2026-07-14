---
id: refactor-dedupe-reconnect-operation-rejection
kind: story
stage: done
tags: [refactor, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Deduplicate rejected operations during reconnect

## Brief

`crates/krometrail-cdp/src/session/reconnect.rs:462-468` and `:545-551` contain the same operation-rejection policy while the supervisor waits through reconnect backoff and while a reconnect transaction is in flight. Both derive the direct target and send the identical `BrowserDisconnected` error with `browser is reconnecting; operation was not replayed`.

Extract one private helper for this reconnect-boundary rejection and call it from both command-select branches. Keep the oneshot send/discard behavior, target anchoring, error code, retry metadata, and exact message unchanged. Do not combine the two reconnect loops or alter their cancellation/interrupt control flow.

**Source lens**: elimination / missing abstraction

**Rationale**: makes the reconnect rejection policy single-source and prevents the two asynchronous reconnect phases from drifting in caller-visible error behavior.

**Black-box classification**: pure refactor. For every rejected operation, the helper must produce the same `BrowserDisconnected` error, optional target identity, retry advice, message, and sender-consumption behavior as the duplicated blocks.

## Acceptance criteria

- [ ] One private helper owns the reconnecting-operation rejection mapping.
- [ ] Both reconnect command-select branches call the helper; the duplicated target/error construction is removed.
- [ ] Reconnect backoff, transaction cancellation, command ordering, and operation non-replay behavior remain unchanged.
- [ ] Existing reconnect/session supervision tests pass, including target-anchored and browser-scoped rejected operations.
- [ ] `cargo fmt --all -- --check`, targeted CDP session tests, and the locked workspace quality gates pass.

## Risk and rollback

**Risk**: Low. The two blocks are textually identical, but the helper must consume the oneshot sender in the same way in both select contexts.

**Rollback**: Revert the implementation commit to restore the two inline rejection blocks.

## Implementation notes

- Execution capability: baseline inline ownership; one private helper in the extracted reconnect module.
- `reject_operation_during_reconnect` consumes the original request and oneshot sender, derives the same direct target, and sends the unchanged `BrowserDisconnected` error from both reconnect phases.
- Both select-loop control-flow shapes remain unchanged: backoff continues after rejection, and in-flight transaction rejection still yields `None` rather than interrupting the attempt.
- Target-file rustfmt check, all CDP all-target tests, and CDP all-target Clippy with warnings denied passed.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: Combining reconnect backoff and transaction command loops would change cancellation and interruption ownership and is correctly excluded.

**Evidence**: Bounded standalone-story review inspected commit `fdf1c29`, verified the helper reproduces the exact target anchoring, error code/message/retry mapping, sender consumption, and per-loop continuation result, and relied on the full CDP all-target suite plus Clippy. No independent reviewer ran, as required for a standalone story.

## Discovery notes

- Scope: committed implementation paths touched from `e798b63` through committed `HEAD` `6e65586`, with direct focus on the post-split CDP session modules; uncommitted temporal-query work was excluded.
- Dispatch: direct-read only as required; no nested agents or peer review.
- Value: medium — a tiny policy helper removes exact duplicate error mapping at a high-friction reconnect boundary without changing reconnect semantics.
- Dependencies: none.
