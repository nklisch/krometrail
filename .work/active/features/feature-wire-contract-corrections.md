---
id: feature-wire-contract-corrections
kind: feature
stage: done
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

## Sizing (from the conformance sweep)

The prerequisite feature's sweep ran and reported eight failure classes. Triage
splits them in two, and the split is the central design decision here: **three of
the eight are not contract defects at all — they are the sweep asserting beyond
what a JSON Schema can express.**

### Class A — genuine: schema advertises what the domain rejects

| # | Tools | Advertised | Domain |
|---|---|---|---|
| A1 | `fill`, `select_option`, `upload_files` | coordinate locator branch | element locator only |
| A2 | 9 range-consuming tools | `anchor_kind: latest_interaction` | `interval`/`interaction`/`navigation`/`marker`/`source_frames` |
| A3 | `generate_artifacts`, `generate_region_filmstrip` | `scale.factor` 0–255 (uint8) | 2–8 |
| A4 | `generate_artifacts` storyboard | `tile_limit` 0–255 (uint8) | 3–12 |
| A5 | `region_filmstrip` | `display_scale: fit_limits` | rejected, "must be explicit" |

All five resolve the same way: **narrow the schema to what the domain accepts.**
Per the prerequisite feature's contract rule, a domain-only restriction is itself
the bug — once expressed in the schema, the sweep guards it permanently.

A2 is worth stating precisely: `latest_interaction` is a *request* anchor that
collapses to `interaction` once resolved. The resolved-range wire type should
therefore never advertise it. This is a modelling leak, not a missing branch —
do not "fix" it by teaching the resolver to accept `latest_interaction`.

A3 and A4 are not merely conformance fixes; publishing real bounds is a genuine
agent-surface improvement, since `0–255` actively misleads a caller into
constructing invalid requests.

### Class B — sweep defects, fix in the sweep not the contract

| # | Symptom | Why it is not a contract defect |
|---|---|---|
| B1 | `batch` steps → `missing field 'query'` etc. | Generator emitted `"request":{}`; it does not recurse into each operation's nested request schema. The schema **does** declare those required fields correctly. |
| B2 | difference-map `reference.id` → "outside the resolved range" | Sweep invented a UUID absent from its own synthetic range. |
| B3 | markers `session_time: 1`, `focus_times: [1]` → outside range | Same: synthetic scalar unrelated to the synthetic range. |

B2 and B3 are **cross-field state validation** — "this identifier must appear in
`frame_ids` of this same request", "this timestamp must fall inside
`resolved_range`". No JSON Schema can express an intra-document consistency rule
of that kind, so a conformance sweep must not fail on it. Left unaddressed these
are permanently unfixable red, which would train everyone to ignore the sweep.

B1 is a straightforward generator bug: recurse into nested request schemas and
fill their `required` properties.

## Design decisions

- **Narrow schema, never widen domain, for all of Class A.** Each restriction is
  deliberate and load-bearing (element-only locators, admissible downscale
  factors, legible tile counts). Widening the domain to match an over-broad
  schema would degrade real invariants to satisfy a test.
- **Scope the sweep to schema-expressible constraints.** It must assert enum
  membership, `oneOf` branch validity, and declared numeric bounds. It must not
  assert cross-field or state-dependent consistency. This is a correction to the
  sweep delivered by the prerequisite feature, made here because the sweep only
  revealed its own over-reach once run against the full surface.
- **Do not suppress Class B with an allowlist.** An ignore-list would let genuine
  future regressions hide behind it. Fix B1's generator; scope B2/B3 out by
  construction so there is nothing to suppress.

## Implementation Units

### Unit 1: Recurse into nested request schemas (B1)
**File**: the conformance sweep module in `crates/krometrail-mcp/`

Minimal-instance generation must descend into `batch` step request schemas and
satisfy their `required` properties rather than emitting `{}`.

**Acceptance Criteria**:
- [ ] Every `batch` operation yields a structurally complete minimal instance.
- [ ] No `missing field` failures remain.
- [ ] The vacuity guard still passes (nested branches counted as visited).

### Unit 2: Scope the sweep to schema-expressible constraints (B2, B3)
**File**: the conformance sweep module in `crates/krometrail-mcp/`

Exclude cross-field/state-dependent validation from the sweep's remit.

**Implementation Notes**:
- Prefer exclusion **by construction** (do not synthesize values for fields whose
  validity depends on other fields) over catching-and-ignoring errors by message.
  Matching on error strings would silently swallow real regressions when wording
  changes.
- Record the boundary in a comment so a later reader does not "helpfully" widen
  the sweep back into state validation.

**Acceptance Criteria**:
- [ ] No failures from identifier- or timestamp-in-range rules.
- [ ] The exclusion is structural, not error-message matching.
- [ ] A deliberately introduced *enum* mismatch still fails (exclusion did not
      blunt the sweep).

