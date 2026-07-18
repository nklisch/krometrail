---
id: epic-agent-browser-ergonomics-browser-contexts-frame-scope
kind: story
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-browser-contexts
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Qualified frame scope

Implement Unit 3 of the parent design after the parent feature dependency `epic-agent-browser-ergonomics-semantic-targeting` is available: frame inventory, same-origin/same-process qualification, semantic reference binding, and root-viewport geometry.

Acceptance evidence is the core/scripted-CDP/real-browser frame slice listed in Unit 3. Cross-origin, OOPIF, detached, and indeterminate variants must fail closed.

## Implementation notes

- Execution capability: direct inline implementation within the parent feature bundle.
- Added bounded preorder frame inventory with loader-scoped opaque tokens, sanitized URLs, parent/depth projection, and main/same-origin/cross-origin/OOPIF/indeterminate access classification. Raw frame and loader IDs remain adapter-private.
- `query_page` accepts an explicit document scope, re-reads and requalifies the frame tree and browser target inventory, rejects stale/cross-origin/OOPIF/indeterminate references before AX/DOM access, and scopes the AX tree to a qualified frame. Existing exact node references remain the action boundary.
- Verification: frame token navigation invalidation, access classification, CDP all-target check, semantic MCP schema coverage. No coordinate fallback was added.
