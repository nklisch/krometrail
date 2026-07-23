---
id: feature-retention-trim-transparency-capture-status-echo
kind: story
stage: implementing
tags: [store]
parent: feature-retention-trim-transparency
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# capture_status echo honesty: report state at range bounds, not frozen counters

## Checkpoint

Unit 5 of the parent feature — the folded-in `resolve_temporal_range`
`capture_quality.capture_status` bug. Independent of the retention stories; keep
its schema regen off the same beat as the status-transparency story. Design in the
parent body (`## Architectural choice` → "capture_status echo",
`## Implementation Units` → Unit 5).

`crates/krometrail-core/src/timeline/context.rs`:

- Replace the counter-bearing bound projections with a bound-honest
  `CaptureStatusBound { state, established_at, attachment_generation }` for
  `at_range_start` / `at_range_end`; keep the full `CaptureStatusPoint` list in
  `transitions` (each honest at its own time).
- In `capture_status_evidence`, build the bounds from the establishing transition's
  state / session_time / attachment_generation. Do not synthesize or interpolate
  counters — the store retains transitions only, so omitting the frozen snapshot is
  the honesty fix. Ordering checks and `CaptureQualityWarning` logic unchanged.
- Update the MCP `capture_status` projection and any schema.rs assertion reaching
  into `at_range_start.status`.

## Done when

- A range well into a steady-capturing session (only transition is session-start
  Idle→Capturing at ~11 ms) reports `at_range_start.state == Capturing`,
  `established_at == 11 ms`, and no counter snapshot — the all-zero echo is gone.
- A range spanning a mid-session Capturing→Paused transition reports it in
  `transitions` and the correct bound state at each end.
- `bash scripts/check-wire-enum-schemas.sh` clean; no fabricated stats.

## Implementation notes

- Replaced the counter-bearing range-bound projections with exported
  `CaptureStatusBound { state, established_at, attachment_generation }` while
  retaining full `CaptureStatusPoint` transitions. The context fixture pins
  both bound timestamps/generations and verifies serialized bounds contain no
  frozen status counters.
- Test: `range_context::context_derives_exact_capture_quality_gaps_warnings_and_status`.
- Verification: the E schema checkpoint (`bash scripts/check-wire-enum-schemas.sh`
  plus generated MCP schema tests) passed, followed by the final locked
  workspace gate.
