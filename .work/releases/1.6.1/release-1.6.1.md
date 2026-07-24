---
id: release-1.6.1
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.6.1
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Release 1.6.1

Patch release fixing every defect and friction surfaced by the v1.6.0
full-surface shakedown: retention trim correctness and transparency, honest
giant-DOM observation failures, temporal-video qualification on mainstream
FFmpeg builds, and two agent-surface polish items.

## Bound items

- `feature-retention-trim-transparency` (5 child stories) — the in-session
  budget-halving bug is fixed: the census fallback is now the last successfully
  proven live-instance count instead of a monotonic maximum that latched a
  transient startup overlap (SPEC updated to the current contract). The
  15-minute artifact grace is now real: the reclaim walk skips in-grace
  artifacts and their backing segments and reclaims the next-oldest instead,
  overriding only when nothing else is reclaimable — and that override is
  causally bound to the operation that forced it and surfaced as a response
  warning. `browser_status` reports effective budget, live instance count, and
  trim state derived from one census observation; temporal range responses
  (including `temporal_debug_bundle`) carry a calm trimmed-through note whose
  boundary is scoped to the response's own session/target. Range-bound capture
  status is bound-honest ({state, established_at, generation}) instead of
  echoing the session-initial zero block. Store-level end-to-end reclaim
  regressions pin the skip-ordering, emergency override, and trim-exhausted
  latch; pins, the unified walk, durability barriers, set-based eviction, and
  usage-accumulator gating are unchanged.
- `feature-ax-overflow-observation-failure` — pages whose accessibility tree
  exceeds what Chrome will serialize (one-page WHATWG HTML spec class) now fail
  with a classified `page_observation_failed` error: honest stage-based
  explanation, the shared frame-scoped recovery guidance, `RetryAdvice::Never`,
  and one bounded `observation.serialization.failed` diagnostics event per
  failed command. Disconnects keep the `browser_disconnected` boundary; wait
  polling emits no log storm; screenshot/layout/malformed paths untouched.
- `feature-temporal-video-qualification` — temporal video now qualifies on
  FFmpeg builds whose mov muxer writes the terminal held-sample duration as 0
  (Fedora 7.1.2 class): the validator accepts the muxer-defined terminal stts
  delta while every other structural, codec, dimension, timescale, and duration
  check stays exact. Qualification diagnostics name the failed check with
  bounded expected/observed values via a single Mp4Check registry. New
  terminal-hold-zero fixture with recorded provenance.
- `story-source-frames-concise-page` — concise `list_source_frames` pages keep
  rows, continuation offset, and omission counts but no longer publish one
  resource link per frame; expanded/full retain them.
- `story-exhaustive-cap-structured-error` — the exhaustive-sampling cap refusal
  carries the structured error shape: exact plan numbers, recovery naming both
  remedies (narrower range or uniform_bounded), and diagnostics correlation.

## Gate runs

- Designs by fresh-context Opus sub-agents (retention and temporal-video
  designs each included empirical root-cause verification); implementations and
  review fixes by cross-model gpt-5.6-luna; one cross-model gpt-5.6-sol static
  review per feature and a bounded fresh-context review for the standalone
  stories.
- Review findings fixed and re-verified: five retention materials (scoped trim
  boundaries, bundle warnings, causally-bound grace override, single-census
  status, SPEC census-fallback drift) plus an end-to-end reclaim regression
  gap; three AX-overflow minors fixed in-cycle; temporal-video approved with
  zero findings.
- Full workspace gate green after every step (final: 75 suites, 0 failures,
  clippy `-D warnings`).

## Notes

- Origin: all items trace to the 2026-07-23 v1.6.0 full-surface shakedown
  (parked as idea-ax-overflow-opaque-failure,
  idea-silent-trim-evicts-fresh-artifacts,
  idea-temporal-video-qualification-failure, plus three unfiled minor notes).
- No schema-incompatible persisted-format change: retained recording caches
  carry over; the retention status and capture-status wire additions are the
  current single schema.
