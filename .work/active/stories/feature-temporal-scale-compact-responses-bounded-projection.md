---
id: feature-temporal-scale-compact-responses-bounded-projection
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: feature-temporal-scale-compact-responses
depends_on: [feature-temporal-scale-compact-responses-not-yet-elapsed-tail, feature-temporal-scale-compact-responses-epoch-capture-summary]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Bounded resolved-range and manifest projection

## Checkpoint

Design Unit 3 of `feature-temporal-scale-compact-responses` (issue #14 finding
#7): no detail tier enumerates unbounded identifier vectors; every omission has
exact accounting and named drill-down.

- `bounded_resolved_range(range, detail)` in
  `crates/krometrail-mcp/src/response.rs` becomes the single range-projection
  entry point (concise = existing `CompactResolvedRange`; expanded = compact +
  first/last frame + bounded per-kind event-id lists + exact omitted counts +
  drill-down block; full = expanded + frame-id head slice capped at
  `MAX_FULL_RANGE_FRAME_IDS` with exact `omitted_frame_id_count` and
  `list_source_frames` offset). Replaces every direct
  `serde_json::to_value(&range)` site, including the expanded/full bundle
  arms, `map_temporal_range_resolution_result`, `ListSourceFrames`, and the
  generation results.
- `generate_artifacts`/filmstrip: compact artifact handles at expanded; bounded
  inline manifests at full via `bounded_manifest_value` (canonical manifest
  resource stays complete).
- Capture-quality projection gains bounded `epochs` with exact
  `omitted_epoch_count`.
- Sampling reclassification per the feature's recorded design decision: retire
  `analysis_sampling_warning`/`add_analysis_sampling_warnings`; add
  `sampling_mode` to `BundleArtifactHandle`; Exhaustive over-limit failures
  keep `resource_limit_exceeded`.
- Doc roll-forward in the same stride: SPEC "Temporal Queries" bounded-full
  wording, ARCHITECTURE "MCP Boundary" bundle paragraph, VISUAL-EVIDENCE
  "Progressive Detail" line.
- Ordering hazard from the design: `compact_temporal_context_value`
  round-trips the serialized range — bound only at final presentation.

## Acceptance evidence

- Synthetic 1,000-frame range: expanded projection has no frame-id array beyond
  first/last; full has at most the cap plus exact omitted count and drill-down
  offset; expanded bundle `range` is bounded.
- Full `generate_artifacts` inlines bounded manifests while the manifest
  resource returns complete provenance.
- UniformBounded difference-map success carries structured sampling accounting
  at every tier with no `resource_limit_exceeded` degradation warning;
  Exhaustive over-limit still fails with `resource_limit_exceeded`.

## Ordering constraints

Depends on both sibling stories: presents the epoch summary and lands after the
wire-schema change.
