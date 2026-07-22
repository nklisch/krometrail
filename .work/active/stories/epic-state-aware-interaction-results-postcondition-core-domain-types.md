---
id: epic-state-aware-interaction-results-postcondition-core-domain-types
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-postcondition-core
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Postcondition domain types and record integration

Checkpoint for Units 1-2 of the parent feature's design: the
`InteractionPostcondition` type family in
`crates/krometrail-core/src/browser/postcondition.rs` (NodeStateFacts,
FlagObservation with validated `changed` consistency, TargetNodeOutcome,
page/target blocks, `from_facts` pure assembly, `unobserved()`), plus the
non-optional `postcondition` field on `InteractionRecord` with wire mapping
and store `record_json` round-trip coverage.

## Acceptance evidence

- `FlagObservation` wire decoding rejects inconsistent `changed`.
- `from_facts` truth table passes (differing probes → changed; missing post
  probe → unobserved with node outcome preserved; no panics).
- Store decode test round-trips a populated block through `record_json`.
- Workspace gate green.

## Implementation

Landed as designed:

- `crates/krometrail-core/src/browser/postcondition.rs` — the full type family
  (`NodeStateFacts`, `FlagObservation`, `TargetNodeOutcome`,
  `TargetPostcondition`, `PagePostcondition`, `InteractionPostcondition`) with
  `from_facts` and `unobserved()`. `FlagObservation` decodes through
  `deserialize_validated` and rejects any `changed` that does not equal
  `before.zip(after).map(|(b, a)| b != a)`. No `JsonSchema` derives. Inline
  truth-table, wire-rejection, and serde round-trip tests.
- `InteractionRecord` carries the non-optional `postcondition` field through
  the wire twin and `new`; all constructors updated
  (`WriteClipboard`/`CancelDownload` evidence projections use `unobserved()`;
  the CDP interaction path temporarily passes `unobserved()` until the
  fact-capture story assembles real facts).
- Store round-trip: `crates/krometrail-store/tests/temporal_query_index.rs`
  round-trips a populated block through `record_json` and asserts the decoded
  postcondition facts.

Judgment calls (design left them open):

- The container types derive plain `Deserialize` (persisted `record_json` must
  decode); the design listed them Serialize-only but the invariant-bearing
  member (`FlagObservation`) is the one with validated decoding, matching the
  validated-wire-contracts pattern's "skip for simple data" guidance.
- `from_facts` with a present pre probe and a missing/disconnected post probe
  reports `DetachedOrReplaced` (the design's "backend node no longer resolves"
  note) while keeping observed `before` values and unobserved `after`/`changed`;
  a disconnected post node's readable state stays out of the after-facts since
  it no longer describes the current document.
- `TargetNodeOutcome::NotEvaluated` doc widened to also cover a blocked
  observation point (dialog-blocked renderer), where probing is impossible and
  claiming detachment would be false.

Gate: fmt, wire-enum schemas, check, full test suite (0 failures), clippy -D
warnings — all green.
