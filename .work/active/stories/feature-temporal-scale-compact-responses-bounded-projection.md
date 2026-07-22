---
id: feature-temporal-scale-compact-responses-bounded-projection
kind: story
stage: done
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

## Implementation

Implemented 2026-07-21; full gate green (fmt, wire-enum schema check, check,
test, clippy `-D warnings`); `bun run docs:build` re-run (the public
llms-full bundle does not include foundation docs, so it is unchanged).

- `bounded_resolved_range(range, detail)` in
  `crates/krometrail-mcp/src/response.rs` is the single range-projection
  entry point. Concise = `CompactResolvedRange` (counts only, now including
  `applied_interaction_window`). Expanded = compact fields + resolved anchor
  + first/last frame id + per-kind event-id `BoundedIds {ids, omitted_count}`
  capped at 32 + inline gaps/retention warnings + a `drill_down` block naming
  `list_source_frames` offset paging and the range handle. Full = expanded
  with 128-per-kind event ids + a `frame_ids` head slice capped at 256 with
  exact `omitted_count` and `drill_down.next_offset`.
- Replaced sites: `map_temporal_range_resolution_result` (plus epoch-bounded
  capture quality), `ListSourceFrames`, `GenerateArtifacts` and
  `GenerateRegionFilmstrip` (one `projected_generation_value` for all tiers:
  bounded range, bounded epoch rows without per-epoch frame-id vectors,
  compact handles at expanded, bounded inline manifests at full), the
  expanded and full bundle arms (one `bounded_bundle_value` covering
  `range`, `artifacts.range`, `artifacts.epochs`, outcome artifacts, and
  `context.range`/`context.capture_quality`), and
  `project_temporal_value` for expanded/full temporal-context responses
  (`query_browser_events`), applied only at final presentation after the
  concise round-trip hazard point.
- `bounded_manifest_value` presents the manifest with
  `source/analyzed/selected_frame_ids` (and `analysis_sampling.
  analyzed_source_indices`) capped at 256, an `omitted_id_counts` object with
  exact per-array omissions, and the canonical `manifest_uri`; the persisted
  manifest resource stays complete (server resource test still verifies
  byte-complete provenance).
- Sampling reclassification (receiver-accepted behavior reduction):
  `analysis_sampling_warning`/`add_analysis_sampling_warnings` removed —
  successful UniformBounded analyses no longer carry a
  `resource_limit_exceeded` warning. `BundleArtifactHandle` gained
  `sampling_mode` (from the manifest `analysis_sampling` disclosure);
  analyzed/source counts were already present. Exhaustive over-limit keeps
  its hard `resource_limit_exceeded` failure in the artifact service
  (`src/artifacts` tests unchanged).
- Epoch presentation bounded everywhere capture quality is presented:
  8 rows at concise, 32 at expanded/full, always with exact
  `omitted_epoch_count`.
- Scope additions beyond the design's named sites (same principle, logged):
  `map_temporal_video_result` presents its range through the expanded-tier
  bounded projection (it has no detail parameter and is already a compact
  handle surface), and pin/unpin/query-pin-state responses bound the
  `expected_frame_ids` enumeration (32, 256 at full) with
  `omitted_expected_frame_id_count` — a pinned 5k-frame range would
  otherwise re-enumerate every frame id in the pin response.
- Presentation shape note: `generate_artifacts` expanded/full now use the
  same `available`/`unavailable` outcome wrappers as concise (one
  presentation per tool); bundle outcomes keep the domain outcome shape with
  the artifact value swapped per tier, as before.
- Docs rolled forward in-stride: SPEC "Temporal Queries" bounded-full
  wording plus structured-sampling sentence, ARCHITECTURE "MCP Boundary"
  bundle paragraph, VISUAL-EVIDENCE "Progressive Detail" line, and the
  plugin skill `references/evidence.md` "full preserves complete structures"
  line.
- Regression tests: expanded projection of a 1,000-frame synthetic range has
  no frame-id array and exact omissions; full caps the head at 256 with
  exact omitted count and continuation offset; expanded bundle range bounded
  (server end-to-end assertion); bounded manifest presentation with exact
  `omitted_id_counts`; sampled-analysis success carries structured
  accounting at every tier with no `resource_limit_exceeded` warning; epoch
  presentation bounded with exact accounting. The two retired
  sampling-warning tests were removed with the warning.
