---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-wire-authenticity-remediation
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

# Make candidate routing, ordering, lifecycle, and drift evidence wire-authentic

## Origin

Phase 2 feature review found that `ScriptedCdpPeer` pushes and consumes its own expected-message deque while the actual cdpkit candidate talks to a separate `ScriptedCdpServer`. It also found static real-Chrome gate records for deterministic routing and protocol drift.

## Scope

Replace the parallel expected deque with one wire-connected scripted server/controller. Advance barriers only after observing candidate commands; emit event-before-response, detach while a command is pending, close the socket, and require a genuinely new connection/session rebuild. Derive candidate scenario outcomes from observed transport behavior. Compute real-Chrome routing counts from unique correlated commands/events. Either execute drift probes through the wire-connected candidate path and bind their trace to decisive evidence or represent candidate-contract drift evidence separately without calling it a real-Chrome measurement. Add a bounded burst/reader-drain scenario or narrow the RSS/queue claim to what the continuously drained run actually proves.

## Acceptance criteria

- [ ] Candidate lifecycle and ordering assertions derive from one observed wire script, not a parallel expected deque.
- [ ] Routing counts derive from unique correlated responses/events; drift is genuinely exercised or honestly represented as separate contract evidence.
- [ ] Hidden-queue/RSS claims match the experiment actually run; no static pass is presented as measured.
- [ ] Deterministic fake/candidate tests and denied-warning clippy pass; no production adapter or core-port change lands.
