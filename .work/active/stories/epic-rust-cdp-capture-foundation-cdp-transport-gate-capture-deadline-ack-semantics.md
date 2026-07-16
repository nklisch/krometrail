---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-capture-deadline-ack-semantics
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

# Correct capture deadline, cancellation, and acknowledgement semantics

## Origin

Second adversarial feature review found a frame-rate-derived loop cutoff instead of the configured hard stop, cancellation-unsafe Chrome/profile ownership during startup, and ack latency timing beginning before frame receipt.

## Scope

Make the configured operation deadline authoritative when minimum frames are not reached; every receive remains phase-bounded without an accidental 60-fps assumption. Establish Chrome process/profile ownership in a cancellation-safe guard immediately after spawn and prove timeout cancellation reaps the process/removes the profile. Measure receive-to-ack-completion only after a frame is returned, preserving acknowledgement before bounded handoff. Update evidence names/contracts/docs and deterministic tests accordingly.

## Acceptance criteria

- [x] Slow capture may continue until the configured hard stop; no derived frame-rate deadline terminates it early.
- [x] Startup/global timeout cancellation reliably kills Chrome and removes its temporary profile.
- [x] Ack latency measures only post-receive acknowledgement completion and ack remains before `try_send`.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence hand edit lands.

## Implementation notes

- Execution capability: inline implementation; one spike-only ownership surface with deterministic tests and documentation/schema contract updates, with no production/core changes.
- Review weight: standard, caller explicitly requested the implementing-to-review boundary.
- Files changed: `crates/krometrail-cdp/src/spike/chrome_harness.rs`, `crates/krometrail-cdp/src/spike/evidence.rs`, `crates/krometrail-cdp/src/spike/scenarios.rs`, `crates/krometrail-cdp/tests/transport_contract.rs`, `.github/workflows/cdp-transport-gate.yml`, `docs/evidence/cdp-transport/v2/schema.json`, `docs/evidence/cdp-transport/v2/README.md`, `docs/ARCHITECTURE.md`, `docs/research/rust-cdp-transport-2026-07.md`, `.agents/skills/rust-cdp-transport/SKILL.md`, generated `docs/public/llms-full.txt`, and the parent feature contract.
- Tests added: deterministic paused-time timeout regression proves the startup process is reaped and the temporary profile is removed; source-order regression proves no frame-rate cutoff and ack timer placement; evidence-contract regression rejects retired elapsed measurement names.
- Verification: `cargo fmt --all -- --check`; default workspace tests/clippy; `cdp-spike` tests/clippy; `cdp-spike-cdpkit` tests/clippy; schema generation check; documentation build.
- Discrepancies from design: evidence measurement names were made explicit (`capture_elapsed_seconds` and `handoff_elapsed_seconds`) so observed elapsed work cannot be confused with configured thresholds; the generated schema description was refreshed without changing schema version or historical evidence files.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane timing review verified no frame-rate cutoff, global-limit authority, cancellation-safe process/profile guard coverage, timer placement after frame receipt, acknowledgement before handoff, explicit elapsed names, 30 candidate-feature tests, and denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
