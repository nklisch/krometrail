---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-condition-packaging-and-source-interval
kind: story
stage: done
tags: [testing, visual]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix]
release_binding: 1.0.0
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

## Implementation notes
- Execution capability: feature-owning Luna worker, direct-read and single-write-set implementation; the child checkpoint was kept cohesive because interval identity and A–E validation share one browser-agnostic boundary.
- Review weight: standard by default; child-story checkpoints do not enter review, so green verification advanced this item directly to `done`.
- Files changed: `crates/temporal-evaluation/src/interval.rs`, `crates/temporal-evaluation/src/packaging.rs`, `crates/temporal-evaluation/src/lib.rs`, `crates/temporal-evaluation/tests/conditions.rs`.
- Tests added: deterministic interval canonicalization, clock/order/gap/retention rejection, integer B slot selection and insufficient-retention failure, authority/version/cache/output validation for C–E, progressive request and filmstrip budgets, unavailable evidence preservation, shared interval identity, non-claim/privacy checks, and canonical package round trips.
- Simplification: reused the existing condition, artifact-kind, named-version, evidence-availability, retention, canonical JSON, SHA-256, and privacy contracts; no cache implementation, provenance manifest copy, decoder, renderer, reader, scorer, or runtime dependency was added.
- Discrepancies from design: `ArtifactEvidenceReference` also retains the authority manifest resolved range so C/D/E constructors can reject mixed-range projections rather than trusting an uncheckable handle; `NonClaimId` is defined once in this package boundary for later result records to reuse.
- Adjacent issues parked: none.

## Verification evidence
- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` — 675 passed, 1 ignored
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- No browser, network, model, paid-agent, product CLI, generated documentation, or `target/temporal-evaluation/` output was used.
