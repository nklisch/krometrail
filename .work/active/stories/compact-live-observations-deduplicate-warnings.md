---
id: compact-live-observations-deduplicate-warnings
kind: story
stage: implementing
tags: [agent-ux, diagnostics]
parent: compact-live-observations
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Deduplicate equivalent observation warnings

## Checkpoint

Structurally identical top-level warnings are logged and returned once in first-seen order, while meaningfully distinct errors remain separate.

## Acceptance evidence

- Dialog-blocked observation emits one warning for three unavailable nested components.
- Same-code/different-context warnings are not collapsed.
