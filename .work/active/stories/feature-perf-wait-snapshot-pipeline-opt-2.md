---
id: feature-perf-wait-snapshot-pipeline-opt-2
kind: story
stage: done
tags: [perf]
parent: feature-perf-wait-snapshot-pipeline
depends_on: [feature-perf-wait-snapshot-pipeline-opt-1]
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# container_text ancestor memoization

Optimization 2 of the parent feature. `nearest_container_text_matches`
(`snapshot.rs:1015`) re-walks and re-normalizes shared ancestors once per
candidate with no memo (16.20 ms vs 0.93 ms floor at 50k). Depends on opt-1's
`normalized_match_text` so each ancestor is normalized at most once. See the
parent feature body (Optimization 2) for the verdict-function signature.

## Scope

- Add `ancestor_container_verdict(ancestor, needle, parents, semantic,
  nodes_by_id, memo, scratch)` memoized by ancestor `SnapshotNodeId`. The
  verdict "does the walk starting at this ancestor (inclusive) yield a
  container-text match" depends only on the ancestor chain and the fixed
  per-call needle, so it is identical for every descendant candidate.
- Reduce `nearest_container_text_matches(node, ...)` to: resolve the parent,
  return `ancestor_container_verdict(parent, ...)`. Encode the existing rules
  unchanged — nearest LOCAL container is the sole authority; a bounded
  (`MAX_GENERIC_CONTAINER_TEXT_BYTES`) matching GENERIC container qualifies;
  otherwise recurse upward.
- **Memo scope/lifetime**: allocated once at the top of `query` /
  `probe_presence`, one per query evaluation, keyed by ancestor
  `SnapshotNodeId`, dies with the call. No cross-poll caching, no invalidation
  surface. Shares the fold scratch from opt-1.

## Files

- `crates/krometrail-cdp/src/control/snapshot.rs` — `ancestor_container_verdict`,
  `nearest_container_text_matches`, memo/scratch creation in `query` /
  `probe_presence`.

## Acceptance criteria

- [ ] Each distinct ancestor's rendered text is matched at most once per query
      evaluation (counting instrumentation or `perf_` benchmark).
- [ ] Existing nearest-container / generic-container-bound / local-container
      authority tests pass unchanged.
- [ ] `perf_container_contains_50k` drops from 16.20 ms toward ~2–3 ms.

## Implementation notes

Implemented per-matcher ancestor verdict memoization with an iterative walk,
preserving nearest-local-container authority and generic-container bounds.
The release benchmark measured approximately 5.2 ms/op for the 50k container
query; existing authority and deep-chain tests remain green.
