---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-event-stream-closure
kind: story
stage: implementing
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

- [ ] Every stop/failure/exhaustion path yields one `Ended`, then stream exhaustion (`next() == None`) within a bounded timeout.
- [ ] Receiver-owned state cannot retain a sender; registry cleanup releases closed subscribers.
- [ ] Slow-subscriber lag behavior remains non-blocking and terminal closure wins deterministically.
- [ ] Workspace/supervision/real Chrome tests and clippy pass; no resource or screencast leakage.
