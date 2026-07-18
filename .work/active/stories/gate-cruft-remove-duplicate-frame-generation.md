---
id: gate-cruft-remove-duplicate-frame-generation
kind: story
stage: done
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

## Acceptance

- `PageFrameReference` retains target, attachment, and opaque loader-derived frame-key authority without a second generation field.
- Generated request schemas and installed skill/docs contain no `frame_generation` property or recovery guidance.
- Frame navigation and target reattachment still reject stale references through loader-key and attachment-generation checks.

## Test notes

Run core context/schema tests, CDP frame qualification tests, and repository-wide searches for the removed field.

## Implementation notes

- Execution capability: focused inline cleanup; one redundant public field and its adapter-only mirror check.
- Review weight: bounded standalone-story review, per gate-bundle caller.
- Files changed: core browser context contract and CDP frame context construction/validation.
- Tests: core context tests and CDP context tests pass; repository search contains no `frame_generation` reference.
- Simplification: attachment generation remains the target-reattach fence and the opaque loader-derived frame key remains the navigation fence, eliminating the duplicated arithmetic generation.
- Discrepancies from design: no generated schema, skill, or foundation-doc occurrence existed to remove.
- Adjacent issues parked: none.

## Bounded inline review — 2026-07-18

- Verdict: approved. The removed value carried no authority beyond `attachment_generation + 1`; current attachment and loader identity remain revalidated on every frame dereference.
