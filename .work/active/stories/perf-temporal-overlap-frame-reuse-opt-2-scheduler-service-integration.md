---
id: perf-temporal-overlap-frame-reuse-opt-2-scheduler-service-integration
kind: story
stage: implementing
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: [perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Integrate shared work with artifact scheduling and lifecycle fences

## Scope

Integrate the shared frame-work flights into `TemporalVisionArtifactService`
without changing generated artifact semantics. The service must reuse exact
source-frame decode and normalization work across concurrent adjacent windows,
while the scheduler charges unique shared bytes once and keeps request-local
output/publication memory bounded.

## Implementation notes

- Pass one batch lease through `generate_inner` and `run_flight`; assemble each
  request's source and normalized sequences in source order from shared immutable
  frames.
- Split memory accounting into unique shared intermediate bytes and
  request-local output/temporary bytes. Enforce a byte-weighted reuse admission
  cap with a capture reserve; recompute when an entry cannot be retained rather
  than exceeding the cap.
- Keep the current one/two request permit and one generator permit behavior
  observable. Do not introduce nested unbounded worker pools or parallel decode.
- Revalidate exact source frames before selecting any intermediate hit. Final
  artifact lookup, source payload validation, artifact cache locking, and
  publication remain authoritative.
- Ensure deletion and invalidation cannot return stale pixels: source reads must
  reject deleted/evicted evidence, active work must observe cancellation, and
  publication must recheck the deletion fence. Drop all session entries after
  the final batch lease.
- Test source corruption, visual-epoch changes, mask/normalization changes,
  segment eviction, session deletion during decode/normalize/publication,
  cancellation of leaders and waiters, and concurrent publication races.

## Verification

- Current artifact service tests remain green, including deterministic
  manifests and existing final-artifact cache behavior.
- Two concurrent adjacent requests report one decode and normalization execution
  per shared frame key and no durable intermediate rows/files.
- One-permit and over-budget cells remain bounded and explicit rather than
  silently claiming reuse.
