---
id: epic-browser-interface-hardening-runtime-reliability-capture-refresh
kind: story
stage: done
tags: [browser, visual]
parent: epic-browser-interface-hardening-runtime-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Recover from geometry refresh gaps

Complete failed geometry refreshes as explicit paused gaps while keeping the screencast stream active on the last established geometry and allowing later recovery.

## Implementation notes

- `abandon_geometry_transition` now completes an exhausted refresh as a `ScreencastPaused` gap without setting `frame_envelope` failure state.
- The geometry fence still rejects frames crossing an unproven transition, and malformed screencast envelopes remain terminal at their existing boundary.
- Runtime, reconnect, and rollback integration retain the last established geometry until a later refresh commits a replacement.
- Deterministic pipeline and runtime tests cover abandoned-gap acknowledgement, active-state continuity, and later geometry recovery.
