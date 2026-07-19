---
id: browser-runtime-manual-test-hardening-capture-reconnect
kind: story
stage: implementing
tags: [browser, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Recover capture after terminal acknowledgement failure

Preserve one-shot acknowledgement truth while routing terminal acknowledgement failure through generation-fenced session reconnect. Acceptance is exact one-command/one-gap accounting followed by successful capture on the rebuilt attachment generation.
