---
id: release-1.6.0
kind: release
stage: released
tags: []
parent: null
depends_on: []
release_binding: 1.6.0
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Release 1.6.0

Minor release delivering the system-wide performance program: a
profiling-driven sweep of the four runtime-dominating entry points
(frame ingestion, retention accounting, the semantic-wait/snapshot query
pipeline, and the artifact pixel pipeline), with every change held to
byte-identical observable behavior.

## Bound items

- `feature-perf-store-ingestion-accounting` — frame appends drop from
  0.54–6.65 ms (growing with store size) to a flat ~120 µs: incremental
  usage accounting replaces per-append full-store scans, the per-append WAL
  checkpoint becomes a bounded policy under `synchronous=NORMAL` (index
  durability aligned with the segment layer's seal-time promotion; recovery
  reconciles the tail), eviction switches to set-based deletes under a new
  `kind='frame'` partial index (schema v13; 253 ms → low single-digit ms per
  segment), and interactive reads move to a read-only WAL connection pool,
  decoupled from write cadence. Removes the btrfs 47 fps capture ceiling
  against the ~50 fps screencast arrival rate. Review hardening: durability
  barriers for deletion staging, pins, and terminal catalog writes; gated
  accumulator deltas; single-snapshot availability reads; best-effort
  periodic checkpoints; crash-recovery and WAL-bound acceptance tests.
- `feature-perf-wait-snapshot-pipeline` — semantic-wait polls at the
  50k-node bound drop from 188 ms (above the 100 ms cadence) to 49 ms:
  probe text matching is pre-normalized and allocation-free with the
  relaxed rescan fused into a single evaluation (55 → 8.6 ms), container
  queries memoize ancestor verdicts (16 → 5.2 ms), and a typed serde AX
  decoder replaces `Value` traversal (68 → 34 ms) with tolerant polymorphic
  decoding and last-wins duplicate-id traversal preserving the previous
  decoder's exact behavior. The transport-seam short-circuit for ~1–5 ms
  quiescent polls remains parked (`idea-cdpkit-byte-fingerprint-hook`).
- `feature-perf-artifact-pixel-pipeline` — the 4-artifact identity suite
  drops from 9.3 s to 2.08 s single-worker (deterministic scoped
  parallelism scales further): adjacent-pair classification is computed
  once per cohort and shared across generators (cohort keys carry the
  ordered sampled-frame identity; consumers validate and fail closed),
  the per-pixel classifier is a hoisted-u64 row-based loop with a proved
  no-overflow bound, and region filmstrips normalize only selected tiles
  (389 → 135 ms, fixing the 120-frame retained-bytes failure). All
  artifacts, manifests, and output hashes remain byte-identical.

## Gate runs

- Discovery by four parallel measurement agents writing release-mode
  profiling drivers (wall-clock, perf counters, EXPLAIN QUERY PLAN,
  fsync/syscall accounting); designs by fresh-context Opus sub-agents;
  implementations and review fixes by cross-model gpt-5.6-luna; one
  cross-model gpt-5.6-sol static review per feature.
- Review findings fixed and re-verified: five store durability edges, a
  live-page snapshot-decoder strictness regression, and an under-keyed
  shared-analysis cohort — each pinned by new regressions.
- Full workspace gate green after every step: fmt, wire-enum schema check,
  check, tests (76 suites, 1,309 tests), clippy `-D warnings`.

## Notes

- Schema v13: incompatible retained recording caches are cleared and
  re-initialized on first open; configuration, managed browser profiles,
  and diagnostics survive (Current Contract Discipline).
- Parked follow-ups: `idea-cdpkit-byte-fingerprint-hook` (transport-seam
  quiescent-poll short-circuit); RGB16-linear evidence-format decision
  recorded in the pixel feature body as a strategic contract question.
