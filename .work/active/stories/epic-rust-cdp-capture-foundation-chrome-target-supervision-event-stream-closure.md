---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-event-stream-closure
kind: story
stage: done
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-bounded-reconstruction]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Close browser session event streams after Ended

## Origin

Adversarial feature review confirmed each receiver retains an `Arc<Subscriber>` that owns a sender, so publishing `Ended` never permits `next()` to return `None`.

## Scope

Separate subscriber lag/revision state from registry-owned sender ownership. Publish the terminal `Ended` event exactly once, then explicitly close and release every fan-out sender. Existing receivers may drain queued events and then must return `None`; lag errors remain bounded and do not keep senders alive. New subscriptions after terminal state must return a stream that closes consistently rather than creating immortal channels.

## Acceptance criteria

- [x] Every stop/failure/exhaustion path yields one `Ended`, then stream exhaustion (`next() == None`) within a bounded timeout.
- [x] Receiver-owned state cannot retain a sender; registry cleanup releases closed subscribers.
- [x] Slow-subscriber lag behavior remains non-blocking and terminal closure wins deterministically.
- [x] Workspace/supervision/real Chrome tests and clippy pass; no resource or screencast leakage.

## Implementation notes

- Split receiver lag/revision/terminal state into `SubscriberState`; only `SubscriberRegistry` retains bounded-channel senders. Dropping a receiver therefore closes and removes its registry entry without an `Arc` self-retention path.
- Terminal publication is idempotent. The registry stores one `Ended` event per existing subscriber, drops all registry senders, and lets each receiver drain queued non-terminal events before returning `Ended` and then `None`. A full slow queue cannot lose the terminal event, and post-terminal subscriptions close immediately.
- `finish_state` now guards the terminal transition so stop, process death, reconnect exhaustion, and cancellation cannot publish duplicate `Ended` events. Added bounded unit coverage for lag ordering, full subscribers, terminal idempotence, and post-terminal subscriptions; production stop coverage drains to `Ended` and `None`.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo check --workspace --no-default-features --all-targets --locked`
- `cargo test -p krometrail-cdp --features cdp-spike --all-targets --locked`
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test session_supervision opt_in_real_chrome_reconnects_through_a_new_physical_proxy_connection --locked -- --nocapture`
- [x] All deterministic, workspace, feature, clippy, and real reconnect checks pass without blocking or resource leakage.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Focused review verified sender ownership is registry-only, terminal publication is exactly once, terminal delivery remains ordered after queued events even at capacity, and late subscriptions close. Verification covered stop-stream closure, full workspace/default and no-default checks, cdp-spike tests, clippy, and the opt-in real Chrome rotating-path reconnect test. Verdict: Approve - story verified by implement; fast-lane advance.
