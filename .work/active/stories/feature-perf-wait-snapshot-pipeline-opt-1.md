---
id: feature-perf-wait-snapshot-pipeline-opt-1
kind: story
stage: implementing
tags: [perf]
parent: feature-perf-wait-snapshot-pipeline
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Probe normalization + relaxed-rescan fusion

Optimization 1 of the parent feature — the highest-confidence,
transport-independent win. The steady state of a semantic wait is a no-match
text probe, which today allocates a fresh normalized `String` per candidate,
re-normalizes the needle per candidate, and runs the whole scan twice (primary
+ relaxed rescan): ~100k normalizations / ~50 MB transient allocation per poll
at 50k. See the parent feature body (Optimization 1) for full design, unit
signatures, and pre-mortem.

## Scope

- Add `normalized_match_text` to `SemanticNodeMetadata`, populated at decode as
  `normalize_semantic_text(rendered_text, /*case_sensitive=*/true)` from the
  already-bounded `rendered_text` (bound-then-normalize = today's `matches()`
  input, so semantics are byte-identical). `rendered_text` is retained
  unchanged (internal-only; feeds length accounting + `true_collapsed_text_bytes`).
- Add `NormalizedTextNeedle` (needle normalized once) with
  `SemanticTextMatch::normalized_needle()` and an allocation-free
  `matches_prenormalized(candidate, scratch)` in
  `crates/krometrail-core/src/browser/observation.rs`. Exact/case-sensitive
  paths never allocate; case-insensitive Contains uses one reused scratch
  buffer per probe.
- Fuse the relaxed-candidate rescan into the primary `probe_presence` pass (and
  the parallel `query` no-match diagnostics): one traversal, accumulating
  primary `match_count` and the capped relaxed count together. Preserve the
  `MAX_SEMANTIC_RELAXED_CANDIDATES` cap and the "only when `match_count == 0`"
  surfacing gate.
- Thread `NormalizedTextNeedle` + `&mut String` scratch through
  `semantic_query_matches` (Text / Role-name / Label).

## Files

- `crates/krometrail-cdp/src/control/snapshot.rs` — metadata field,
  `probe_presence` / `query` fusion, `semantic_query_matches` signature.
- `crates/krometrail-core/src/browser/observation.rs` — `NormalizedTextNeedle`,
  `normalized_needle`, `matches_prenormalized`.

## Acceptance criteria

- [ ] Equivalence test: `matches` and
      `normalized_needle().matches_prenormalized(normalize(candidate,true), _)`
      agree across a corpus of candidates/needles in both modes and both case
      flags (include zwsp / private-use / mixed-whitespace cases).
- [ ] `normalized_match_text == normalize_semantic_text(rendered_text, true)`
      asserted for zwsp / private-use inputs.
- [ ] `probe_presence` performs exactly one traversal of `snapshot.nodes` on the
      no-match path (counting test double or `perf_` benchmark instrumentation).
- [ ] Outcome / `match_count` / `relaxed_match_candidates` unchanged for existing
      probe and query tests.
- [ ] `perf_probe_text_miss_50k` drops from 55.19 ms toward low single-digit ms.
