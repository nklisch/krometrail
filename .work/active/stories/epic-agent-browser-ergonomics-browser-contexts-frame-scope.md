---
id: epic-agent-browser-ergonomics-browser-contexts-frame-scope
kind: story
stage: implementing
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
