---
id: runtime-observation-hardening-capture-acknowledgements
kind: story
stage: done
tags: [browser]
parent: runtime-observation-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Keep capture acknowledgements healthy during frame-heavy navigation

Align the production acknowledgement deadline with the qualified transport maximum, preserve immediate one-shot acknowledgement before bounded handoff, and expose privacy-bounded failure reason, deadline, elapsed time, and pipeline counters. Deterministic and real-Chrome evidence must cover the observed frame-heavy failure shape.

## Implementation notes

- Execution capability: inline implementation; one capture reader and its evidence fixtures form a cohesive boundary.
- Review weight: standard (project default).
- Root cause: the production default acknowledgement deadline was 250ms even though the qualified cdpkit transport envelope permits up to 1000ms; frame-heavy CDP multiplexing could exceed the lower deadline and terminally stop capture.
- Files changed: `crates/krometrail-cdp/src/capture/mod.rs`, `capture/pipeline.rs`, `capture/tests.rs`, the nested-frame real-Chrome qualification in `tests/verified_interactions.rs`, and current cross-platform smoke sample/schema/validator files. Committed Linux/macOS run evidence retains its observed 250ms configuration unchanged.
- Implementation: the default is now one second. The reader still performs one synchronous acknowledgement before ordinal allocation, parsing, or bounded handoff and never retries. All invalid-token, transport-error, and deadline failures flow through one helper that declares one acknowledgement gap and logs `capture.ack.failed` with categorical reason, deadline/elapsed nanoseconds, opaque lifecycle identities, and bounded received/acknowledged/accepted/dropped/persisted/gap/queue counters.
- Tests added/changed: default configuration asserts 1s; a held acknowledgement delayed 300ms succeeds before handoff; a 20ms configured deadline fails terminally after one command and one explicit gap; source checks protect the bounded log fields. The current synthetic smoke sample projects 1000ms while the validator accepts both truthful 250ms historical evidence and the 1000ms current default.
- Verification: `cargo test -p krometrail-cdp capture::tests --locked` (35 passed); `cargo test -p krometrail-cdp --test cross_platform_smoke deterministic_ --locked` (9 passed); `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test verified_interactions opt_in_real_chrome_qualifies_frame_actions_staleness_and_bounded_assets --locked -- --nocapture` (passed with persisted 3→5 and received=acknowledged=11).
- Simplification: centralized all acknowledgement terminal failures in one gap/log/fail path; no retry, alternate reader, or parallel timeout authority was introduced.
- Discrepancies from design: none.
- Adjacent issues parked: none.
