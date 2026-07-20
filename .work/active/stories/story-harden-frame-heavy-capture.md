---
id: story-harden-frame-heavy-capture
kind: story
stage: done
created: 2026-07-20
updated: 2026-07-20
tags: [browser, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
---

Recheck capture acknowledgement and queue behavior during a frame-heavy GitHub issue search. In a temporary managed session with `every_nth_frame: 1`, navigating to `https://github.com/nklisch/krometrail/issues`, filling `Search Issues` with `viewport`, and pressing Enter produced 48 dropped frames out of 341 received and 54 known gaps. A temporal bundle around the search interaction retained five frames but crossed 37 gaps, so all three artifact outcomes were unavailable. Ordinary TodoMVC interaction and MDN scrolling in the same session produced usable gap-free evidence, making the heavy navigation path the useful stress reproduction.

## Acceptance

- Screencast acknowledgement remains prompt while capture geometry refresh is pending.
- A frame burst does not turn viewport-metadata uncertainty into dozens of false visual-evidence gaps.
- Deterministic stress coverage accounts exactly for received, acknowledged, accepted, dropped, persisted, and gap outcomes.

## Implementation notes

- Root cause: retained diagnostics showed zero acknowledgement or ingestion saturation gaps. All observed loss was `screencast_paused`: the frame reader acknowledged each frame, then discarded it while a geometry refresh waited behind the interaction on the supervisor.
- Fix: frames crossing an open geometry transition retain their pixels and last established geometry with `viewport_metadata_incomplete`. A committed refresh affects subsequent frames, and beginning/completing refresh no longer declares visual gaps.
- Regression: acknowledgement-spanning, unresolved-refresh, native-event, and a 12-frame burst test prove exact warning, geometry, counter, and zero-gap behavior. Genuine queue saturation remains separately covered.
- Verification: `cargo test -p krometrail-cdp --lib --locked capture::tests`.

## Bounded inline review — 2026-07-20

- Verdict: approved. The correction preserves immediate one-shot acknowledgement and genuine bounded-queue loss accounting while separating pixel availability from viewport-metadata confidence.
- Acceptance: transition frames persist with explicit warning, post-commit geometry is exact, and burst counters prove 13 received/acknowledged/accepted/persisted with zero drops and gaps.
