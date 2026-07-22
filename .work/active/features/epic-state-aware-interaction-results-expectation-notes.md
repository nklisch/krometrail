---
id: epic-state-aware-interaction-results-expectation-notes
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results
depends_on: [epic-state-aware-interaction-results-postcondition-core, epic-state-aware-interaction-results-side-channel-outcomes]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Expectation notes

## Brief

The interpretive layer, deliberately last and deliberately small: when a
common expectation for the dispatched action observably did not hold, the
interaction result carries at most one conservative expectation note — for
example, a link activation with no navigation, no new page, and no download; or
a checkbox click with no checked-state delta. The note is descriptive ("the
click dispatched and no navigation or page change was observed"), never a
failure claim, per the epic's locked strategic decision. This addresses the
issue #14 finding #1 ask that the result warn when the semantic postcondition
differs from likely intent.

Expectations are declared in the existing browser-operation registry (the
`ActionDefinition` table that already declares category, actionability, and
completion per operation) keyed by action kind and target role — one registry
declaration, not a parallel expectation table. Note derivation is a pure
function over the postcondition facts produced by `postcondition-core` and
`side-channel-outcomes`; it introduces no new observation work.

Does NOT cover: any new fact capture (upstream features own facts), and any
verdict language — the note never says "failed", "broken", or "bug".

## Advisory constraints (binding, from the epic's cross-model adjudication)

Negative notes require a completeness gate: each channel an expectation
depends on (navigation signal, node state, page cursor, download cursor)
carries a typed observation state — changed / unchanged / unavailable /
not-applicable, with what it was observed through — and a "did not hold" note
is emitted only when every required channel was successfully observed.
Anything less becomes "expectation not evaluated", never "no effect
observed". Role-based expectations are suppressed when the target role is
unavailable (coordinate actions, unresolved selectors).

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: consumer of both fact-producing features; the epic's
  highest false-signal-risk surface, so it lands after the facts are proven.

## Simplification opportunity

- Expectations extend the existing operation registry; do not introduce a
  second registry or per-tool special cases.
- Async/deferred applications legitimately defer effects past the observation
  point — the conservative bar (observed facts only, one note, descriptive
  wording, observation-point framing) is the mitigation; design should not add
  configurable sensitivity knobs in v1.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (at most one conservative
  expectation note)
- `docs/ARCHITECTURE.md` — Capability Registry, Interaction Execution
- GitHub issue #14, finding #1
