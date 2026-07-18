---
id: epic-agent-browser-ergonomics-response-projections-route-integration
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-response-projections
depends_on: [epic-agent-browser-ergonomics-response-projections-projector]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Projection routing, concise status, and agent guidance

## Checkpoint

Wire the shared response preference through browser and temporal routes, add the additive concise `browser_status` request, honor explicit diagnostic omission without weakening structured failures, regenerate schema fixtures, and teach agents to request the cheapest sufficient projection.

## Acceptance evidence

- Stdio integration covers legacy, compact, full, omit, concise status, invalid preference, and failed/degraded diagnostic behavior.
- Concise status retains capture loss/failure and retention-pressure facts while excluding compatibility and timing detail.
- Plugin instructions include economical request examples and explicit drill-down guidance.

## Ordering

Depends on `epic-agent-browser-ergonomics-response-projections-projector`; it consumes that single contract rather than introducing route-local variants.
