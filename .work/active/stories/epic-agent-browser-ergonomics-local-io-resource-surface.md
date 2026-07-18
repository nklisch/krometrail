---
id: epic-agent-browser-ergonomics-local-io-resource-surface
kind: story
stage: implementing
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-local-io
depends_on: [epic-agent-browser-ergonomics-local-io-download-authority]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Active-session download resources

## Checkpoint

Implement Unit 3 of the parent design after the download authority exists: strict canonical resource reads through `BrowserSessionOwner` plus installed skill lifetime/privacy guidance.

## Acceptance evidence

Canonical URI, byte-exact active-session read, post-stop invalidation, schema/resource registry, and plugin static tests named in Unit 3 pass.
