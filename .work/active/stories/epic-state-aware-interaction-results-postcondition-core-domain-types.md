---
id: epic-state-aware-interaction-results-postcondition-core-domain-types
kind: story
stage: implementing
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
