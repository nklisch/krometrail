---
id: story-bound-auto-markers-to-resolved-range
kind: story
stage: review
tags: [bug, visual, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Bound automatic artifact markers to the resolved range

## Symptom

After an interaction-anchored temporal bundle produced valid retained frames but unavailable storyboards because its semantic anchor preceded the first frame, an exploratory plugin run retried `temporal_debug_bundle` using the returned first and last source-frame IDs with `RequireComplete` and `Reject`. The request failed with `invalid_input: artifact marker is outside the resolved range`, even though the caller supplied no markers. Diagnostic correlation: `43dab22b-0a81-467c-89c3-f07c1f80a354`.

## Root cause

Timeline observations were correctly selected inside the resolved range, but an interaction observation resolves to the interaction's authoritative dispatch time. That dispatch can precede an exact source-frame range. Marker assembly admitted the derived marker without rechecking its authoritative time, and downstream artifact-request validation rejected the whole bundle.

## Fix

Filter only automatically assembled candidates by their final authoritative session time. Preserve caller markers and natural anchor markers as mandatory inputs with their existing validation semantics.

## Regression

A bounded timeline observation inside the range that resolves to an interaction dispatch before the range must produce no automatic marker and no bundle-invalidating request.

## Implementation notes

- Automatic interaction, navigation, and generic candidates are admitted only when their final marker time lies inside the resolved range.
- Caller and natural-anchor marker handling is unchanged.
- Verified the failing regression and all marker/debug-bundle tests.
