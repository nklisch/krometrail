---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-retention-recovery-and-performance
kind: story
stage: done
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-control-reliability-and-session-barriers]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Qualify retention, recovery, resources, and production latency

## Checkpoint

Exercise the concrete recording/retention/recovery and evidence services after capture/control
measurements exist. Use temporary data through the existing store and recovery implementation;
never introduce a benchmark-specific store, retention policy, artifact renderer, source reader, or
cache identity.

## Exact implementation

Add scenario code under `src/app/live_evaluation/retention.rs`,
`src/app/live_evaluation/recovery.rs`, `src/app/live_evaluation/resource_usage.rs`, and
`src/app/live_evaluation/latency.rs` (all test-only). Use one concrete `RecordingStore` per
sequential scenario, opened by the production `open_storage_with_budget` path, and expose only
its existing `RecordingSink`, `RetentionStore`, `FrameSource`, `TemporalQuery`,
`ProgressiveEvidenceStore`, and artifact/bundle ports.

Retention must verify bounded disk usage, pin a resolved source interval, preserve pinned frames,
evict unpinned frames, report declared gaps/availability, pause when all candidates are pinned,
resume after unpin, and clean artifact-linked data using the existing retention semantics.
Recovery must close after a controlled interruption and reopen through `recover`, then verify
trailing open-segment repair, corrupt/staged artifact treatment, frame/gap reconciliation, and
usage accounting. Record recovered/removed counts and explicit unavailable/recovery failures.
Do not write database or segment formats outside the existing authority merely to force an outcome.

Resource sampling records the qualification-process scope, sample count, RSS/CPU values, and
browser-child accounting if the platform adapter can provide it. A missing platform measurement
is `inconclusive` or `unavailable` with recovery, never a fabricated zero and never a new host
threshold. Keep raw process paths out of the manifest.

Latency uses the same source interval and concrete production ports. Measure a cold call and a
repeat warm-cache call for temporal bundle retrieval, and the existing storyboard/difference-map
artifact generation path. Include range duration, frame dimensions, cache disposition, output
hash/manifest identity, and elapsed values. Only the thresholds already in `docs/EVALUATION.md`
are decisive: cached temporal bundle below 1 s and uncached storyboard/difference-map below 5 s
for the declared two-second 1080p performance profile. Use a distinct 1920x1080 performance
profile over the same standalone fixture; do not label an 800x450 capture-fidelity measurement as
1080p. No other host-speed threshold is allowed.

All measurements must reference authority-returned source interval, retention, artifact, and cache
identities. The harness never re-renders or hand-authors an artifact to satisfy latency.

## Acceptance evidence

- [x] Scripted store tests verify pin/evict/pause/resume, recovery repair/reconciliation, resource
      unavailable handling, and latency cache-disposition accounting without Chrome.
- [x] Live retention/recovery scenarios use the existing store/recovery authority and record
      concrete bounded usage, pinning, eviction, repair, and reconciliation outcomes.
- [x] Resource metrics identify process scope and preserve unavailable reasons; no fabricated zero,
      local host threshold, or private path is emitted.
- [x] Uncached/warm query and artifact measurements use one source interval, existing cache
      identities, exact dimensions/range, and only the 1 s/5 s EVALUATION thresholds for a true
      two-second 1080p profile; aggregate cache state is identity-derived and preserves mixed
      generated/hit bundles.
- [x] A failed, unavailable, corrupt, or incomplete service result produces fail/inconclusive/
      blocked status as appropriate and cannot be promoted to pass by cleanup or serialization.
- [x] No model, paid service, remote endpoint, product CLI, second storage authority, or benchmark
      browser runtime is involved.

## Ordering

This child depends on control barriers because all measurements must be tied to stable interaction
and source interval identities. The final child consumes its measurements to assemble and verify
the operator-facing manifest.

## Implementation notes

- Execution capability: resumed inline over the inherited qualification composition; this is one
  cohesive store/recovery/resource/latency boundary and needed no separate worker.
