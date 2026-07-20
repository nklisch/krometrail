---
id: feature-agent-surface-contract-clarity
kind: feature
stage: implementing
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Agent surface contract clarity

## Brief

Two agent-facing contract defects surfaced in the 2026-07-19 fifth shakedown
(v1.2.5 live). Both cost an agent avoidable work on the most common paths.

1. **The temporal anchor schema advertises optional scope that validation
   rejects.** `resolve_temporal_range` and `temporal_debug_bundle` publish
   `anchor.scope.session_id` and `anchor.scope.target_id` as optional and
   nullable, but a `session_time` or `wall_clock` anchor fails with
   `range anchor requires both session and target scope`. A caller following
   the published schema gets a guaranteed round-trip on the cheapest temporal
   entry point.
2. **Unnamed targets crowd out named ones in the bounded snapshot ranking.**
   Observed live: threejs.org returned ~30 of 48 ranked targets with
   `name: null`; a DuckDuckGo results page returned ~15 of 48 as
   `role: generic, name: null`. An unnamed node cannot be addressed by
   `query_page` semantics and is rarely the agent's intended target, yet it
   displaces an addressable one under a hard 48-entry cap.

## Root cause

### Defect 1

`AnchorScope` (`crates/krometrail-core/src/timeline/range.rs:64-69`) carries
`session_id: Option<SessionId>` and `target_id: Option<TargetId>` and is shared
by **every** variant of `TemporalRangeAnchor` (`range.rs:181-205`). schemars
therefore emits both fields optional-and-nullable for all branches. But
`required_scope` (`range.rs:1547`) demands both, and is called only from the
`SessionTime` arm (`range.rs:984`) and the `WallClock` arm (`range.rs:997`).
The remaining anchors use `validate_scope_match` (`range.rs:1558`), which
legitimately tolerates `None`.

So `Option` is correct for most variants and wrong for exactly two — one shared
type is being asked to describe two different contracts. This is a
`validated-wire-contracts` violation: the published schema permits inputs the
domain rejects.

### Defect 2

`bounded_targets` (`crates/krometrail-mcp/src/response.rs:1495-1539`) selects on
`node.actionable` and sorts by
`(!intersects_viewport, snapshot_action_rank, original_index)`, truncating at
`MAX_CONCISE_TARGETS = 48` (`response.rs:28`) under a parallel JSON byte budget.

`snapshot_action_rank` (`response.rs:1678`) never reads `node.name`:

```rust
if focused { 0 } else if editable { 1 } else if node.role != "link" { 2 } else { 3 }
```

An anonymous in-viewport `button` scores 2, identical to a fully-named button;
only DOM order separates them. The sibling `semantic_rank` (`response.rs:1700`)
already encodes the naming-aware idiom for the `nodes` list — ranking on
`name.is_some() || value.is_some() || description.is_some()`. `targets` simply
does not use it.

## Design decisions

- **Split the scope type rather than patch the schema.** Introduce a scope type
  with non-optional `session_id`/`target_id` for the `SessionTime` and
  `WallClock` variants, leaving `AnchorScope` for the anchors where `None` is
  meaningful. This makes the Rust type, serde, and the generated schema agree
  by construction instead of maintaining a hand-edited schema patch that can
  drift from validation. Under Current Contract Discipline the wire shape
  changes directly; no alias or dual accept.
- **Rank on addressability, not on name alone.** Add a name-presence term to
  the target sort so an addressable target outranks an anonymous one at equal
  action rank. Keep it a *sort* term, not a filter: anonymous actionable nodes
  stay reachable by reference, they simply stop displacing named ones under the
  cap. Reuse the existing `semantic_rank` predicate
  (`name || value || description`) so the two rankings share one definition of
  "identifiable".
- **Do not change the 48 cap or the byte budget.** The cap is a token-economy
  contract; the fix is ordering quality within it, not a larger response.
- **Deliberately out of scope: frame URL redaction.** The shakedown also noted
  that `list_frames` exposes only `origin` + `path_sha256`, so frames cannot be
  told apart by URL. `SanitizedUrl` (`crates/krometrail-core/src/browser/privacy.rs:291`)
  is an intentional, documented privacy invariant with no configuration knob,
  and its constructor rejects rather than downgrades. Frames already carry a
  unique `frame_key` plus `depth`/`parent`, which is sufficient for targeting —
  the actual requirement. Loosening a deliberate privacy boundary to improve
  recognizability is not a trade worth making, and this item is closed as
  working-as-designed.

## Implementation Units

### Unit 1: Required scope for interval anchors
**File**: `crates/krometrail-core/src/timeline/range.rs`

Add a scope type with non-optional `session_id` and `target_id`; use it for the
`SessionTime` and `WallClock` variants of both `TemporalRangeAnchor`
(`range.rs:181`) and `TemporalRangeAnchorWire` (`range.rs:221`). Update the
hand-written `Deserialize` at `range.rs:309` in lockstep. Remove the now-dead
`required_scope` call sites; keep `validate_scope_match` for the tolerant
anchors.

**Implementation Notes**:
- The wire enum and domain enum must change together or the hand-written
  `Deserialize` will not compile against the new variant shape.
- `deny_unknown_fields` is already set on the scope struct; preserve it.

**Acceptance Criteria**:
- [ ] The generated `resolve_temporal_range` and `temporal_debug_bundle`
      schemas mark `session_id` and `target_id` required and non-nullable on
      the `session_time` and `wall_clock` branches only.
- [ ] Those branches still deserialize successfully when both ids are supplied.
- [ ] Omitting either id on those branches fails at deserialization with a
      clear message, not at a later domain check.
- [ ] `interaction`, `latest_interaction`, `navigation`, `marker`, and
      `source_frame` anchors continue to accept partial or absent scope.

### Unit 2: Name-aware target ranking
**File**: `crates/krometrail-mcp/src/response.rs`

Add a name-presence term to `snapshot_action_rank` (or to the `bounded_targets`
sort tuple) so identifiable targets sort ahead of anonymous ones at equal
action rank. Share the identifiability predicate with `semantic_rank`.

**Acceptance Criteria**:
- [ ] Given a snapshot with more than 48 actionable in-viewport nodes, a mix of
      named and anonymous at the same action rank, the concise projection
      retains the named ones.
- [ ] Focused and editable nodes still rank first regardless of naming
      (an anonymous focused input must not be demoted below a named link).
- [ ] Relative order of two equally-identifiable nodes is still DOM order.
- [ ] The 48-entry cap and the JSON byte budget are unchanged.

## Implementation Order
1. Unit 1 (independent)
2. Unit 2 (independent)

## Testing
- Deserialization tests per anchor variant: both ids present, each omitted, and
  the tolerant anchors with absent scope.
- A schema assertion that the two strict branches publish the ids as required,
  guarding against future drift back to optional.
- A `bounded_targets` test with a synthetic over-cap snapshot mixing named and
  anonymous nodes at equal action rank, plus a regression asserting an
  anonymous focused node still leads.
- No real-Chrome tests needed; both units are pure wire and projection logic.

## Risks
- Unit 1 changes a published request shape. Under Current Contract Discipline
  that is acceptable, but the hand-written `Deserialize` is easy to get subtly
  wrong — the tolerant anchors must keep accepting absent scope, which is the
  regression most likely to slip through.
- Unit 2 changes which targets survive truncation. A page whose only actionable
  controls are anonymous must still surface them; the name term must be a sort
  key, never a filter.

Origin: 2026-07-19 fifth shakedown friction report.
