---
id: epic-state-aware-interaction-results-postcondition-core
kind: feature
stage: drafting
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
