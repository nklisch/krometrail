---
id: browser-runtime-manual-test-hardening-interaction-tail
kind: story
stage: implementing
tags: [browser, visual, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Clamp eligible natural interaction tails to captured bounds

Under `AllowPartial`, intersect only interaction-derived natural ranges with retained evidence while preserving requested provenance, limitations, and exact behavior for explicit or require-complete requests.
