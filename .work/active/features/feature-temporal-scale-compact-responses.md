---
id: feature-temporal-scale-compact-responses
kind: feature
stage: drafting
tags: [agent-ux, visual, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Compact temporal responses at every-frame scale

## Brief

GitHub issue #14 finding #7: a roughly 90-second streamed interaction under
every-frame capture retained 5,029 frames. The expanded temporal response
enumerated thousands of identifiers and exceeded a practical tool-output
budget, and difference-map analysis degraded to a 59-frame uniform sample with
`resource_limit_exceeded`. Long windows need a compact response mode that
summarizes epochs, gaps, keyframes, and selected differences without
enumerating every retained frame — preserving drill-down authority through
paging and canonical resources rather than inline enumeration
(canonical-result-projection).

- Interaction: `bbbc0f3d-6b9b-41dd-851c-c74b5d66cacb`.
- Range: `2dfc582d-aafd-41aa-97c8-6dc96999f4d7`.

Also from finding #10: a bundle requested immediately after an action marked
the tail partial because the requested post-action interval had not yet
elapsed. Distinguish "future interval not elapsed" from actual evidence loss in
partial-tail reporting (bounded-loss-accounting: every omission states exactly
why).

## Related backlog

The `perf-scout-*` backlog items target raw artifact-pipeline performance
(decode, accumulators, fanout, caching). This feature is about response
*shape* — summarization and truthful degradation — not pipeline throughput.
Cross-reference during design; do not merge. Pipeline perf may independently
raise the point at which sampling degradation kicks in.

## Simplification opportunity

Uniform-sample degradation under `resource_limit_exceeded` may become
unnecessary for summary-level questions once a keyframe/change-aware compact
summary exists; design should check whether the degraded path can be retired
or narrowed rather than kept alongside the new mode.

## References

- GitHub issue #14, findings 7 and 10 (partial-tail clarity).
