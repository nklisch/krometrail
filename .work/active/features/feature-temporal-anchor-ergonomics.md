---
id: feature-temporal-anchor-ergonomics
kind: feature
stage: drafting
tags: [agent-ux, visual, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Temporal anchor ergonomics

## Brief

Small temporal request-shape frictions from GitHub issue #14 finding #10 and
the 2026-07-21 v1.4.0 local shakedown. (Distinct from the completed
`feature-temporal-request-ergonomics`, which covered the 2026-07-19 schema
frictions.)

1. **`query.anchor` nesting is non-obvious (issue #14).** A temporal request
   only worked once its anchor was nested under `query.anchor`; the schema and
   examples made the flatter shape appear plausible. Improve the schema
   descriptions/examples — or accept the direct shape via input
   canonicalization — so the first plausible call succeeds.
2. **`resolve_temporal_range` echoes the default implicit window when an
   explicit window was applied (local shakedown).** With an explicit
   `window: {before_ms: 1500, after_ms: 1500}`, the resolved bounds correctly
   used the explicit window, but the response echoed
   `options.implicit_interaction_window: {after_ms: 250, before_ms: 150}`.
   Arguably correct ("implicit" is the fallback label), but the echo reads as
   the applied window. Make the echoed options unambiguous about which window
   governed the resolved bounds.

The confirmed `latest_interaction` bundle failure (issue #14 finding #6,
independently reproduced in the local shakedown) is promoted from backlog as
child story `feature-temporal-anchor-ergonomics-latest-interaction-bundle`,
implementable immediately.

## Simplification opportunity

None identified; each item is a bounded clarity or canonicalization fix.

## References

- GitHub issue #14, findings 6 and 10 (anchor nesting).
- 2026-07-21 v1.4.0 shakedown (implicit-window echo; latest_interaction repro).
