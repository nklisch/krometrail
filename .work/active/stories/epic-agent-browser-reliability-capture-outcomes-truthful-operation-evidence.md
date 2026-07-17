---
id: epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-capture-outcomes
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Keep proven operations separate from evidence degradation

Once input dispatch or a page mutation is proven, retain its dispatched/succeeded outcome even if completion probing or post-operation live observation fails. Return the unavailable evidence as MCP degradation with the same interaction anchor rather than a replay-encouraging generic action failure.

## Acceptance evidence

- Navigation regression tests prove a committed reload remains succeeded when observation is interrupted.
- Interaction tests prove post-dispatch completion failure returns a dispatched anchor and unavailable observation, while pre-dispatch failure remains failed.
- MCP tests prove degraded responses are non-error and preserve the operation result, while true operation failures remain error responses.

## Ordering

This checkpoint has no sibling dependency. It must complete before compositor readiness so readiness fallback can rely on the truthful post-dispatch boundary.

## Implementation evidence

- Successful navigation/page mutations now retain `Succeeded` when post-operation evidence is interrupted or unavailable.
- Post-dispatch interaction completion/observation failures retain the dispatched interaction anchor with unavailable `page_observation_failed` evidence.
- Page lifecycle cancellation/disconnect regressions pass with the truthful non-replay boundary.