### Unit 3: Narrow Class A schemas
**Files**: browser operation schemas; `crates/krometrail-core/src/timeline/range.rs`; artifact request types

A1 element-only locators; A2 drop `latest_interaction` from resolved-range wire;
A3 `factor` 2–8; A4 `tile_limit` 3–12; A5 resolve `fit_limits`.

**Implementation Notes**:
- A5 needs a decision recorded either way: narrow the schema (if filmstrips
  genuinely cannot resolve limits) or accept it in the domain (if incidental).
  Prefer narrowing unless accepting is trivially correct.
- A3/A4 bounds must come from the same constants the domain validates against —
  do not hand-copy numbers that can drift.

**Acceptance Criteria**:
- [ ] All five Class A entries pass the sweep.
- [ ] Each narrowed bound derives from the domain's own constant.
- [ ] Previously-valid requests still succeed; only genuinely-rejected inputs are
      no longer advertised.
- [ ] A5's resolution is recorded with rationale.

## Implementation Order

1. Unit 1 (B1 generator recursion)
2. Unit 2 (B2/B3 scoping) — 1 and 2 together clear the false positives
3. Unit 3 (Class A narrowing) — real fixes, verified against a sweep that now
   only reports real things

## Testing

- The sweep itself is the acceptance evidence; it must go from 8 failure classes
  to zero **without** any suppression mechanism.
- Negative control: a deliberately reintroduced enum mismatch must still fail.
  Without this, Units 1–2 could pass by disabling the sweep.
- A1/A2 need assertions that the removed branches are genuinely unreachable, not
  merely unadvertised.

## Risks

- **Largest risk: Units 1–2 make the sweep quieter, and quieting a test is
  exactly how a guard gets defeated.** The negative control is mandatory, not
  optional, and review should treat its absence as a blocker.
- A2 touches a wire enum shared across nine tools; removing a variant from the
  resolved-range type must not remove it from the *request* anchor type where it
  is valid and useful.
- A3/A4 tighten previously-permissive schemas. Any in-repo fixture using an
  out-of-bounds value will now fail to build — that is correct, but expect
  fallout in tests and docs.

## Implementation notes

- Execution capability: inline implementation; the feature's schema, generator,
  and domain corrections form one cohesive owner boundary.
- Review weight: standard, from the project default.
- Files changed: `crates/krometrail-mcp/src/registry.rs`,
  `crates/krometrail-mcp/src/schema.rs`, the affected core wire/domain modules,
  and the affected store/root tests and fixtures.
- Tests added/removed: nested batch-schema generation and counting, structural
  state-dependent-field exclusions, enum-mismatch negative control, and
  wire/domain schema assertions; stale expectations were updated for the
  intentional count-mode and resolved-anchor contracts.
- Simplification: removed the resolved-range `latest_interaction` wire branch
  and the filmstrip `fit_limits` schema branch rather than adding compatibility
  paths.
- Discrepancies from design: none. Filmstrip tile limits are distinct from
  storyboard limits (1–24 versus 3–12), and filmstrip display scale remains
  explicitly bounded because the renderer has no fit-limit resolution path.
- Adjacent issues parked: none.

## Review (cross-model, Fable reviewing Luna — three passes)

**Pass 1 NOT SHIP**, one blocker landing squarely on this feature's central
discipline. The sweep had been taught to synthesize `json!(10)` for any pointer
ending `/poll_interval` — exactly `MIN_WAIT_POLL_INTERVAL`. `WaitRequestWire.poll_interval`
is a bare `u64` publishing the full uint64 range while the domain requires
10ms–5s, so this was a genuine, fully schema-expressible Class-A defect hidden
by a name-keyed de-facto allowlist — the precise thing this feature's design
forbade, committed inside the guard built to prevent it.

Resolved the designed way: special case deleted, bounds published from the
domain constants. A full audit of every remaining synthesis case confirmed the
rest are genuine cross-field state (resolved-range anchor pairing, mask
bits-vs-dimensions, range-scoped identifiers) that no JSON Schema can express.

**Pass 2** confirmed the fix with no unit skew between the `Duration` constants
and the published millisecond bounds, and found `timeout` publishing a maximum
but not its non-zero minimum — the same family, fixed.

**Pass 3 SHIP.** All Class A entries pass; the negative control still
discriminates; `latest_interaction` is gone from resolved ranges while remaining
valid on request anchors, with the resolver collapsing it to `interaction` and
tests asserting both directions.

A5 resolved by narrowing: filmstrip rendering requires explicit display scaling,
so accepting implicit limit resolution would contradict deterministic domain
validation. Filmstrip and storyboard tile bounds confirmed genuinely distinct
(1–24 vs 3–12), both derived from shared constants rather than copied literals.
