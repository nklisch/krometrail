---
id: epic-state-aware-interaction-results-postcondition-core
kind: feature
stage: review
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Postcondition core

## Brief

The foundation feature of the epic: every successful interaction result carries
a bounded, on-by-default postcondition block of observed pre/post deltas —
navigation/URL identity, backing-node identity (did the reference's node
survive; did the document generation change), and control state
(checked/expanded/selected/pressed and a value-changed fact). This directly
answers issue #14 finding #1 (link click stayed on the same route; radio clicks
left checked state unchanged; a button click navigated somewhere unexpected —
all reported as plain success today).

Covers: the postcondition domain type on `InteractionRecord` (persisted
automatically through the store's opaque `record_json`); pre-state capture in
the existing actionability pre-flight (`resolve_backend_node` already computes
editable/select/hidden facts in one `Runtime.callFunctionOn` and discards them
— widen that payload and `ResolvedNode` to carry checked/expanded/selected/
value facts) plus one net-new bounded pre-action page-identity read (URL);
post-state extraction from the post-action `LiveObservation`; delta assembly in
`execute_interaction_request_inner`; and the concise-projection integration.

Does NOT cover: side-channel outcomes (new page, download, clipboard — the
side-channel feature), expectation notes (the notes feature), or imagery
completeness (the visual-completeness feature).

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: foundation feature — side-channel outcomes and expectation
  notes attach to the postcondition block this feature introduces.

## Simplification opportunity

- The MCP-layer `semantic_outcomes` list describes current post-action state
  and explicitly does not claim a pre/post change; with a postcondition block
  present, design must decide whether to consolidate into one bounded
  post-action semantic surface rather than shipping two parallel lists.
- Postcondition facts follow the `SanitizedParameters` privacy discipline
  already in the record: booleans, bounded enums, lengths, opaque ids — never
  raw values or page content.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (postcondition contract direction)
- `docs/ARCHITECTURE.md` — Interaction Execution, MCP Boundary
- GitHub issue #14, finding #1 (correlation `a7ee197f-01b8-470a-af45-481b978f6445`)

## Design decisions

- **Assembly lives in the CDP control path, types in core**: the projection
  layer never derives postconditions (it presents already-acquired structures;
  pre-state is not visible there), and no new observation service is
  introduced. Matches injected-core-ports.
- **URLs never enter the postcondition**: the CDP layer compares pre/post
  `location.href` strings (same `Runtime.evaluate` expression family `inspect`
  already uses, so the comparison is source-consistent) and passes only the
  boolean inward. Stricter than needed given the observation already carries
  the current URL, but it keeps the persisted record free of URL material.
- **`semantic_outcomes` is retained, not consolidated**: it reports bounded
  *current* alerts/status/text (content-bearing, presentation-layer); the
  postcondition block reports *deltas* (fact-bearing, domain-layer, persisted).
  No content overlap; consolidating would drag content-redaction concerns into
  the record. This resolves the epic's consolidation arc: intentionally
  retained, both bounded.
- **Postcondition is non-optional on the record**: every fact individually
  degrades to a not-observed state; `WriteClipboard`/`CancelDownload` records
  (which also flow through `InteractionRecord`) carry an all-unobserved block
  until the side-channel feature enriches them.
- **Lifecycle subscription becomes unconditional (passive)**: today the
  interaction path subscribes to lifecycle signals only when
  `wait_for_navigation` is set; postconditions subscribe always but never add
  wait time — `navigation_lifecycle_observed` reports what arrived before the
  observation point. Observation-point semantics per SPEC.

## Architectural choice

CDP-assembled, core-typed facts (option A) over (B) projection-layer diffing —
impossible without leaking pre-state capture into the MCP layer and contrary
to "projection changes presentation only" — and (C) a dedicated observation
service/port — machinery with one caller. The pre-flight probe
(`resolve_backend_node`) already executes a side-effect-free
`Runtime.callFunctionOn` per interaction and discards its state; the design
widens that payload, adds one bounded post-action re-probe of the same backend
node, and one pre-dispatch `location.href` read.

## Implementation Units

### Unit 1: Postcondition domain types
**File**: `crates/krometrail-core/src/browser/postcondition.rs` (new module, exported from `browser/mod.rs`)
**Story**: `epic-state-aware-interaction-results-postcondition-core-domain-types`

```rust
/// Bounded node-state facts captured by an actionability probe.
/// All fields are observations; absence means the probe could not read the fact.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeStateFacts {
    pub connected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
    pub pressed: Option<bool>,
    pub value_length: Option<u32>,
}

/// One observed boolean pre/post fact. `changed` is None when either side is
/// unobserved; constructors enforce consistency (validated wire contract).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FlagObservation {
    pub before: Option<bool>,
    pub after: Option<bool>,
    pub changed: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetNodeOutcome {
    /// Action had no element target (e.g. page-scoped press_keys).
    NotEvaluated,
    /// The pre-resolved backing node was still connected post-action.
    Present,
    /// The pre-resolved backing node was gone or replaced post-action.
    DetachedOrReplaced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TargetPostcondition {
    pub node: TargetNodeOutcome,
    pub checked: FlagObservation,
    pub expanded: FlagObservation,
    pub selected: FlagObservation,
    pub pressed: FlagObservation,
    pub value_length_changed: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PagePostcondition {
    /// None when the pre or post URL read degraded. URLs themselves never
    /// enter this type.
    pub url_changed: Option<bool>,
    /// A page lifecycle signal arrived between dispatch and observation.
    pub navigation_lifecycle_observed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct InteractionPostcondition {
    pub page: PagePostcondition,
    pub target: TargetPostcondition,
}

impl InteractionPostcondition {
    /// Pure assembly from pre/post probes; every degradation maps to
    /// not-observed, never to an error.
    pub fn from_facts(
        pre: Option<&NodeStateFacts>,
        post: Option<&NodeStateFacts>,
        url_changed: Option<bool>,
        navigation_lifecycle_observed: bool,
    ) -> Self;
    /// All facts unobserved (pre-dispatch-only records, clipboard/download records).
    pub fn unobserved() -> Self;
}
```

**Implementation Notes**:
- Wire deserialization via the existing `request_wire!`/`deserialize_validated`
  discipline: `FlagObservation`'s `changed` must equal
  `before.zip(after).map(|(b, a)| b != a)`; reject inconsistent wire input.
- Serde snake_case renames on all output enums; no `JsonSchema` derives
  (output-only — the wire-enum guard targets input schemas).
- `TargetNodeOutcome::DetachedOrReplaced` when the pre-resolved node existed
  but the post probe reports `connected: false` or the backend node no longer
  resolves.

**Acceptance Criteria**:
- [ ] `FlagObservation` wire decoding rejects an inconsistent `changed`.
- [ ] `from_facts` truth table: both probes present and differing → `Changed`;
      missing post probe → all-unobserved target facts with node outcome
      preserved; no panic on any combination.

---

### Unit 2: Record integration
**File**: `crates/krometrail-core/src/browser/interaction.rs`
**Story**: `epic-state-aware-interaction-results-postcondition-core-domain-types`

Add `pub postcondition: InteractionPostcondition` to `InteractionRecord` (and
its `Wire` twin + `request_wire!` mapping + `InteractionRecord::new`
parameter). Persisted automatically through the store's `record_json`; update
`decode_interaction` round-trip coverage in
`crates/krometrail-store/src/index/interactions.rs` tests.

**Acceptance Criteria**:
- [ ] Record serialization round-trips the postcondition through
      `record_json` (store decode test).
- [ ] All existing record constructions compile with an explicit block
      (`unobserved()` where facts are not yet captured).

---

### Unit 3: Pre-flight fact capture
**File**: `crates/krometrail-cdp/src/control/snapshot.rs`
**Story**: `epic-state-aware-interaction-results-postcondition-core-fact-capture`

Widen the `resolve_backend_node` `functionDeclaration` to additionally return
`{checked, ariaExpanded, selected, pressed, valueLength}` (per-property guards
inside the JS so an unreadable property degrades that field to null rather
than throwing under `throwOnSideEffect: true`), parse into
`NodeStateFacts`, and carry it on `ResolvedNode`:

```rust
pub(crate) struct ResolvedNode {
    pub(crate) backend_node_id: i64,
    pub(crate) document_quad: [f64; 8],
    pub(crate) facts: NodeStateFacts,
}
```

**Implementation Notes**:
- `checked`: `INPUT` checkbox/radio native `checked`, else `aria-checked`
  ("true"/"false" only; "mixed" maps to unobserved). `expanded`/`pressed`:
  aria attributes. `selected`: `OPTION.selected` or `aria-selected`.
  `value_length`: string `value` length only — never the value.
- The existing actionability validation is untouched; facts ride the same
  response value.

**Acceptance Criteria**:
- [ ] Existing resolution tests still pass; new doubles assert facts parse
      and degrade per-field.

---

### Unit 4: Post-action capture and assembly
**File**: `crates/krometrail-cdp/src/control/interaction.rs`
**Story**: `epic-state-aware-interaction-results-postcondition-core-fact-capture`

In `execute_interaction_request_inner`:
1. After target binding, before dispatch: one bounded pre-URL read
   (`Runtime.evaluate` `location.href`, silent, side-effect-checked); degrade
   to `None` on any failure.
2. Subscribe to lifecycle signals unconditionally (currently gated on
   `navigation_aware`); record whether a signal arrived before observation.
   Never adds wait time — the `NAVIGATION_AWARE_WINDOW` wait remains gated on
   `wait_for_navigation` exactly as today.
3. After completion, before/alongside `observe_live`: one post-action state
   probe of the same `backend_node_id` (reusing the widened probe; a
   resolution failure maps to `DetachedOrReplaced`). Skipped when the plan had
   no element target (`NotEvaluated`).
4. Post-URL from the observation's `PageState.url`; degraded observation →
   `None`.
5. `InteractionPostcondition::from_facts(...)` → `InteractionRecord::new`.

**Implementation Notes**:
- Probe/URL failures must never fail or delay a proven dispatch: every
  degradation maps to unobserved facts (mirrors observation-degradation
  philosophy in SPEC).
- Batch steps flow through the same path and inherit postconditions with no
  batch-specific code.

**Acceptance Criteria**:
- [ ] Checkbox-click double: `checked` `false→true`, `changed: true`.
- [ ] Link-click double without navigation: `url_changed: false`,
      `navigation_lifecycle_observed: false`.
- [ ] Fill double: `value_length_changed: true`.
- [ ] Post-probe transport failure degrades to unobserved facts; the action
      still reports success.
- [ ] Page-scoped `press_keys` (no locator): target facts `NotEvaluated`,
      page facts still observed.

---

### Unit 5: Concise projection
**File**: `crates/krometrail-mcp/src/response.rs`
**Story**: `epic-state-aware-interaction-results-postcondition-core-projection`

The interaction projection arm attaches `result["postcondition"]` (serialized
from `record.postcondition`) at every detail level — concise included, per the
on-by-default strategic decision. The `expanded`/`full` record echo continues
to carry the same block inside `record` (one authority, projected twice is
acceptable: the concise block IS the record field).

**Acceptance Criteria**:
- [ ] Concise interaction response contains the bounded postcondition block.
- [ ] Response-shape tests updated; no other tool responses change.

---

## Implementation Order
1. `postcondition-core-domain-types` (Units 1-2)
2. `postcondition-core-fact-capture` (Units 3-4)
3. `postcondition-core-projection` (Unit 5)

## Simplification
- `semantic_outcomes` intentionally retained (see Design decisions) — no
  parallel-surface growth beyond the one postcondition block.
- The widened probe replaces what would otherwise become a second per-target
  state read in later features; side-channel and notes consume these facts
  without new capture machinery.

## Testing
- Core truth-table test for `from_facts` (complex isolated logic).
- Wire-consistency rejection test for `FlagObservation` (validated-wire-contracts).
- CDP deterministic doubles per Unit 4 acceptance criteria, including the
  degraded-probe path (bounded-loss philosophy: absence is explicit).
- Store round-trip (`record_json` decode) with a populated block.
- MCP response-shape test for the concise block.
- One gated real-Chrome qualification: checkbox click asserts a checked delta
  end-to-end (`KROMETRAIL_REAL_CHROME_TESTS`).

## Risks
- **`throwOnSideEffect` conservatism**: Chrome's side-effect analysis may
  refuse some property getters. Mitigation is designed in: per-property guards
  in the probe JS degrade individual facts to null instead of failing the
  probe; the fallback if a getter class proves unreadable is dropping that
  fact to unobserved, never blocking the action.
- **Two added CDP round-trips per interaction** (pre-URL, post-probe).
  Bounded, silent, no retries; if measured cost matters later it routes to a
  `[perf]` item, not into this design.
- **Post-probe timing races legitimate async UI**: a framework may replace the
  node after observation. `DetachedOrReplaced` is itself the honest fact;
  observation-point framing in SPEC covers it.

## Implementation notes

All three stories landed in design order (commits `dc17fe19`, `c7b84b3d`,
`bf02f1e8`); per-story detail lives in the story bodies. Deviations and
discoveries beyond the design text:

- **Container types derive plain `Deserialize`.** The design listed the
  postcondition containers Serialize-only, but the record persists as opaque
  `record_json` and must decode; validation concentrates in
  `FlagObservation`'s `deserialize_validated` (the only invariant-bearing
  member), per the validated-wire-contracts "skip for simple data" guidance.
