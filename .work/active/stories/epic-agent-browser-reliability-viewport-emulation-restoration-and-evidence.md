---
id: epic-agent-browser-reliability-viewport-emulation-restoration-and-evidence
kind: story
stage: implementing
tags: [browser, agent-ux, visual]
parent: epic-agent-browser-reliability-viewport-emulation
depends_on: [epic-agent-browser-reliability-viewport-emulation-public-contract]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Restore viewport state and preserve geometry-transition evidence

## Checkpoint

Retain acknowledged target overrides in the single-writer supervisor, restore them before capture
on reattachment, and prove source-frame geometry/temporal epoch behavior remains honest. This
checkpoint owns the lifecycle/evidence portion of GitHub issue #10.

## Acceptance evidence

- [ ] Same-target navigation preserves the override without reissuing commands; same-key reconnect
      restores it before domain/capture effects.
- [ ] Restore failure fails only the affected target before capture starts, while clear/new/closed
      targets retain no stale override.
- [ ] Capture remains continuous across runtime changes and stores each frame's own geometry/scale
      with no inferred normalization or artificial gap.
- [ ] Temporal artifacts split incompatible geometry epochs, and foundation/skill guidance explains
      override, clear, effective metrics, target scope, and epoch interpretation.

## Ordering and blocker

Depends on the public contract checkpoint. Restore ordering must be verified before the feature can
enter review because starting capture under un-restored geometry would make retained evidence
misleading.
