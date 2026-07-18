---
id: story-fix-navigation-geometry-refresh
kind: story
stage: done
tags: [bug, browser, visual]
parent: null
depends_on: []
release_binding: 1.0.5
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Retry navigation-time capture geometry refresh

## Symptom

Navigating to Wikipedia under a mobile viewport override reproducibly moved retained capture to
`failed` at `frame_envelope`, while current control continued and comparable MDN/HTTPBin navigation
remained healthy.

## Root cause

`Page.frameNavigated` fences capture geometry and asks the session supervisor to observe the new
effective viewport. A single transient CDP failure while the replacement document is becoming
available returns `false`; the supervisor immediately fails the entire capture stream at
`frame_envelope` even though the target and its declared viewport remain valid moments later.

## Fix approach

Retry the read-only geometry observation within a short bounded settling window. Frames remain fenced
and become explicit gaps during the unproven transition; a persistent failure still terminates only
the capture stream under the existing contract.

## Regression test

`crates/krometrail-cdp/src/session/mod.rs` scripts one failed `Page.getLayoutMetrics` call followed by
valid mobile geometry and asserts the transition commits rather than failing capture. It failed on the
first transient response before the fix; the existing persistent-failure assertion remains.

## Implementation notes

- Execution capability: host agent, high reasoning; this touches the capture-provenance boundary and
  preserves its fail-closed behavior while admitting transient document replacement.
- Files changed: session geometry refresh and its generation-scoped capture test.
- Confirmation: the transient-failure regression failed before bounded retry and passes afterward;
  the persistent-failure path still reaches failed capture after five attempts, and the exact gap
  count includes the additional fenced transition. Live Wikipedia qualification is deferred to the
  integrated browser pass.
- Adjacent issues parked: none.

## Review

- **Mode:** bounded inline standalone-story review; no independent or cross-model reviewer ran.
- **Verdict:** approve.
- **Correctness:** geometry remains fenced until independently observed, transitional frames remain explicit gaps, and persistent failure still terminates capture. The bounded retry admits only transient document-replacement failures.
- **Tests:** the transient-failure regression and existing persistent-failure contract both pass, as do the complete workspace suite and real-Chrome viewport/navigation qualification.
- **Design and compatibility:** the retry is local to read-only geometry observation, bounded to 200 milliseconds, and preserves the stable retained-evidence and failure contracts.
- **Security:** no external input validation, privileges, filesystem paths, network destinations, secrets, or privacy boundaries changed.
- **Findings:** no blockers, important findings, or nits.
