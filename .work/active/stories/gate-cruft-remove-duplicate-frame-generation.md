---
id: gate-cruft-remove-duplicate-frame-generation
kind: story
stage: drafting
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Remove frame generation duplicated from attachment generation

## Confidence
Medium

## Category
redundant state and check

## Location
`crates/krometrail-cdp/src/control/contexts.rs:25`

## Evidence

Every frame reference derives `frame_generation` as `attachment_generation + 1`, and its only read verifies the same equation. Attachment invalidation remains enforced by `attachment_generation`; navigation invalidation remains loader-scoped.

## Removal

Remove `PageFrameReference.frame_generation`, construction/validation/schema/docs mentions, and tests that exercise only the duplicated value.
