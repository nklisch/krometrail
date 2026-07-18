---
id: resilient-compact-temporal-bundles-fit-high-dpi
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

# Fit high-DPI default bundles

## Checkpoint

Default `FitLimits` admits the reproduced 53-frame 2400×1410 interval while preserving all frames and the fixed combined request cap; larger work fails with actionable progressive-evidence recovery.

## Acceptance evidence

- Deterministic scale and cache/manifest identity are verified.
- Peak reservation remains within the combined scheduler budget.
