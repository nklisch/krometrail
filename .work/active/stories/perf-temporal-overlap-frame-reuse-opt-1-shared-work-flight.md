---
id: perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight
kind: story
stage: implementing
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: []
release_binding: null
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
