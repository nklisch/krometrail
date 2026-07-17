---
id: epic-agent-browser-reliability-capture-outcomes-capture-health
kind: story
stage: implementing
tags: [browser, storage, agent-ux]
parent: epic-agent-browser-reliability-capture-outcomes
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Preserve and surface retained-capture health

Add the stable `capture_failed` code and bounded capture `failure_stage`, retain the initiating stage through the CDP pipeline, and project every current failed target as an MCP warning on every later browser operation without blocking current-state control.

## Acceptance evidence

- Core constructor/serde tests prove failure stage is present exactly for failed streams.
- CDP fault injection proves event stream, acknowledgement, decode, frame persistence, and gap persistence failures retain the first stage.
- MCP response/server tests prove successful screenshots and operations become degraded with target-scoped capture warnings while existing action failures retain precedence.
- Sanitized tracing evidence contains only session/target/generation/stage fields and composes with the durable-diagnostics response correlation contract.

## Ordering

This checkpoint has no sibling dependency and must land before the feature's documentation describes capture-failure diagnostics.
