---
id: epic-agent-browser-ergonomics-local-io-clipboard
kind: story
stage: implementing
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics-local-io
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Explicit managed-page clipboard

## Checkpoint

Implement Unit 1 of the parent design: bounded explicit read/write operations that preserve browser focus and permission authority, reject attachment, and redact content outside the explicit request/result.

## Acceptance evidence

The core, scripted-CDP, real-browser, and privacy tests named in Unit 1 pass without any clipboard permission mutation command.
