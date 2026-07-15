---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-retention-recovery-and-performance
kind: story
stage: implementing
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-control-reliability-and-session-barriers]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
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

- [ ] Scripted store tests verify pin/evict/pause/resume, recovery repair/reconciliation, resource
      unavailable handling, and latency cache-disposition accounting without Chrome.
- [ ] Live retention/recovery scenarios use the existing store/recovery authority and record
      concrete bounded usage, pinning, eviction, repair, and reconciliation outcomes.
- [ ] Resource metrics identify process scope and preserve unavailable reasons; no fabricated zero,
      local host threshold, or private path is emitted.
- [ ] Cold/warm query and artifact measurements use one source interval, existing cache identities,
      exact dimensions/range, and only the 1 s/5 s EVALUATION thresholds for a true two-second
      1080p profile.
- [ ] A failed, unavailable, corrupt, or incomplete service result produces fail/inconclusive/
      blocked status as appropriate and cannot be promoted to pass by cleanup or serialization.
- [ ] No model, paid service, remote endpoint, product CLI, second storage authority, or benchmark
      browser runtime is involved.

## Ordering

This child depends on control barriers because all measurements must be tied to stable interaction
and source interval identities. The final child consumes its measurements to assemble and verify
the operator-facing manifest.
