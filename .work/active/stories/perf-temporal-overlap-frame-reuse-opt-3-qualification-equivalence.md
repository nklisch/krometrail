---
id: perf-temporal-overlap-frame-reuse-opt-3-qualification-equivalence
kind: story
stage: implementing
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: [perf-temporal-overlap-frame-reuse-opt-2-scheduler-service-integration]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Qualify overlap reuse and make the implementation decision

## Scope

Run the parent feature's ignored Rust 1.85 release benchmark after the shared
work integration and decide whether the mechanism earns its complexity. This
story is a hard qualification checkpoint, not permission to relax thresholds.

## Benchmark matrix

Run concurrent adjacent windows and sequential one-frame sliding controls at
30/60/120 frames with one and two request permits. Collect repeated wall and
process CPU samples, allocation bytes, RSS/HWM, scheduler reservations,
intermediate hit/decode/normalize counters, artifact-cache dispositions, and
capture-headroom proxies. Run `perf stat` for task-clock, cycles, instructions,
cache misses, and branch misses where permitted; record permission denial
verbatim.

## Exact equivalence

For the same deterministic source, IDs, request windows, and generator slots,
compare cache-disabled and reuse-enabled output by window and artifact kind.
Require byte equality for normalized buffers, manifest JSON, PNG bytes, output
hash, source frame IDs/order, visual epoch, normalization/mask/measurement
provenance, and combined artifact hash. Exercise clean, masked, gapped,
alpha/crop/scale, tie-timestamp, deletion, corruption, invalidation, and
cancellation cases.

## Acceptance decision

The two-permit 120 overlap result must achieve all of:

- wall <= 2,986.643 ms (25% below 3,982.190 ms);
- CPU <= 2,773.898 ms (30% below 3,962.712 ms);
- allocations <= 3,584,253,236 bytes (30% below 5,120,361,765 bytes);
- peak RSS <= 1,505,412 KiB (8% above the 1,393,900 KiB single-request
  reference);
- no capture-headroom, deletion, cancellation, or equivalence failure.

If any material condition fails, record the measured reason in the parent,
remove/reject the optimization stories as appropriate, and retain the simpler
no-intermediate-cache behavior and benchmark scaffold. Do not ship a cache that
wins CPU by starving capture or retaining deleted session pixels.
