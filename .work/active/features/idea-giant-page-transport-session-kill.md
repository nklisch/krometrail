---
id: idea-giant-page-transport-session-kill
kind: feature
stage: drafting
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-07-23
updated: 2026-07-23
tags: [cdp, bug]
---

Navigating to a giant single-page document (https://html.spec.whatwg.org/) on
v1.6.1 deterministically kills the whole browser session, where v1.6.0 produced
only a bounded observation failure with the session surviving. Reproduced twice
in the 2026-07-23 v1.6.1 shakedown (correlation ids
`c739732b-2e6c-46c9-baec-09df93c22d8b` and
`ef44ee90-05f3-4820-891e-182807e3175b`), Chrome 149.0.7827.155.

Observed sequence (identical both runs):

- `navigate_page` returns degraded: navigation itself succeeds (title/URL/layout
  metrics retrieved), but snapshot and screenshot report
  `browser_disconnected` "during the page observation command", and capture
  reports `capture_failed` at `frame_event_stream` (run 1) /
  `visibility_event_stream` (run 2).
- Log sequence: `browser.compositor.signal_unavailable`
  (compositor_readiness, page_observation_failed) →
  `capture.pipeline.failed` (frame_event_stream) →
  `capture.geometry_refresh.pending` (attempts: 5, browser_disconnected) →
  `reconnecting` → 5 × `browser.session.reconnect_attempt` →
  `browser.session.failed reconnect_exhausted` → session ended, Chrome shut
  down.
- Critical evidence: Chrome was ALIVE the whole time. systemd/journal shows the
  Chrome scope logging normally 9 seconds after the "disconnect" and its scope
  ending only when krometrail's own shutdown killed it ("Consumed 1.741s CPU,
  453.2M memory peak" — Chrome was not struggling; no OOM, 19GB free).
- So: the CDP websocket died mid-page-observation (consistent with an oversized
  message — e.g. a huge AX-tree/snapshot response — tripping a transport
  message/frame limit and aborting the websocket), and then five reconnect
  attempts against a live, responsive browser all failed before supervision
  gave up. The `capture.geometry_refresh.pending attempts: 5` entries suggest
  the pending geometry/observation refresh may re-fire the same lethal
  observation on reconnect, re-killing the transport each time.

Impact:

- The v1.6.1 `feature-ax-overflow-observation-failure` classified error path
  (honest stage + `SNAPSHOT_SCOPE_RECOVERY` + RetryAdvice::Never) is
  unreachable for this page class: the transport dies before classification can
  run, and the whole session (all tabs, capture history continuity) is lost —
  strictly worse than the 1.6.0 behavior the feature set out to improve.
- A single poison URL is a session-kill: an agent mid-investigation loses its
  browser, retained-capture continuity, and any un-pinned in-flight evidence.

Investigation directions (verify, do not assume):

- Find the actual transport death cause: cdpkit websocket max-message/frame
  limit vs read error vs ping timeout during the observation command issued
  while the page is still parsing (`readiness: loading`).
- Why 5 reconnects fail against a live browser — and whether the pending
  geometry refresh re-issuing the observation on re-attach makes reconnect
  self-defeating for this page class.
- Expected shape: observation command fails bounded (classified
  `page_observation_failed`, stage-honest), transport survives or reconnect
  succeeds, session continues, control keeps working — matching the intent of
  feature-ax-overflow-observation-failure.

Related: `feature-ax-overflow-observation-failure` (done, 1.6.1) — its
serialization-failure classification is untestable end-to-end until this is
fixed. Parked cdpkit byte-fingerprint idea is adjacent but distinct.

## Execution assessment — 2026-09-05

The [reliability execution topology](../../backlog/epic-a-grade-reliability.md#execution-graph)
places this existing item in the first session-survival queue at proposed P1
priority. Losing the entire browser session is more severe than losing one
observation. The report above is historical evidence, not a fresh reproduction
on the current revision. Reproduce with a bounded local giant-document fixture,
identify the actual transport/reconnect failure, and retain regressions before
choosing a fix. This assessment does not authorize implementation or establish
the oversized-message hypothesis as the cause.

## Authorized execution checkpoint — 2026-09-05

The user authorized continued reliability work. Astra medium owns a diagnosis-first checkpoint: reproduce with a bounded local giant-document fixture using isolated temporary browser/profile/store state; establish the actual command/transport/supervisor failure and why reconnect succeeds or fails before selecting a fix. Do not visit private/public target sites or use the user's reusable profile. Existing Chrome opt-in qualification and process cleanup authorities apply. No transport replacement, fork, raised limits, or blanket observation suppression based on the historical oversized-message hypothesis alone. Record exact versions, fixture size, wire/error evidence, regression shape, diagnosis confidence, narrow design, and remaining uncertainty in this item for parent adjudication. Source/probe/test changes are allowed for the investigation; hold production transport changes until the cause and design are reviewed.
