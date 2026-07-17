---
id: epic-agent-browser-reliability-viewport-emulation-restoration-and-evidence
kind: story
stage: done
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

- [x] Same-target navigation preserves the override without reissuing commands; same-key reconnect
      restores it before domain/capture effects.
- [x] Restore failure fails only the affected target before capture starts, while clear/new/closed
      targets retain no stale override.
- [x] Capture remains continuous across runtime changes and stores each frame's own geometry/scale
      with no inferred normalization or artificial gap.
- [x] Temporal artifacts split incompatible geometry epochs, and foundation/skill guidance explains
      override, clear, effective metrics, target scope, and epoch interpretation.

## Ordering and blocker

Depends on the public contract checkpoint. Restore ordering must be verified before the feature can
enter review because starting capture under un-restored geometry would make retained evidence
misleading.

## Implementation evidence

- The single-writer target state stores only acknowledged overrides. Reattachment emits an exact
  target/session/generation-bound restore before capture resume; the reconnect transaction applies
  metrics and touch before staging capture effects.
- Restore failure removes queued target-local domain/capture effects and reduces through the
  existing target-attach-failed path. Clear and newly keyed targets retain no override.
- Reducer tests prove restore-before-resume ordering and absence of stale restoration after clear or
  target replacement. Broader live-Chrome geometry/capture qualification and final public/skill
  guidance remain assigned to the root integration gate, so this checkpoint remains implementing.
- A capture-stream regression test sends two consecutive screencast frames with 1280x720@1 and
  390x844@3 metadata through one coordinator. It proves strict ordinals `[1, 2]`, no declared gap,
  capturing state throughout, per-frame geometry/scale, and preservation of a missing-source-time
  warning on the second frame.
- A scale-only artifact-service regression test proves unchanged image/viewport dimensions with a
  device-scale transition partition into two `VisualEpoch` results instead of implicit source
  normalization.
- The opt-in real-Chrome qualification passes responsive CSS, navigation persistence, clear, and
  target isolation. The effective-size authority is CDP `cssVisualViewport`; page `innerWidth` can
  be content-expanded under mobile emulation and is not used for contract validation. Clear sends
  `enabled:false` without invalid `maxTouchPoints:0`.
- Verification: capture transition and scale-only epoch tests pass; the macOS live qualification
  passes under `KROMETRAIL_REAL_CHROME_TESTS=1`; foundation and skill guidance are current.
