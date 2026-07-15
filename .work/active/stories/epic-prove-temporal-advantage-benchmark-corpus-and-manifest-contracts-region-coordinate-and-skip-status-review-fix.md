---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix
kind: story
stage: implementing
tags: [testing, visual]
parent: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-ci-boundary-and-samples]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Align benchmark ROIs and close skipped manifests

## Checkpoint

Resolve the benchmark-contract review findings that must land before deterministic scoring consumes
hidden region truth. Replace the ambiguous `affected_region` shape with one explicit fixed
viewport-pixel ROI contract, align every canonical value with the actual rendered fixture geometry,
and tighten skipped-run validation so an aggregate `Skipped` manifest cannot contain rows in any
other state.

This is a contract correction, not a scoring change. It owns the benchmark definition/schema,
fixture geometry assertions, prompt coordinate wording, and existing run-manifest validation. It
does not implement a scorer, add coordinate conversion heuristics, launch Chrome, or create a
second ROI/provenance format.

## ROI contract

The one current v1 meaning of every `affected_region` is:

- fixed captured viewport pixels at the canonical 800×450 benchmark geometry;
- top-left origin `(0,0)`;
- integer half-open bounds `[x,x+width) × [y,y+height)`;
- a visual region that contains the complete visible change relevant to the case;
- not CSS pixels, DOM/layout coordinates, canvas/logical coordinates, device-independent units,
  element geometry, or a region that follows an element through time.

Keep the current wire shape only if needed for the prepublic contract, but give `Rect` one
explicit fixed viewport-pixel meaning rather than leaving it semantically open. A separate
runtime conversion type is not required for this benchmark contract. Every rectangle must be
non-empty and fit within 800×450. The fixture geometry assertions must derive the values from the actual rendered
CSS/canvas placement in `temporal-benchmark`, not from the old JavaScript logical constants alone.
The definition digest and generated schema/sample identities must change with the corrected values. The
existing wire shape may remain `Rect`; the semantics, validation, and fixture-geometry evidence are
what make the coordinate contract explicit.

The structured interpretation prompt must say that `affected_region` answers use this fixed
viewport-pixel space. The answer wire shape remains one current contract; do not add a caller
selectable coordinate-space field that would permit mixed semantics.

## Skipped manifest closure

Update `RunManifest::validate_outcome` and its contract tests so `status=Skipped` is valid only
when:

1. the run is the explicitly optional Linux Chromium configuration;
2. the top-level failure is `OptionalUnavailable`;
3. the browser availability is the optional Chromium skip state; and
4. every row exists, has `status=Skipped`, and carries its own `OptionalUnavailable` failure with
   a concrete recovery action.

A mixed skipped manifest containing a passing, failing, inconclusive, or blocked row is invalid.
Do not reinterpret a row state as an aggregate shortcut, and do not broaden `Skipped` to required
platforms or model lanes.

## Acceptance evidence

- [ ] The definition and schema expose one explicit fixed viewport-pixel ROI contract; all 13
      canonical regions are measured against actual fixture geometry, fit 800×450, and no longer
      depend on ambiguous JS logical coordinates.
- [ ] Fixture/contract tests prove the ROI origin, half-open bounds, non-empty/within-viewport
      invariants, and representative rendered geometry for movement, flicker, layout, and
      DOM-opaque cases.
- [ ] The model-facing prompt documents fixed viewport-pixel coordinates without exposing family,
      case, variant, or expected answer metadata.
- [ ] A skipped manifest with every row skipped remains valid; any mixed row-status variant is
      rejected and preserves row-level optional-unavailability reasons/recovery actions.
- [ ] Regenerated definition/schema/sample digests and clean-checkout comparisons pass; no live
      browser, network, model, paid work, product CLI, or generated VitePress documentation is
      involved.

## Ordering and handoff

This review-fix story must be `done` before
`epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-structured-scorer-and-ground-truth`
can implement or score region localization. The scoring feature consumes the corrected `Rect`
semantics and has no fallback for pre-fix interpretation.
