---
id: epic-agent-browser-ergonomics-temporal-range-handles-authority
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics-temporal-range-handles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Process-local resolved-range handle authority

## Checkpoint

Add the typed handle identity and one bounded, non-evicting process-local authority that deduplicates exact validated ranges and revalidates every ordered retained source-frame metadata record before resolving a handle.

## Acceptance evidence

- Unit tests prove stable deduplication, distinct identity, capacity behavior, browser-stop survival, session invalidation, and restart/unknown recovery.
- Frame-source doubles prove missing, reordered, or cross-scope metadata fails before a range is returned.
- Composition tests prove MCP dependencies share the one authority built from the root ID source and recording store.

## Ordering

This authority must exist before any public handle-or-range route can be wired.
