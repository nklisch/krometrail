---
id: perf-temporal-share-pair-classification-opt-3-service-scheduler-wiring
kind: story
stage: implementing
tags: [perf, visual, testing]
parent: perf-temporal-share-pair-classification
depends_on: [perf-temporal-share-pair-classification-opt-2-temporal-context]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Group Compatible Artifact Generators Under Existing Budgets

## Purpose

Wire the bounded temporal pair-analysis context into the production artifact
service without changing external artifact behavior, publication ordering,
cancellation, epoch isolation, or scheduler/capture headroom.

## Implementation units

### Unit 1: Group compatible pending slots

**Files**:

- `src/artifacts/service.rs`;
- `src/artifacts/generators.rs`;
- `src/artifacts/epoch.rs`.

Change `run_flight` grouping from `(epoch_index, generator_index)` to a
compatible analysis identity containing the exact epoch, normalized-sequence
identity, transformed mask/domain, measurement parameters, ordered frame
identity/timestamps, gaps, and algorithm/decoder identity. Storyboard,
orientation, difference-map, and compatible motion slots share one context;
different noise floors, masks, normalization recipes, epochs, or requests do
not. Display-only difference parameters share the core only after measurement
identity is proven equal.

Keep output IDs, result slots, publication sequence, cache keys, artifact
kinds, and partial-failure handling in their current deterministic order.
Orientation continues to consume the storyboard selection rather than measure
again. Single-output paths may keep the direct path when building a context
would add work.

### Unit 2: Extend scheduler accounting without a new cache

**Files**: `src/artifacts/scheduler.rs`, `src/artifacts/service.rs`.

Reserve the bounded trace plus exactly one working core per distinct requested
difference/motion consumer in the existing combined-request memory budget.
Reject before allocation with the existing resource-limit error. Do not count a
shared core once per output slot, do not create an unbounded semaphore, and do
not move any pixels into a global or persistent cache. Retain the existing
request, CPU, generator, decoded, normalized, output, deadline, and cancellation
permits.

### Unit 3: Preserve service failure fences and production evidence

Add/retain tests for epoch boundaries, equal timestamps, gaps, masks,
normalization identity, cache hits, source invalidation, session deletion,
flight cancellation, deadline expiry, partial failure, and deterministic
publication. Run the story-1 benchmark through the real
`TemporalVisionArtifactService`/`RecordingStore` production policy for cold
60/120-frame storyboard + orientation + difference outputs, with motion as a
separate cell.

## Acceptance criteria

- [ ] Compatible outputs make one adjacent traversal and report the expected
      pair-pass/classifier-call reduction; incompatible contexts never share.
- [ ] Stored artifact bytes, manifests, source IDs, hashes, selection roles,
      timestamps, gaps, masks, and cache dispositions are identical to a
      cache/context-disabled baseline.
- [ ] Cancellation and deadline errors cannot publish partial context results;
      last-waiter flight cancellation remains effective; session deletion and
      source revalidation fences remain effective.
- [ ] Scheduler memory reservations include the trace and distinct consumer
      cores, remain within configured limits, and preserve capture queue/gap
      headroom. No 100 MB-plus trace or persistent intermediate is introduced.
- [ ] After rebaselining the current normalization revision, cold E2E median is
      at least 20% faster at both 60 and 120 frames, or the parent feature
      records the measured miss and stops without adding lower-level or
      parallel work to this feature.
- [ ] Peak RSS is no more than 8% above the current 120-frame baseline and
      release stage/counter/CPU/allocation evidence is recorded.

## Non-goals

Do not modify normalization, Chrome/model execution, public artifact contracts,
`.work/bin/work-view`, persistent caches, GPU paths, or generator fan-out
parallelism. Do not absorb the concurrent normalization worker's changes.

## Dependency and handoff

Depends on `perf-temporal-share-pair-classification-opt-2-temporal-context`,
which depends on the release baseline/equivalence scaffold story. This is the
final implementation checkpoint before the parent feature review.
