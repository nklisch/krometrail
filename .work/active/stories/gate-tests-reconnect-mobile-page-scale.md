---
id: gate-tests-reconnect-mobile-page-scale
kind: story
stage: review
tags: [testing, browser]
parent: null
depends_on: []
release_binding: 1.0.3
gate_origin: tests
created: 2026-07-17
updated: 2026-07-17
---

# Protect mobile page-scale restoration across reconnect

## Priority

High

## Value evidence

Item: `story-fix-live-mobile-viewport-override`. The stable viewport contract restores an acknowledged override before capture resumes after reconnect. The new page-scale replay and its target-local failure path need direct protection.

## Suggested test

Assert reconnect command order includes `setDeviceMetricsOverride`, touch emulation, `setPageScaleFactor(1)`, then capture restoration; an injected page-scale failure must fail only the affected target.

## Implementation notes

- Execution capability: inline; one focused Rust test seam.
- Review weight: standard by project default.
- Files changed: `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/reconnect.rs`.
- Tests added: reconnect replays page scale before capture and fences page-scale failure to the affected target.
- Simplification: reused the transactional reconnect staging boundary and existing controlled transport.
- Discrepancies from design: the session tests live in `session/mod.rs`, not the scanner's suggested nonexistent `session/tests.rs`.
- Adjacent issues parked: none.
