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

Keep failed geometry refreshes as explicit paused gaps while leaving the screencast stream active and fenced until a later authoritative refresh succeeds.

## Implementation notes

- Exhausted refreshes and deferred dispatch leave the transition active without setting `frame_envelope` failure state; every crossing frame remains acknowledged and is recorded as a paused gap instead of being persisted with stale geometry.
- A later geometry event redispatches the still-open transition, and only its successful authoritative observation commits replacement geometry.
- The last established geometry describes only frames before the transition; malformed screencast envelopes remain terminal at their existing boundary.
- Deterministic pipeline and runtime tests cover fenced acknowledged gaps, redispatch, active-state continuity, and later geometry recovery.
