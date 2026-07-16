---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-runtime-determinism
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding]
release_binding: 1.0.0
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

- [x] Repeated exact-path candidate contract runs are deterministic in one process and after the test suite; no stale task/server state can close a fresh connection.
- [x] Every blocking qualification phase has a bounded operation and stage-specific failure; the global hard stop reports the active stage.
- [x] Repeated short real-Chrome qualification runs complete or fail at a named stage within their declared bounds.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change lands and no evidence is fabricated.

## Implementation notes

- Root cause of the Linux 120-second hang: `ScriptedCdpPeer::wait_for_command` could check its snapshot, then create its notification future after the command notification had already fired. The missed wake left the candidate lifecycle barrier pending. The notification future is now registered before the snapshot check.
- Root cause of the macOS “candidate test passed, gate immediately got Connection closed” class of failure: scripted server connection tasks were detached from server lifetime. The server now tracks accepted connection tasks, shuts down the listener and joins/aborts every connection deterministically, and the decisive helper uses that lifecycle for both success and failure paths. The same observed controller remains the only wire authority.
- Added `QualificationStage`/`StageTracker` diagnostics and stage-bound `SpikeError` values. The 120-second default hard stop is unchanged, but timeout errors now identify the active phase; local evidence from a bounded 30-second gate reported `ScreencastFrameReceive` rather than an anonymous global deadline.
- Added five-second phase bounds for real-Chrome transport operations, five-second receive/ack bounds for screencast work, two-second candidate-contract operation/subscription bounds, bounded disconnect closure waits, bounded rebuild startup/connection/session work, and bounded Chrome HTTP response reads. Removed only the accidental hard-coded 120-second screencast loop floor; default 60-second/1,000-frame thresholds and the 120-second global limit remain unchanged.
- Added regressions that repeat the exact candidate helper twice in one process and repeat the short real-Chrome gate twice when `/usr/bin/google-chrome` is available. The short gate uses 2 seconds/20 frames/20 attempts with a 30-second hard stop only as a local diagnostic; decisive validation still enforces the existing production thresholds.
- Regenerated `docs/evidence/cdp-transport/v2/schema.json` for the stage-aware error contract. No production/core files or evidence reports were changed or fabricated.
- Verification: `cargo fmt --all --check`; default workspace tests/clippy; `cdp-spike` tests/clippy; `cdp-spike-cdpkit` tests/clippy; repeated candidate helper and repeated short Chrome gate passed. A bounded CLI gate with the unchanged decisive thresholds and a 30-second diagnostic hard stop emitted schema-valid failure evidence naming `ScreencastFrameReceive`.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane runtime review verified notification registration before observation, deterministic server connection-task shutdown, stage-aware global errors, phase-bounded candidate/Chrome operations, repeated exact helper and short Chrome coverage, 25 candidate-feature tests, and denied-warning clippy. Thresholds remain unchanged and no evidence was accepted. Verdict: Approve - story verified by implement; fast-lane advance.

