---
id: feature-bounded-response-detail
kind: feature
stage: done
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

## Architectural choice

`full` becomes the widest tier of the *same* bounded projection, not a separate
raw-passthrough path.

`expanded` and `full` now share one function, `bounded_snapshot`, parameterized
by a `SnapshotBudget` derived from `ResponseDetail`. The only thing that varies
between the two tiers is four numbers. This is the shape the item asked for —
reuse the accounting `expanded` already emits — and it removes the structural
cause of the bug rather than the bug: `full` was unbounded because it was a
different code path that happened to have no ceiling, and any future tier added
as a fresh path would have repeated the mistake.

Rejected alternative: keeping the raw dump but capping it by byte count after
serialization. That produces a truncated tree with no omission accounting, which
is a worse contract than the one being replaced — an agent could not tell what it
was missing.

## Design decisions

- **Full ceilings are 4× expanded, uniformly.** Targets 48 → 192, target JSON
  12 KB → 48 KB, context nodes 96 → 384, total snapshot JSON 32 KB → 128 KB.
  A single ratio is easier to reason about than four independently tuned numbers,
  and 128 KB (~32k tokens) is a deliberate, predictable worst case for a tier an
  agent opts into — against the 933 KB that was actually observed.
- **`full` always materializes; it does not take the unchanged short-circuit.**
  The novelty optimization is correct for `concise` and `expanded`, where the
  caller is asking for an economical projection and "nothing changed" answers the
  question. `full` is the explicit "give me everything" tier: a caller that asks
  for maximum detail and receives a summary has had its request silently
  reinterpreted, with no way to force materialization. The bound — 128 KB plus
  omission accounting — is what makes `full` safe; the short-circuit is not part
  of the fix. `bounded_snapshot` therefore gates the unchanged return on
  `detail != Full`. Consistency across tiers is the weaker principle here.
- **Page assets were bounded too — deliberate scope extension.**
  `project_page_assets` had the identical violation: `Full` returned the raw
  inventory and skipped the `by_kind` aggregation and omission accounting
  entirely. It now uses full budgets (256 rows / 64 KB) and emits the same
  accounting as the other tiers. Leaving one unbounded projection sitting next to
  a bounded one in the same function family would be a trap for whoever hits it
  next — the reader would reasonably assume the tier is bounded everywhere, which
  is exactly the assumption that made this bug expensive to find the first time.
- **Struct names follow the concept.** `ExpandedSnapshot` /
  `ExpandedSnapshotOmissions` became `BoundedSnapshot` /
  `BoundedSnapshotOmissions`. Wire field names are unchanged.

## Implementation Units

- `crates/krometrail-mcp/src/response.rs`
  - `MAX_FULL_TARGETS`, `MAX_FULL_TARGET_JSON_BYTES`, `MAX_FULL_CONTEXT_NODES`,
    `MAX_FULL_SNAPSHOT_JSON_BYTES`, `MAX_FULL_ASSETS`,
    `MAX_FULL_ASSET_JSON_BYTES`.
  - `SnapshotBudget::for_detail`; `bounded_targets` takes `ResponseDetail`
    instead of a `concise: bool`.
  - `expanded_snapshot` → `bounded_snapshot(snapshot, detail, novelty,
    viewport)`; `project_root_snapshot` routes `Expanded | Full` to it.
  - `project_page_assets` drops its `Full` early return.

## Testing

- `response::tests::full_snapshot_of_a_large_page_stays_bounded_with_exact_
  omission_accounting` builds the measured page shape — 2215 source nodes, 160
  actionable targets, 4744 omitted context nodes — projects at `full`, and
  asserts the encoded snapshot is at or under `MAX_FULL_SNAPSHOT_JSON_BYTES`,
  that `presentation_targets` and `presentation_context_nodes` equal the exact
  difference between what existed and what was emitted, that `source_nodes`
  (never acquired) stays distinct from what the projection dropped, and that
  `full` is still strictly richer than `expanded` in targets, context, and bytes.
- `page_asset_detail_is_aggregated_and_progressively_bounded` extended to assert
  `full` stays inside its row and byte ceilings and still emits `by_kind` and
  both omission counters.
- The same test asserts the materialization rule directly: `full` at
  `SnapshotNovelty::Unchanged` returns byte-identical content to the novel
  projection, while `expanded` at `Unchanged` still returns the summary.
- `response_detail_grows_without_changing_authoritative_envelope` and the
  end-to-end `successful_mutation_roundtrip_preserves_semantics_across_response_
  projections` updated: both previously asserted `full` emits the raw `nodes`
  array. These were drifted assertions against a shape the item explicitly
  authorizes changing, not product bugs. The end-to-end test repeats an unchanged
  generation across two `full` calls and now asserts both materialize.

## Risks

- **`full` no longer returns the complete accessibility tree.** That is the
  intended change, and the omission accounting makes the loss visible, but any
  workflow that depended on the raw tree loses it. Per Current Contract
  Discipline there is no supported third-party consumer, so no shim was added.
- **128 KB is still large.** It is roughly 32k tokens. It is bounded and
  predictable, which was the acceptance criterion, but it is not small. Lowering
  it later is a constant change with no structural cost.
- **The truncation-exactness fold-in was not needed.** The `len == limit` checks
  in `response.rs` are loop breaks, not warning triggers; the omission counts are
  already computed as exact differences. Scanned-vs-matched separation
  (`matched_count`, `returned_count`, `scanned_count`, `collection_gaps`) already
  exists and is already exact in `crates/krometrail-core/src/timeline/
  context.rs`, outside this feature's file ownership. Nothing to fold in.
