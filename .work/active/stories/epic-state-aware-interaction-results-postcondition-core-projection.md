---
id: epic-state-aware-interaction-results-postcondition-core-projection
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-postcondition-core
depends_on: [epic-state-aware-interaction-results-postcondition-core-fact-capture]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Concise postcondition projection

Checkpoint for Unit 5 of the parent feature's design: the interaction
projection arm in `crates/krometrail-mcp/src/response.rs` attaches
`result["postcondition"]` (serialized from `record.postcondition`) at every
detail level including concise, per the epic's on-by-default decision.
Includes the response-shape test updates and one gated real-Chrome
qualification (checkbox click asserts a checked delta end-to-end).

## Acceptance evidence

- Concise interaction response contains the bounded postcondition block;
  expanded/full record echo carries the same field.
- Response-shape tests updated; no other tool responses change.
- Gated real-Chrome checkbox qualification added.
- Workspace gate green.
