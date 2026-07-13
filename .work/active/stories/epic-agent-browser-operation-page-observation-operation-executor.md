---
id: epic-agent-browser-operation-page-observation-operation-executor
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-page-observation
depends_on: [epic-agent-browser-operation-page-observation-core-contracts]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Route page observations through the supervised session

## Checkpoint

Implement Unit 2 of the parent design after the core contracts exist. Add `krometrail-cdp::control::PageControl`, route `BrowserSessionPort::execute` through an `Execute` command on the existing single-writer production supervisor, and implement fresh current-page inspection plus bounded side-effect-free evaluation.

Every execute path—including normal running, reconnect delays/attempts, stop, terminal state, target closure, queue closure, malformed response, and command timeout—must answer the oneshot exactly once. Do not replay an operation after reconnect. Resolve the request's Krometrail target to the current attached `TransportSessionId` inside the actor; no transport handle, browser target key, or adapter type enters core.

Use the root-injected monotonic clock and existing session origin even without active capture. Inspection reads fresh URL/title/readiness/device scale, layout/visual/content metrics, and navigation history. Evaluation always requests by-value JSON with `throwOnSideEffect`, an adapter timeout, and a 1 MiB serialized result bound; undefined is distinct from null.

## Required files

- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/evaluation.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs`
- existing test fakes affected by deliberate port construction changes

## Acceptance evidence

- [ ] Inspect/evaluate commands use only the exact current flat session for the requested target and return target/attachment/session-time provenance.
- [ ] Reconnect/stop/terminal/missing-target paths fail promptly and actionably; no operation sender hangs, disappears, or auto-replays.
- [ ] Inspection rejects malformed/non-finite/incoherent runtime, layout, scale, readiness, and history responses rather than substituting defaults.
- [ ] Evaluation refuses side effects, remote-only/unserializable/oversized values, exceptions, and timeouts without exposing raw page stacks or transport details.
- [ ] Deterministic actor tests use barriers/events rather than wall-clock sleeps.

## Ordering

Depends on `epic-agent-browser-operation-page-observation-core-contracts`. It establishes the actor seam used by snapshot and screenshot checkpoints.
