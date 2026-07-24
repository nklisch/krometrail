---
id: story-query-nonactionable-hint
kind: story
stage: done
tags: [mcp]
parent: null
depends_on: []
release_binding: 1.6.2
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Non-actionable match hint on query_page no_match

## Brief

`query_page` returns only actionable references, so a text/name query whose
matches are all non-actionable reports a plain `no_match` with
`omitted_match_count: 0`. In the 2026-07-23 v1.6.1 shakedown, "Graydon Hoare"
existed in the full AX tree (semantic wait found 9 matches, all plain
StaticText) while `query_page {kind: text}` reported bare `no_match` — an
agent reading only that result would wrongly conclude the text is absent from
the page. Container queries already solve this shape with
`uncontained_match_candidates`; plain queries have no equivalent counter.

## Direction

- When the actionable projection yields no matches but the same query matches
  non-actionable nodes in the acquired tree, report a bounded count (e.g.
  `non_actionable_match_count: N`) alongside `no_match`, in a calm
  informational voice — mirroring the `uncontained_match_candidates`
  diagnostic. Zero stays omitted or zero per existing conventions of the
  response shape.
- The counter is presentation/diagnostic only: outcomes, matching semantics,
  and actionability rules are unchanged. Follow canonical-result-projection:
  derive the count from the already-acquired tree; do not run a second
  acquisition.
- Wire schema addition → regenerate schemas; `bash
  scripts/check-wire-enum-schemas.sh` green.

## Acceptance criteria

- [ ] A query whose only matches are non-actionable returns `no_match` plus
      the non-actionable count; a query with zero matches anywhere returns
      `no_match` without a misleading count.
- [ ] Existing unique/ambiguous/truncated outcomes and
      `uncontained_match_candidates` behavior unchanged.
- [ ] Schema regenerated and wire checks green; tests pin both no_match
      variants.
- [ ] Full workspace gate green.

## Implementation notes

- Execution capability: inline implementation; the query already has one bounded snapshot scan and one result-construction seam.
- Review weight: standard default; no independent review requested.
- Files changed: `crates/krometrail-core/src/browser/observation.rs`, `crates/krometrail-cdp/src/control/snapshot.rs`, `docs/SPEC.md`, and `plugin/skills/krometrail/SKILL.md`.
- Tests added: `query_no_match_reports_bounded_non_actionable_matches_only_when_present` pins positive, zero-match omission, and saturation behavior from the acquired tree.
- Simplification: the existing no-match diagnostics constructor remains the compatibility-free default while the query path supplies the additive non-actionable count through a single canonical projection.
- Discrepancies from design: the bounded count uses the existing `RelaxedMatchCandidates` shape so saturation remains explicit alongside the count; no checked-in schema file is generated in this workspace.
- Adjacent issues parked: none.

## Review

Bounded fresh-context review: PASS, no findings. Count derives from the single acquired-tree pass, saturates per the relaxed-candidates convention, appears only on no_match with matches present, and stays distinct from uncontained_match_candidates in SPEC/skill wording.