- **Blocked/degraded observation paths report `NotEvaluated`, not
  `DetachedOrReplaced`.** The design's step 3 only defines the post probe on
  the observe path; probing a dialog-blocked renderer would time out and
  then falsely claim node detachment. `TargetNodeOutcome::NotEvaluated`'s doc
  is widened accordingly ("no element target, or the observation point was
  blocked"). On the healthy path, probe/resolution/transport failure maps to
  `DetachedOrReplaced` with unobserved after-facts exactly as designed.
- **HandleDialog also skips the pre-URL read** — an open modal blocks the
  renderer's evaluation loop, and the read must never sit in front of the
  dialog handling that unblocks it. Its URL fact degrades to unobserved.
- **Both silent reads share a 2s `POSTCONDITION_PROBE_WINDOW`** so a stalled
  renderer degrades facts instead of delaying a proven dispatch.
- **A disconnected post probe keeps its readable state out of the
  after-facts**: a detached node's properties no longer describe the current
  document, so only `DetachedOrReplaced` is claimed.
- **Scripted-harness seam**: `ScriptedCdp` answers the silent `location.href`
  expression out of band (before its per-method queue) so the existing
  interaction scripts stayed byte-stable; the URL comparison's other side
  remains fully scriptable through the observation identity read.
- **Risk #1 (throwOnSideEffect conservatism) qualified against real Chrome**:
  the widened probe with per-property guards passes the local Chrome's
  side-effect analyzer; the new gated checkbox qualification observes the
  full `false → true, changed: true` delta end-to-end in 1.2s.

Verification summary: full gate green after each story (fmt, wire-enum
schema guard, check, workspace tests — finally 1189 passed / 0 failed —
clippy `-D warnings`). Real-Chrome opt-in runs: the new checkbox
qualification and the dialog-synchronization test pass; the pre-existing
`opt_in_real_chrome_executes_verified_interaction_families` fails identically
at the v1.4.0 release commit on this machine (its fixture-mutation helper
`window.replaceClickTarget()` is refused as side-effecting by the local
Chrome's read-only evaluation) — pre-existing, environment-dependent, and
untouched by this feature; flagged here for the review lane rather than
fixed inside this feature's diff.
