---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-capture-deadline-ack-semantics
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Correct capture deadline, cancellation, and acknowledgement semantics

## Origin

Second adversarial feature review found a frame-rate-derived loop cutoff instead of the configured hard stop, cancellation-unsafe Chrome/profile ownership during startup, and ack latency timing beginning before frame receipt.

## Scope

Make the configured operation deadline authoritative when minimum frames are not reached; every receive remains phase-bounded without an accidental 60-fps assumption. Establish Chrome process/profile ownership in a cancellation-safe guard immediately after spawn and prove timeout cancellation reaps the process/removes the profile. Measure receive-to-ack-completion only after a frame is returned, preserving acknowledgement before bounded handoff. Update evidence names/contracts/docs and deterministic tests accordingly.

## Acceptance criteria

- [ ] Slow capture may continue until the configured hard stop; no derived frame-rate deadline terminates it early.
- [ ] Startup/global timeout cancellation reliably kills Chrome and removes its temporary profile.
- [ ] Ack latency measures only post-receive acknowledgement completion and ack remains before `try_send`.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.
