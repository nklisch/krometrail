---
id: release-1.6.2
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.6.2
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Release 1.6.2

Patch release resolving every friction surfaced by the 2026-07-23 v1.6.1
shakedown short of the parked giant-page transport investigation: error-shape
polish across the video, failed-response, and query surfaces; activation and
serving-version transparency; a benchmark-backed doubling of the analysis
budget; and the trim-signaling fixes proven necessary by a live small-budget
retention exercise.

## Bound items

- `story-video-limit-structured-refusal` — temporal-video limit refusals name
  the violated limit(s) with observed and limit values, carry both remedies
  and `retry: never`, and arrive as normal failed tool responses instead of a
  schema-mismatch wrapper. Validation moved from wire deserialization into the
  request constructor without weakening any check.
- `story-failed-response-error-detail` — failed responses render one
  single-line summary carrying the stable code, recovery, and retry advice at
  both summary sites and batch steps; stale snapshot-binding errors carry the
  canonical fresh-snapshot recovery.
- `story-query-nonactionable-hint` — `no_match` results report a bounded
  non-actionable match count derived from the already-acquired tree, so text
  present but non-actionable is distinguishable from text absent.
- `story-activation-version-transparency` — `browser_status` reports
  `server_version` at every detail tier; the plugin installer distinguishes
  the neutral not-yet-staged first activation from genuinely unsafe states,
  with hermetic fixtures pinning both (including a symlinked release
  directory failing strict, never masked as not-staged).
- `story-analysis-budget-tuning` — exhaustive analysis budget doubled to
  1536 MiB decoded within a 2 GiB combined request and a 240-frame ceiling;
  temporal-video source-frame cap raised to 480. Enabled by an O(log N)
  balanced PTS lookup replacing the linear nested-if filter, with an explicit
  ultrafast preset under the new `krometrail-ffmpeg-h264-v2` argument policy
  (re-qualification forced; v1-cached clips invalidated). Benchmarks on the
  release machine: 240-frame exhaustive difference_map 5.73 s against the
  15 s wall-time limit; 430-frame / 5.2 s clip encoded in 2.6 s against 30 s.
- `story-trim-signaling-visibility` — from the live 250 MB-budget retention
  exercise (parked as idea-trim-signaling-visibility-gaps): the transient
  grace-override flag is replaced by a sticky `grace_overridden_through`
  boundary that survives later clean reclaims, and fully-evicted range
  resolution failures keep `not_found` while naming the oldest retained
  boundary with anchor-forward recovery and `retry: never`.

## Gate runs

- Implementations by cross-model gpt-5.6-luna in two serialized jobs; one
  bounded fresh-context Claude Opus review across the batch (all six PASS, no
  material findings; two minors fixed, one measurement-harness note
  accepted).
- Host-run independent gate caught one semantic slip the implementation job
  reported green (fully-evicted refusal advising `after_recovery` retry on
  permanently evicted data); fixed to `retry: never` before review closure.
- Live verification: isolated small-budget trim exercise confirmed 1.6.1
  retention mechanics under sustained pressure (85% high-water exact,
  per-segment reclaim, capture uninterrupted, lone-instance census) and
  motivated the trim-signaling story.
- Full workspace gate green after every step (final: 75 suites, 0 failures,
  wire schemas verified, clippy `-D warnings`).

## Notes

- Origin: 2026-07-23 v1.6.1 full-surface shakedown minor frictions plus the
  live retention exercise. The shakedown's one material regression,
  `idea-giant-page-transport-session-kill` (giant-page CDP transport death
  ending the session), remains parked for its own investigation cycle and is
  not addressed by this release.
- No schema-incompatible persisted-format change: retained recording caches
  carry over. The ffmpeg argument-policy identity change re-runs video
  qualification on first startup and regenerates video cache entries; wire
  additions (`server_version`, `grace_overridden_through`,
  `non_actionable_match_count`) are the current single schema.
