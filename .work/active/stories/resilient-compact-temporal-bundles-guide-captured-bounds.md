---
id: resilient-compact-temporal-bundles-guide-captured-bounds
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: resilient-compact-temporal-bundles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Guide requests to captured bounds

## Checkpoint

Never-captured range failures expose exact captured bounds, concrete adjusted-request recovery, and retry-after-recovery advice without silently changing `AllowPartial` semantics.

## Acceptance evidence

- Future-edge requests carry requested context plus captured start/end values.
- Existing eviction-edge behavior remains unchanged.
