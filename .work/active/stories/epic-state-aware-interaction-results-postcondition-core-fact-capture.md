---
id: epic-state-aware-interaction-results-postcondition-core-fact-capture
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results-postcondition-core
depends_on: [epic-state-aware-interaction-results-postcondition-core-domain-types]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# CDP fact capture and assembly

Checkpoint for Units 3-4 of the parent feature's design: widen the
`resolve_backend_node` probe (per-property-guarded JS returning
checked/expanded/selected/pressed/value-length) and carry `NodeStateFacts` on
`ResolvedNode`; in `execute_interaction_request_inner` add the bounded
pre-dispatch `location.href` read, unconditional passive lifecycle
subscription, post-action re-probe of the same backend node (failure →
`DetachedOrReplaced`; no target → `NotEvaluated`), post-URL from the
observation's `PageState`, and `from_facts` assembly into
`InteractionRecord::new`.

## Acceptance evidence

- Deterministic doubles: checkbox click `checked false→true`; link click
  without navigation `url_changed: false` + no lifecycle signal; fill
  `value_length_changed: true`; degraded post-probe leaves the action
  successful with unobserved facts; page-scoped `press_keys` reports
  `NotEvaluated` target with observed page facts.
- Probe/URL degradation never fails or delays a proven dispatch.
- Batch steps inherit postconditions with no batch-specific code.
- Workspace gate green.
