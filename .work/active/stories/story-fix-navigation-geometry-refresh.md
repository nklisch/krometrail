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
effective viewport. The refresh originally failed on one transient CDP read. After adding bounded
retry, exact live diagnostics exposed the deeper path: navigation can reset Chrome's mobile page
scale, so every observation truthfully disagreed with the still-declared target override. The
refresh never replayed that declared state and failed the capture stream at `frame_envelope`.

## Fix approach

Retry geometry observation within a short bounded settling window. If Chrome reports that a declared
override is no longer applied, replay that same target-scoped override once and independently observe
it again before committing capture geometry. Frames remain fenced and become explicit gaps during the
unproven transition; a persistent failure still terminates only the capture stream.

## Regression test

`crates/krometrail-cdp/src/session/mod.rs` covers both a failed `Page.getLayoutMetrics` call followed
by valid geometry and a navigation-style 980px mismatch followed by exact replay of the declared
360x640 mobile override. Both transitions commit; the existing persistent-failure assertion remains.

## Implementation notes

- Execution capability: host agent, high reasoning; this touches the capture-provenance boundary and
  preserves its fail-closed behavior while admitting transient document replacement.
- Files changed: session geometry refresh, privacy-safe capture rejection diagnostics, and
  generation-scoped capture tests.
- Confirmation: transient read and navigation-reset regressions pass; the persistent-failure path
  still reaches failed capture after five attempts, and the exact gap count includes every fenced
  transition. The local candidate applied a 360x640 mobile viewport, navigated to Wikipedia, returned
  no operation warnings, and remained `capturing` with 23 persisted frames.
- Adjacent issues parked: none.

## Review

- **Mode:** bounded inline standalone-story review; no independent or cross-model reviewer ran.
- **Verdict:** approve.
- **Correctness:** geometry remains fenced until independently observed, replay uses only the already-declared target override, transitional frames remain explicit gaps, and persistent failure still terminates capture.
- **Tests:** transient-read, navigation-reset replay, and persistent-failure paths pass. The exact local MCP Wikipedia reproduction now remains healthy after mobile navigation.
- **Design and compatibility:** replay is attempted once only after an observed declared-metrics mismatch, the settling loop remains bounded to 200 milliseconds, and stable viewport, retained-evidence, and failure contracts are preserved.
- **Security:** no external input validation, privileges, filesystem paths, network destinations, secrets, or privacy boundaries changed.
- **Findings:** no blockers, important findings, or nits.
