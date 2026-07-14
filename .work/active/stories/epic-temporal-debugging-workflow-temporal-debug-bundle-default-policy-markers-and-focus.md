---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus
kind: story
stage: implementing
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-contracts-and-manifest-trace
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Materialize Default Evidence, Markers, and Focus Times

## Checkpoint

Implement the pure, versioned `temporal-debug-bundle-v1` policy: exact default generator requests, bounded privacy-safe marker assembly from existing timeline/interaction sources, and deterministic major-change focus extraction from the typed storyboard trace. This checkpoint adds no generation, event correlation, marker store, or bundle orchestration loop.

## Files

- `crates/krometrail-core/src/ports/{timeline.rs,range.rs,mod.rs}`
- `crates/krometrail-core/src/debug_bundle.rs`
- `crates/krometrail-store/src/index/{timeline.rs,interactions.rs}`
- `crates/krometrail-store/src/recording.rs`
- `src/debug_bundle/{mod.rs,policy.rs,markers.rs,focus.rs}` (new)
- focused tests beside these modules and `crates/krometrail-store/tests/sqlite_timeline.rs`

## Policy

- Storyboard: anchor from `ResolvedAnchor.effective_time`, 8 tiles, noise floor 512, no crop, black background, existing exact `FitLimits`, `1920 × 2048`, 16 MiB, and orientation included by default/omittable only through `OrientationPolicy`.
- Difference map: epoch-local first reference, normalized frequency, existing spectral palette, implicit range-quarter repeat separation, noise floor 512, same normalization, black canvas, `8192 × 8192`, 64 MiB. Use `AllowPartial`; request no motion history, filmstrip, region, comparison, or inferred output.
- Add bounded kind-filtered `TimelineStore::selected_range(TimelineRangeQuery) -> TimelineRangeSlice`; bundle uses only interaction/navigation/marker kinds with a 1024-row source cap, exact filtered `matched_count`, and explicit truncation.
- Preserve caller markers exactly (maximum 64, kind 64 UTF-8 bytes, label 160), interaction dispatch/operation identity, explicit navigation identity/time, and generic marker identity/time. Use exact caller presentation where supplied; otherwise emit the explicit non-sensitive `Marker <uuid>` fallback and warning.
- Preserve caller and resolved-anchor markers first, select remaining candidates by exact anchor distance/class/time/ID to the existing 256-marker artifact cap, then present chronologically.
- Extract at most 16 focus times from available storyboard summaries and selected major-change reasons only; deduplicate/tie-break as designed and sort chronologically for `TemporalContextRequest`.

## Acceptance evidence

- Default and orientation-omitted effective requests are byte-stable and contain only the approved existing artifact kinds/parameters/limits.
- Kind-filtered timeline SQL returns an exact matched count plus at most the requested rows, excludes browser-event rows, preserves generic timeline order, and requires no migration or alternate table.
- Interaction records/anchors, navigation observations, caller markers, and generic markers produce exact deterministic `ArtifactMarker` values with mandatory inclusion, equal-time order, and honest 64/256/1024 truncation.
- Labels contain only typed IDs and operation stable names; persisted secret, locator, page-text, URL, filename, key, and parameter sentinels do not appear.
- Focus extraction covers unchanged/change/gap, multi-epoch ties, summary/selection priority, dedup, 16-cap, and missing trace without calling `measure_*` or `select_*`.

## Ordering

Depends on the bundle/manifest trace contracts. It unblocks service composition by producing the exact artifact request, marker list, focus times, effective policy, and composition warnings. On green verification this child advances directly to `done`.
