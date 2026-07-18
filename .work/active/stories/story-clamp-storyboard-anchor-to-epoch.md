---
id: story-clamp-storyboard-anchor-to-epoch
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

# Clamp storyboard anchors to each visual epoch

## Symptom

An exploratory plugin run requested `temporal_debug_bundle` around a successful `navigate_page` interaction with a 250 ms before / 3000 ms after window. The resolved interval retained 24 ordered source frames with no gaps, but the natural interaction anchor (`179736803208`) preceded the first retained frame (`179993802291`). The bundle succeeded only partially: difference-map generation worked, while storyboard and before/during/after both became unavailable with `artifact_generation_failed: storyboard anchor must lie inside the source frame range`.

## Root cause

The artifact request validates a storyboard anchor against the overall resolved interval. Generation then partitions retained frames into visual epochs, but forwards that unchanged semantic anchor to every epoch. Temporal Vision requires the render anchor to lie within the concrete epoch's source-frame range, so a valid interval anchor can still fail against retained frames or any non-anchor epoch.

## Fix

Materialize an epoch-local storyboard anchor by clamping the requested anchor to the first/last retained frame time for that epoch before canonical cache parameters are computed. Keep the resolved range's semantic anchor unchanged for provenance.

## Regression

A storyboard request whose anchor is valid in the resolved interval but precedes the first retained frame must generate available storyboard evidence instead of degrading.

## Implementation notes

- Storyboard requests now materialize a per-epoch clamped anchor before canonical cache parameters and generation.
- Resolved-range provenance remains unchanged.
- Verified the failing regression, all artifact-service tests, and all debug-bundle tests.
