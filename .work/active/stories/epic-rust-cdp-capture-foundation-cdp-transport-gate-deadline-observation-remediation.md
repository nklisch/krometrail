---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-deadline-observation-remediation
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: 1.0.0
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

- [x] Rebuild passes only when observed completion is within five seconds, with measured elapsed time in evidence.
- [x] Disconnect closure fields derive from observed pending-command and subscription termination within one second.
- [x] `hard_stop_seconds` bounds the complete gate and is covered by deterministic timeout tests.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production adapter or core-port change lands.

## Implementation notes

- Added strict observed deadline measurements: reconnect/session rebuild records wall-clock completion and is rejected at or beyond five seconds; disconnect records independent pending-command and subscription termination elapsed times, requires the command-start readiness event, and cross-checks both outcomes with the transport close reason.
- Plumbed `hard_stop_seconds` into `GateConfiguration` and wrapped the complete real-Chrome operation in a Tokio timeout. Chrome endpoint readiness is now asynchronous so the timeout can cover startup; zero hard stops are rejected.
- Added paused-time deterministic timeout tests and strict validation regressions for absent, nominal-only, non-finite, and over-threshold deadline evidence. The retained pre-remediation reports are documented as obsolete and left byte-for-byte unchanged.
- Verification: `cargo fmt --all`; workspace default tests/clippy; `cdp-spike` tests/clippy; `cdp-spike-cdpkit` tests/clippy. Only spike/evidence/schema/docs/test files changed; no production adapter, root, or core-port changes.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane remediation review verified complete hard-stop enforcement, observed pending-command and subscription termination, measured rebuild elapsed time, strict validator failures, deterministic paused-time coverage, 17 candidate-feature tests, and denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
