---
id: epic-agent-browser-ergonomics-viewport-intent-runtime
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-viewport-intent
depends_on: [epic-agent-browser-ergonomics-viewport-intent-contract]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Integrate presets with the viewport lifecycle

Materialize presets before the existing apply/observe/commit boundary, decode effective layout and visual geometry plus viewport-meta presence, return bounded guidance, and qualify responsive/mobile behavior through generated MCP schema, real Chrome, and the plugin skill.

## Acceptance evidence

- Scripted lifecycle tests prove preset/custom command equivalence, rollback, reconnect, clear, and target isolation.
- A valid mobile visual/layout mismatch succeeds with specific guidance instead of being corrected or failed.
- Real Chrome and MCP tests verify the two intent classes, provenance, and unchanged custom behavior.

## Ordering

Depends on `epic-agent-browser-ergonomics-viewport-intent-contract`; completes the feature's externally usable slice.
