---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-control-reliability-and-session-barriers
kind: story
stage: done
tags: [testing, infra]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-duration-capture-timing-and-movement]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify browser control reliability and observable barriers

## Checkpoint

Exercise the existing production browser-control operation families against the existing real
interaction fixtures and account for success only when the operation result and required post-
action observation agree. Add no alternate control protocol, fixture runtime, or silent retry
that could turn an unobserved action into a pass.

## Exact implementation

Add the control scenario registry and runner under `src/app/live_evaluation/control.rs` (test-only)
and reuse the current operation registry from `krometrail-core` plus the existing
`verified-interactions` and `waits-and-batches` fixture scenarios. The scenario registry may map
an operation to a fixture/action sequence, but must not re-enumerate or rename the operation
registry. Cover the existing navigation/target, snapshot/screenshot, click/fill/type/press,
select/hover/drag/scroll, dialog, upload, evaluate, wait, and batch families where their current
fixture supports them. An unsupported fixture capability is explicitly unavailable/inconclusive,
not a successful no-op.

Record `ControlAttempt` with scenario ID, operation identity, interaction ID, pre-observation
availability, operation outcome, post-observation availability, and safe failure code. Use the
existing interaction-evidence sink and timeline anchor. Define success as the production
operation's successful result plus the required post-action observation; transport acknowledgement
alone is insufficient. Aggregate exact attempts/successes/failures and the EVALUATION-defined
reliability rate without introducing a host-speed threshold.

Make the barrier protocol explicit in `src/app/live_evaluation/barriers.rs`: existing browser lock;
loopback server ready; managed launcher/target attached; viewport reported; page ready; structured
operation submitted; post-action observation present; fixture `running=false`/button-enabled
settle; capture sink boundary acknowledged; interval query complete. Use existing event/observation
handles and bounded `tokio::time::timeout` deadlines as safety limits, never fixed sleeps. On
transport loss, timeout, target replacement, or stale interaction, mark the attempt and affected
trial honestly, request the existing recovery path, and continue only after a new observable
readiness barrier.

Add no product command and do not use MCP as a shortcut. The runner stays inside the injected
composition and stores every anchor/result through production ports.

## Acceptance evidence

- [x] Scripted tests assert the complete barrier order and reject a shortcut that proceeds from
      transport acknowledgement without the required observation.
- [x] The control matrix derives operation identity from the existing registry and records every
      attempted scenario with exact success/failure accounting.
- [x] A missing fixture capability, stale reference, target replacement, timeout, and transport
      loss become explicit unavailable/inconclusive evidence with recovery, never a pass.
- [x] Repeated runs are deterministic in scenario order and canonical manifest serialization; no
      arbitrary sleep is required for readiness or settle.
- [x] Live execution remains gated by the feature-specific opt-in and uses the same production
      browser connector, interaction evidence, capture, and store authorities.
- [x] Ordinary qualification verification does not launch Chrome.

## Ordering

This child depends on duration capture because its barriers and control attempts must share the
same interaction/source interval identities. Retention, recovery, and performance depend on the
completed control/lifecycle accounting.

## Implementation notes

- Execution capability: inline implementation over the existing qualification composition; the
  story is one cohesive test-only control/barrier boundary and did not need a separate worker.
- Review weight: standard parent-feature review; this child advanced directly to done after green
  verification and does not enter a child-story review lane.
- Files changed: `src/app/live_evaluation/control.rs`,
  `src/app/live_evaluation/barriers.rs`, `src/app/live_evaluation.rs`,
  `src/app/live_evaluation/capture.rs`, qualification-support fixture URL exports, and the root
  serde dependency/lock entry needed for canonical test-only records.
- Tests added: canonical operation-registry-derived scenario ordering; exact control attempt and
  reliability aggregation; transport-acknowledgement-without-observation rejection; safe failure
  classification for unsupported, stale, replacement, timeout, and transport loss; canonical
  attempt/run bytes; complete ordered barrier traces; out-of-order shortcut rejection; and
  bounded timeout behavior without sleeps.
- Verification: Rust 1.85 locked `cargo fmt --all -- --check`, workspace check/test/clippy, and
  qualification-support root/CDP test and clippy gates all passed. No live environment variables
  were set, ignored live tests were not invoked, and Chrome was not launched.
- Simplification: reused the production connector, interaction evidence sink, recording flush,
  timeline anchor, frame/gap/query authorities, existing fixture URLs, and the existing operation
  registry; added no MCP route, product command, alternate protocol, retry shortcut, or storage
  authority.
- Discrepancies from design: none.
- Adjacent issues parked: none.
