---
id: epic-agent-browser-ergonomics-local-io-download-authority
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

# Bounded managed-download authority

## Checkpoint

Implement Unit 2 of the parent design: private session directory, browser-level lifecycle reducer, bounded completion/cancellation, privacy-safe events, and idempotent shutdown cleanup.

## Acceptance evidence

The reducer, filesystem-boundary, scripted-CDP, real-browser, and privacy tests named in Unit 2 pass; attached sessions touch neither permission nor filesystem authority.
