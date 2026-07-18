---
id: gate-cruft-remove-tautological-marker-cap-assertion
created: 2026-07-17
updated: 2026-07-17
tags: [cleanup, testing]
gate_origin: cruft
release_binding: null
---

# Remove a tautological marker-cap assertion

## Confidence

Low

## Category

Low-value test

## Location

`src/debug_bundle/markers.rs:978`

## Evidence

One marker test asserts both `assembled.markers.len() == 1` and `assembled.markers.len() <= MAX_BUNDLE_ARTIFACT_MARKERS`; the inequality adds no confidence. The actual 256-marker cap already has direct boundary coverage in the same module.

## Removal

Delete the redundant inequality assertion and remove “caps” from the test name/comment when this ambient cleanup is next touched.
