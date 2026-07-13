---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-bounded-reconstruction
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-endpoint-rebinding]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Bound and cancel complete reconnect reconstruction

## Origin

Adversarial feature review confirmed that the attempt timeout covered only connection/setup, leaving serial per-target attachment/effects outside the deadline and unable to observe stop, cancellation, or managed process death.

## Scope

Treat reconnect as one transactional attempt: endpoint refresh, connection/setup, target snapshot, bounded target attachments/visibility/domain restoration, reducer reconstruction, and effect application must fit the attempt deadline and race against stop/cancel/process-death. Bound target count and attachment concurrency explicitly. Do not commit the new connection generation, state, pumps, or published restored events until complete reconstruction succeeds; discard partial work on timeout/cancel. Preserve finite backoff and exact-key identity.

## Acceptance criteria

- [ ] Entire reconstruction is deadline-bound and immediately observes stop/cancel/process death.
- [ ] Target restoration has explicit count/concurrency bounds and cannot grow attempt duration linearly without limit.
- [ ] Partial timeout/failure publishes no committed generation or false restored state; next attempt starts cleanly.
- [ ] Deterministic stalled-command/many-target/cancellation tests plus real reconnect tests pass leak-free.
