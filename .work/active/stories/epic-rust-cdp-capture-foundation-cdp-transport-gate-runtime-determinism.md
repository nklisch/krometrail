---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-runtime-determinism
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make strict qualification deterministic and stage-diagnostic

## Reproduction

At exact revision `8d01d50956650befe603bd4178afbbb2ff473105`, hosted macOS run 29202075722 passed the exact-path candidate test and then immediately failed the same candidate contract in the gate with `Disconnected: Connection closed`. The simultaneous Linux run passed candidate setup but exhausted the complete 120-second hard stop without identifying the stuck stage. Neither run produced valid evidence.

## Scope

Reproduce with bounded short local gates and diagnose both paths. Eliminate candidate-contract race/order dependence between repeated runs and make scripted server shutdown/connection state deterministic. Add explicit stage context and bounded waits to every potentially blocking real-gate phase, especially screencast frame receive and lifecycle cleanup, so a hard stop identifies the active stage rather than replacing all gates with an undifferentiated failure. Preserve the 120-second global limit and existing per-gate thresholds.

## Acceptance criteria

- [ ] Repeated exact-path candidate contract runs are deterministic in one process and after the test suite; no stale task/server state can close a fresh connection.
- [ ] Every blocking qualification phase has a bounded operation and stage-specific failure; the global hard stop reports the active stage.
- [ ] Repeated short real-Chrome qualification runs complete or fail at a named stage within their declared bounds.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change lands and no evidence is fabricated.
