---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-deadline-observation-remediation
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

# Enforce and measure transport lifecycle deadlines

## Origin

Phase 2 feature review found that reconnect records a literal five-second deadline without timing or enforcing it, disconnect claims subscription closure while only awaiting one pending command, and the CLI discards `hard_stop_seconds`.

## Scope

Enforce a five-second timeout around the complete reconnect/session rebuild and record observed elapsed time. During disconnect, hold and await both a real subscription and pending command and derive closure fields from their observed termination. Enforce the CLI hard stop around the complete real-Chrome qualification operation. Validation must reject absent, nominal-only, non-finite, or over-threshold measurements.

## Acceptance criteria

- [ ] Rebuild passes only when observed completion is within five seconds, with measured elapsed time in evidence.
- [ ] Disconnect closure fields derive from observed pending-command and subscription termination within one second.
- [ ] `hard_stop_seconds` bounds the complete gate and is covered by deterministic timeout tests.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production adapter or core-port change lands.
