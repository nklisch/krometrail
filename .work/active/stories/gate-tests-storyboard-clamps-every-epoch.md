---
id: gate-tests-storyboard-clamps-every-epoch
kind: story
stage: done
tags: [testing, visual]
parent: null
depends_on: []
release_binding: 1.0.3
gate_origin: tests
created: 2026-07-17
updated: 2026-07-17
---

# Protect storyboard clamping on every visual epoch

## Priority

High

## Value evidence

Item: `story-clamp-storyboard-anchor-to-epoch`. A semantic anchor in one epoch must not degrade storyboard or orientation output in another epoch.

## Suggested test

Generate storyboard and before/during/after output over two visual epochs with `RequireAll`; assert both artifact kinds are available for both epochs while resolved semantic provenance remains unchanged.

## Implementation notes

- Execution capability: inline; one focused artifact-service regression.
- Review weight: standard by project default.
- Files changed: `src/artifacts/service_tests.rs`.
- Tests added: `RequireAll` storyboard plus before/during/after availability for both sides of a visual-epoch transition.
- Simplification: reused the existing two-epoch rig and artifact outcome contract.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Review (2026-07-17)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Bounded inline standalone-story review. Verified both output kinds, both visual epochs, strict failure policy, and unchanged semantic-range provenance.
