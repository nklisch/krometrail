---
id: epic-agent-browser-ergonomics-semantic-targeting-query-resolution
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-semantic-targeting
depends_on: [epic-agent-browser-ergonomics-semantic-targeting-query-contract]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Resolve semantic queries through the snapshot registry

Atomically enrich the active accessibility snapshot with bounded DOMSnapshot-derived label, rendered-text, and test-id metadata; match candidates in document preorder with stale descendant-scope fencing; expose the registry-derived MCP response; and qualify the workflow in real Chrome and the plugin skill.

## Acceptance evidence

- Scripted adapter tests protect DOM/AX joining, all query kinds, scope, ambiguity, limits, and fail-closed malformed input.
- Real Chrome resolves a unique reference, uses it in an existing mutation, exposes ambiguous matches, and invalidates stale references after navigation.
- MCP results remain bounded, image-free, and explicit about no-match/unique/ambiguous/truncated outcomes.

## Ordering

Depends on `epic-agent-browser-ergonomics-semantic-targeting-query-contract`; completes the feature's externally usable slice.
