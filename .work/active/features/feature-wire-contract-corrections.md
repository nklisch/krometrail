---
id: feature-wire-contract-corrections
kind: feature
stage: drafting
tags: [agent-ux]
parent: null
depends_on: [feature-schema-domain-conformance-enforcement]
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Wire-contract corrections

## Brief

Reconcile the concrete schema/domain mismatches that the 2026-07-20 sixth
shakedown found by hand, plus whatever the conformance sweep in
`feature-schema-domain-conformance-enforcement` surfaces.

This feature is deliberately sequenced **after** that one. Its scope is not fully
known at scope time: the shakedown sampled the MCP surface rather than sweeping
it, and the generative conformance test is what turns the sample into a complete
list. Sizing this before that test runs would be guessing.

## Known mismatches

**1. `frequency_mode` advertises values that are all rejected.** The schema
publishes `enum: ["Count", "Magnitude", "NormalizedFrequency"]` while the
deserializer accepts only `count`, `magnitude`, `normalized_frequency`. The
schema's own `default` field says `"normalized_frequency"`, contradicting its own
enum. Every advertised value is invalid.

This is downstream of the `stable_registry!` root cause and may be fixed
wholesale by tier 1 of the prerequisite feature. If so, this item reduces to
verifying it — which is the correct outcome, not a reason to duplicate work here.

**2. `region_filmstrip` rejects `display_scale: fit_limits`.** The published
schema advertises `fit_limits` as one of three `display_scale` variants, with a
description explaining how it resolves; the domain rejects it with
`"filmstrip display scale must be explicit"`. Both sides are defensible, so this
needs a decision rather than a patch:

- *Narrow the schema* — remove `fit_limits` from this generator's `display_scale`,
  if there is a real reason a filmstrip cannot resolve limits the way storyboard
  normalization does.
- *Widen the domain* — accept `fit_limits` and resolve it, if the restriction is
  incidental.

Design must pick one and record why. The conformance test from the prerequisite
feature will fail until it does, which is the intended forcing function.

**3. PascalCase outliers.** `RetentionPolicy` and `CaptureGapPolicy` are
consistent (Pascal on both sides, so they work) but diverge from the project
standard of 185 `rename_all = "snake_case"` types. Normalizing them is a genuine
breaking input change (`AllowPartial` → `allow_partial`), acceptable under
Current Contract Discipline. Note the mechanical fold into `stable_registry!` is
owned by the prerequisite feature's tier 1; what belongs here is any caller-side
or documentation fallout.

## Simplification opportunity

One casing convention across the whole wire surface, so contributors stop having
to know which of three families a given type belongs to. Possible deletion of
now-redundant per-type schema assertions, coordinated with the prerequisite
feature's own cleanup pass so the two do not both claim the same removals.

## Risks

- **Scope is genuinely unknown.** If the conformance sweep surfaces a large set,
  this may warrant splitting rather than absorbing everything. Design should size
  first and split if warranted rather than growing without bound.
- Overlap with the prerequisite feature is real: tier 1 there may resolve item 1
  and the mechanical half of item 3 outright. Design should re-read the delivered
  state before planning, and shrink this feature rather than redo work.
- Item 2 is a contract decision, not a bug fix. Resolving it by whichever side is
  cheaper to change would be the wrong instinct.

Origin: 2026-07-20 sixth shakedown against v1.2.6.
