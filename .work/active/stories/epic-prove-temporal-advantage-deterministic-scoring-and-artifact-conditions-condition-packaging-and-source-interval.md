---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-condition-packaging-and-source-interval
kind: story
stage: implementing
tags: [testing, visual]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Package five evidence conditions over one source interval

## Checkpoint

Create the browser-agnostic source-interval and condition-package contracts consumed by the
structured scorer and later live/manual lanes. This checkpoint depends on the benchmark-contract
review fix that aligns canonical ROIs to actual fixture geometry and closes skipped manifests.
It does not score answers, invoke visual algorithms, launch Chrome, or build a condition renderer.

## Source interval contract

Add `crates/temporal-evaluation/src/interval.rs` with an immutable `SourceInterval` containing:

- opaque session/target scope and interval ID;
- requested/resolved range and one anchor session time;
- ordered source-frame records with frame ID, capture ordinal, source time (optional), observed
  time, normalized session time, exact encoded SHA-256, and availability;
- ordered non-overlapping declared gap records with IDs, normalized time range, safe reason, and
  optional missing-frame estimate; and
- retention state plus a canonical digest over these exact identities.

Constructors reject duplicate IDs, decreasing ordinals, decreasing session time, inverted ranges,
unknown availability, non-canonical hashes, gaps outside the interval, overlapping gaps, and a
retention state that contradicts frame availability. They never infer a gap from ordinal arithmetic
or repair a missing frame. Source, observed, and session clocks remain separate.

## A–E package contract

Add `packaging.rs` with `ConditionPackage`, `EvidenceReference`, `ArtifactEvidenceReference`,
`ArtifactCacheIdentity`, `TemporalBundleEvidence`, `ProgressiveRetrievalRecord`, and the
`ConditionPackager` constructors described in the feature design. Every package records the one
source-interval digest, exact source IDs, gap IDs, retention state, package version, evidence
references, and fixed non-claims.

The five constructors are exact:

1. A references only the interval's final retained source frame and the bounded current-observation
   handle; it has no historical retrieval.
2. B selects exactly eight distinct source IDs by `floor(i * (n - 1) / 7)` in capture order. It
   fails explicitly below eight retained frames and creates no artifact or measurement.
3. C accepts authority-derived temporal-vision storyboard/orientation projections only, requiring
   `temporal-storyboard/1.1.0`, selected/source IDs, output/manifest hashes, declared gaps, and
   exact existing cache metadata.
4. D accepts the existing temporal debug-bundle projection only when its resolved range is the
   exact interval, preserving before/during/after, change-aware storyboard, difference map,
   capture summary, context, references, and unavailable outcomes.
5. E starts with D and records at most two existing source-frame requests of at most four frames
   each plus at most one existing fixed-region filmstrip. Requested, returned, and unavailable
   IDs remain ordered and explicit.

The package adapter receives projections from `ResolvedRange`, temporal-vision manifests and
traces, core artifact cache metadata, temporal debug bundles, progressive handles, and store
retention truth. It must reject hand-authored versions/cache keys and must not add a provenance
format, decoder, renderer, tracking rule, logical-element mapping, or second source reader.

## Acceptance evidence

- [ ] Repeated construction is byte/order-identical and every A–E package has the same interval digest and exact source set.
- [ ] Mixed range, source order, gap, availability, retention, manifest, cache, and output-hash mutations are rejected.
- [ ] B's uniform slots are integer-only, distinct, and observably independent of C's change-aware selection.
- [ ] C/D/E preserve existing authority order and explicit partial/unavailable evidence; E enforces both retrieval budgets and fixed-region semantics.
- [ ] Package bytes contain no image payloads, base64, paths, URLs, page text, ground truth, or raw answers.

## Ordering

This checkpoint unblocks the scorer. The following scorer story must also wait for the upstream
ROI/skipped-manifest review fix; it has no fallback for the old ambiguous region values.