- Review weight: standard parent-feature review; this child advanced directly to `done` after the
  final 1080p cold/warm latency regression and complete gates turned green.
- Files changed: `src/app/live_evaluation/{retention,recovery,resource_usage,latency}.rs`,
  `src/app/live_evaluation.rs`, `src/app.rs`, `Cargo.toml`, `Cargo.lock`,
  `crates/krometrail-store/Cargo.toml`, `crates/krometrail-store/src/lib.rs`, and
  `crates/krometrail-store/src/recording.rs`.
- Tests added or strengthened: concrete-store pin/evict/pause/resume/linked-artifact cleanup now
  requires a passing scripted scenario; recovery injects corrupt and staged artifact state through
  a feature-gated store fault seam and verifies reopen repair, frame/gap reconciliation, usage, and
  cleanup; resource tests require explicit inconclusive/unavailable evidence rather than zeros; and
  latency tests keep the exact two-second 1920x1080 profile separate from 800x450 capture data while
  asserting typed authority cache metadata, complete manifests, output dimensions, source IDs, exact
  uncached/warm dispositions, all-cold/all-warm/mixed classifier cases, and the first bundle's
  identity-level mixed state.
- Validation: retention uses the production `RecordingStore` ports and authority-returned interval;
  recovery reopens through `open_storage_with_budget` and keeps artifact paths private; resource
  metrics retain process scope and unavailable reasons; latency verifies authority cache identities,
  uncached generated/warm hit dispositions, source interval identity, output hashes/manifests, and
  only the EVALUATION cached-bundle `<1 s` and uncached-artifact `<5 s` limits. Aggregate labels are
  derived from exact artifact dispositions, so the first bundle is reported as `mixed`, not `cold`
  merely because it is the first invocation.
- Simplification: no benchmark store, renderer, cache, database/segment format, browser runtime,
  CLI, remote endpoint, model lane, host-speed threshold, or hand-formatted cache-key projection was
  added. Latency observations retain the production typed cache metadata and manifest directly;
  the store fault seam is feature-gated and only exercises the existing recovery authority without
  exposing private paths.
- Discrepancies from design: the store received the narrow feature-gated artifact fault-injection
  seam required to test corrupt/staged recovery without duplicating or exposing storage authority;
  default product behavior is unchanged.
- Adjacent issues parked: none.

## Verification evidence

- Cache-state correction: the direct storyboard/difference-map call intentionally warms the shared
  production artifact cache before the bundle call. The bundle then adds its authority-returned
  marker/orientation parameters, so its first call legitimately reports generated storyboard and
  orientation outputs but a hit for the shared difference map. `LatencySample.cache` now reports
  `Mixed` from those exact per-artifact dispositions; `Cold` no longer means merely first call.
  No identity check or decisive threshold was weakened.
- Latency identity accounting preserves authority-typed cache metadata, cache key, complete
  manifest/output hash, artifact ID, source-frame IDs, and output dimensions, and verifies the
  returned handle against the store authority. The regression proves direct uncached generation /
  warm hits, the bundle's mixed first-call dispositions, and all bundle warm hits for one exact
  authority-returned 1920x1080 two-second range.
- Final focused scenarios: latency 1 passed; retention pin/evict/pause/resume 1 passed; retention
  linked-cleanup 1 passed; recovery repair/reconciliation 1 passed; unavailable-resource handling
  1 passed; platform resource status 1 passed.
- Final Rust 1.85.0 locked gates: fmt clean; default workspace check passed; default workspace test
  701 passed, 1 ignored across 60 suites; default workspace Clippy passed with `-D warnings`;
  qualification-support workspace check passed; qualification-support workspace test 710 passed,
  2 ignored across 60 suites; qualification-support workspace Clippy passed with `-D warnings`;
  qualification-support CDP check passed; CDP test 214 passed across 20 suites; CDP Clippy passed
  with `-D warnings`.
- No live environment variables were enabled, ignored live tests were not invoked, and Chrome was
  not launched. `.work/bin/work-view` remains an intentional user modification and was not
  checked out, overwritten, staged, or committed.
