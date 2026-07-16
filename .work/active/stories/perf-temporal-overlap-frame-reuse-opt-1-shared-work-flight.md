---
id: perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight
kind: story
stage: done
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: []
release_binding: 1.0.0
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Add exact request-lifetime shared frame work flights

## Scope

Implement the bounded intermediate-work primitive designed by the parent
feature. Share immutable decoded RGBA8 and normalized linear RGB16 frame storage
only among overlapping generation requests that hold a live batch lease. Do not
add a durable cache, a TTL cache, pair-classification context, or artifact
publication changes in this story.

## Required identity

Decoded entries must include session/target, frame ID, capture ordinal, session
time, source format and image/viewport/device-scale metadata, exact encoded
SHA-256, visual epoch hash, decoder profile, and decoder algorithm version.
Normalized entries must additionally include effective crop/scale/background,
mask/region digest, normalization recipe and transfer-LUT versions, and the
normalization algorithm version. Final artifact measurement parameters remain
part of the existing artifact identity.

## Implementation notes

- Extend the internal single-flight area with typed decoded and normalized frame
  keys plus `InFlight`/`Ready`/`Failed` outcomes.
- Hold pixel payloads behind immutable `Arc` storage and release them with the
  last `WorkBatchLease`; weak registry cleanup must not retain completed
  sessions.
- Add the narrowest temporal-vision constructors/storage changes needed to
  assemble validated sequences without copying pixels. Preserve all existing
  accessors, dimensions, timestamps, gap ranges, masks, and provenance.
- Decode/normalize failures and cancellations are never cached. A waiter may
  cancel independently; the leader cancels only when no waiter remains.
- Add unit tests for every key field, order/epoch separation, exact normalized
  sequence assembly, duplicate leader suppression, waiter cancellation, and
  byte-account release.

## Verification

- Existing workspace tests and clippy pass.
- A focused test proves two overlapping keys with 119 shared source frames
  execute one decode/normalization operation per shared key while producing
  immutable equal pixels.
- A focused test proves entries disappear after the final batch lease and do
  not remain addressable for a later sequential request.

## Implementation notes

- Execution capability: inline feature-owner implementation; the primitive and its validation surface are cohesive, while service/scheduler integration remains explicitly deferred to the next child story.
- Review weight: standard default; child-story checkpoints do not enter review.
- Files changed: `src/artifacts/cache.rs`, `src/artifacts/single_flight.rs`, `crates/temporal-vision/src/frame.rs`, `crates/temporal-vision/src/sequence.rs`, `crates/temporal-vision/src/normalize.rs`, `crates/temporal-vision/src/lib.rs`, `crates/temporal-vision/src/geometry.rs`, `crates/krometrail-core/src/recording/frame.rs`.
- Tests added: exhaustive decoded/normalized key sensitivity, epoch/order separation, duplicate leader suppression, independent waiter cancellation, cancellation/failure non-cache, byte admission/release and weak cleanup, 119 shared decoded/normalized keys across overlapping batches, and exact Arc-backed normalized sequence reconstruction preserving pixels, dimensions, timestamps, gaps, masks, and normalization steps.
- Simplification: kept the existing final-artifact `SingleFlight` and publication path unchanged; the new registry is weak-only and request-batch scoped, with no durable, TTL, global warm, pair-context, scheduler, or publication behavior.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification evidence

- Rust 1.85.0 `cargo fmt --all -- --check` passed.
- Rust 1.85.0 `cargo check --workspace --all-targets --locked` passed.
- Rust 1.85.0 `cargo test --workspace --all-targets --locked` passed.
- Rust 1.85.0 `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
