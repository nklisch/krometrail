---
id: epic-agent-browser-ergonomics-browser-contexts-page-relationships
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-browser-contexts
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Popup relationships and page waits

Implement Unit 2 of the parent design: monotonic page-context inventory, opener projection, and race-safe page waits without changing `list_pages`.

Acceptance evidence is the reducer/session/registry test slice listed in Unit 2. This checkpoint is independent of profile and asset discovery.
