---
id: browser-runtime-manual-test-hardening
kind: feature
stage: drafting
tags: [browser, agent-ux, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Harden browser runtime behavior found through comparative manual testing

Fix the remaining runtime defects reproduced while comparing Krometrail with the in-app Browser and Chrome: frame-heavy capture acknowledgement failure, frame-scoped queries being rejected by unrelated main-document size, interaction-relative temporal ranges extending beyond the latest retained damage frame, and preserve-focus sessions requiring destructive restart when an agent deliberately needs to foreground one controlled page.

## Source findings

- `idea-frame-heavy-ack-regression`
- `idea-frame-query-global-cap`
- `idea-clamp-interaction-capture-tail`
- `idea-runtime-focus-escalation`

## Simplification opportunity

Keep capture acknowledgement, document-scoped snapshot authority, temporal range resolution, and target activation in their existing owners. Prefer one-shot explicit foreground activation over a second mutable session-policy system, and delete any recovery prose that still instructs agents to restart a healthy managed session.
