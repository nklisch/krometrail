---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-wire-authenticity-remediation
kind: story
stage: done
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

- [x] Candidate lifecycle and ordering assertions derive from one observed wire script, not a parallel expected deque.
- [x] Routing counts derive from unique correlated responses/events; drift is genuinely exercised or honestly represented as separate contract evidence.
- [x] Hidden-queue/RSS claims match the experiment actually run; no static pass is presented as measured.
- [x] Deterministic fake/candidate tests and denied-warning clippy pass; no production adapter or core-port change lands.

## Implementation notes

- Replaced the disconnected expected-message deque with a loopback WebSocket `ScriptedCdpPeer` controller that records candidate commands, responses, events, and connection closes in one ordered trace. The controller drives event-before-response, detach-during-pending plus socket close, and a fresh connection with two rebuilt sessions.
- Candidate routing measurements now use unique correlated command/event tokens observed by the controller. Real-Chrome routing uses unique correlated command/event tokens from the candidate path rather than static counts.
- Added optional candidate-contract evidence with a SHA-256 trace binding. Unknown-event, additive-field, and unknown-enum fixtures are explicitly not described as real-Chrome measurements.
- Narrowed the RSS limitation to the continuously drained reader/counter proxy; the cdpkit unbounded subscriber queue remains an explicit unproven limitation. No final Linux/macOS evidence was recaptured or hand-edited.
- Verification: `cargo fmt --all --check`; workspace default tests; spike tests/clippy; candidate-feature tests/clippy all pass. No production adapter, root composition, or core-port files changed.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane remediation review verified one shared wire-observation controller, correlation-derived routing, trace-bound drift evidence, narrowed RSS claims, 14 candidate-feature tests, and denied-warning clippy. No production/core leakage. Verdict: Approve - story verified by implement; fast-lane advance.
