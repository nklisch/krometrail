---
id: feature-query-node-limit-large-pages
kind: feature
stage: drafting
tags: [bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Query and semantic-wait availability on large pages

## Brief

`query_page` (every query kind) and semantic `wait` fail hard on any document
whose accessibility tree exceeds the snapshot node bound, while `snapshot_page`
on the identical page succeeds with explicit omission accounting. Live repro on
v1.5.1 (https://en.wikipedia.org/wiki/Software_bug): plain role query →
"accessibility nodes: 6060 exceeds limit 5000"; the same query with
`container_text` → "7687 exceeds limit 5000" (container eligibility acquires
more of the tree); semantic wait → same failure. Large Wikipedia articles,
documentation sites, and dense dashboards are entirely outside the ergonomic
targeting path and the new full-tree semantic wait.

Mechanics (verified in source):

- `crates/krometrail-cdp/src/control/snapshot.rs:17` — `MAX_SNAPSHOT_NODES:
  usize = 5_000`, a bare constant with no derivation. The independent resource
  bound `MAX_SNAPSHOT_TEXT_BYTES = 1 MiB` (line 18) is what actually limits
  retained text memory.
- The snapshot builder (around line 2464) counts overflow into
  `omitted_node_count` and succeeds — snapshots tolerate omission by design.
- `active_for_query` (around line 642) refuses to answer whenever
  `omitted_node_count != 0`, which is what turns the builder cap into a hard
  query/wait failure. The error carries recovery text ("narrow the semantic
  query to a smaller document") but a descendant `scope` does not reduce
  acquisition, so the advertised recovery does not actually help today.
- Geometry decode has its own bounded fallbacks at the same constant
  (viewport-scoped selection, `geometry_omitted`), lines ~1156-1180, ~1456.

## Strategic decisions

- **Raise the node bound substantially rather than build partial-query
  machinery**: user direction ("shouldn't we just allow a lot more nodes —
  the node choice was arbitrary"). The 5,000 value is arbitrary; the memory
  story is governed by the text-byte cap. Design should pick a much larger
  node bound justified by measured per-node cost, keep a fail-closed refusal
  only for truly pathological trees, and keep snapshot omission semantics
  unchanged.
- Queries remain exact on complete snapshots: no silently-partial query
  results. If the (much larger) bound is still exceeded, the refusal stays
  explicit — but its recovery guidance must name actions that work.

## Simplification opportunity

The query-refusal path and its recovery text can likely be simplified once the
bound is realistic: if the raised bound covers real-world pages, the
"narrow the semantic query" recovery prose (which today names an action that
does not reduce acquisition) should be corrected or removed rather than
elaborated.
