---
id: story-fix-navigation-viewport-observation-race
kind: story
stage: review
tags: [bug, browser, visual, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Restore viewport overrides before post-navigation observation

## Symptom

`navigate_page` and `go_forward` could succeed while returning a screenshot/layout rectangle from
Chrome's temporary native mobile layout rather than the declared 360x640 DPR 3 override.

## Root cause

The serialized supervisor cannot process queued navigation lifecycle events until the active
operation returns. Navigation completion therefore calls `observe_after_operation` before the
event-driven `RestoreViewport` effect can replay the acknowledged override.

## Fix approach

After navigation commit and before live observation, synchronously reapply and independently verify
the target's acknowledged viewport override. If restoration cannot be verified, preserve the proven
navigation success but return unavailable/degraded observation rather than contradictory evidence or
a replay-safe action failure. The later lifecycle replay remains an idempotent capture safeguard.

## Regression test

Focused navigation tests assert the complete override application and effective-metric verification
sequence used before post-navigation live observation. The navigation-success projection keeps a
restore failure inside unavailable observation rather than changing the committed outcome.

## Implementation notes

- Execution capability: host agent, high reasoning; this is a focused lifecycle ordering repair.
- Files changed: `crates/krometrail-cdp/src/control/navigation.rs`.
- Navigation success now synchronously replays and verifies an acknowledged override before live
  observation. A restore error becomes unavailable observation under the already proven success.
- Regression confirmation: `cargo test -p krometrail-cdp
  navigation_viewport_restore_is_verified_before_live_observation --locked` passes and asserts the
  complete metrics/touch/page-scale apply sequence followed by independent metric reads.
- The original public-site reproduction will be repeated against the refreshed plugin after release.
- Full workspace verification is deferred to the integrated patch pass.
