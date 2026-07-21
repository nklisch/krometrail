---
id: feature-bounded-response-detail
kind: feature
stage: drafting
tags: [agent-ux, browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Bound `detail: full` response projections

## Brief

`detail: "full"` is unusable on real pages. During the seventh shakedown a
single `fill` on the Wikipedia "Temporal logic" article returned **827,188
characters** and exceeded the agent's token limit outright. The breakdown:

```
observation.snapshot   933,959 bytes   <- uncapped accessibility tree
observation.page           738
observation.semantic_outcomes  650
observation.screenshot     426
observation.context        191
```

The same call at `detail: "expanded"` is completely fine, because expanded
compacts the snapshot to a summary:

```json
"snapshot": {"available": {"generation": 3, "target_count": 160,
  "unchanged": true,
  "omissions": {"presentation_targets": 113, "source_nodes": 2215,
                "presentation_context_nodes": 4744}}}
```

So the compaction machinery already exists and is correct — `full` simply does
not apply any bound. The page was 12,796 px tall with 2,215 source nodes, which
is an ordinary encyclopedia article, not a pathological input.

This is a `canonical-result-projection` violation: `full` should be the most
complete *bounded* projection with explicit omission accounting, not an
unbounded dump. An agent cannot predict which pages will blow its context, so in
practice `full` is a footgun that must be avoided entirely — which wastes the
detail tier.

## Simplification opportunity

Do not add a fourth detail tier. Apply a bound plus omission accounting to the
existing `full` projection, reusing the accounting shape `expanded` already
emits. Per Current Contract Discipline the `full` response shape may change
directly.

Fold in if cohesive:
- `idea-temporal-context-clip-and-truncation-exactness` — exact truncation
  warnings rather than `len == limit` heuristics; distinguishing scanned
  collection-gap count from total matched count. Same "bounded output must
  report its own bounding truthfully" concern.

## Acceptance

- `detail: full` on a large real page returns a bounded response with explicit
  omission accounting rather than an unbounded snapshot.
- Truncation/omission reporting is exact, not inferred from `len == limit`.
- A regression test covers a large-page projection against an explicit ceiling.
