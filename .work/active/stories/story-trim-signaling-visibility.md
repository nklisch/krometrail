---
id: story-trim-signaling-visibility
kind: story
stage: implementing
tags: [temporal, mcp]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Trim signaling visibility fixes

## Brief

Promoted from `idea-trim-signaling-visibility-gaps` (verified live in the
2026-07-23 small-budget trim exercise; full evidence and log references in the
backlog item). The 1.6.1 retention mechanics work; two signaling gaps defeat
the transparency intent:

1. `grace_override_active` is an instantaneous latch
   (`crates/krometrail-store/src/recording.rs:781-799`) cleared by the next
   non-override reclaim and by every below-high-water trim entry — a
   background capture-triggered override (the common case) was cleared 95 ms
   after it fired and never observable via `browser_status`. Twelve overrides
   in one pass read `false` at every poll.
2. A range resolved entirely behind the trim boundary fails as bare
   `not_found` with no boundary reference — the exact "surprise" the
   trimmed-through signaling was designed to prevent.

## Direction

- Replace the transient boolean with a sticky, boundary-anchored status fact:
  the store records the newest session-time through which artifact grace has
  been overridden this process (and optionally an override count);
  `browser_status` reports it calmly (e.g. `grace_overridden_through` beside
  `trim_state`), surviving until the process ends or the boundary is
  surpassed by normal retention. Keep the causally-bound per-response warning
  for operation-triggered overrides unchanged. Update SPEC if it documents the
  transient flag. Single current schema — replace the field, no dual shape.
- The no-retained-evidence resolve failure (fully-evicted range) carries the
  oldest-retained boundary and the in-session-trimming fact in its structured
  error message and recovery ("evidence before <t> was reclaimed by in-session
  retention; anchor at or after <t>"), same calm voice as the surviving-range
  trimmed-through note. Keep the stable error code semantics (`not_found`
  unless the contract argues otherwise) — the fix is context, not category.
- Wire schema changes regenerate; `bash scripts/check-wire-enum-schemas.sh`
  green.

## Acceptance criteria

- [ ] A capture-triggered grace override remains visible in `browser_status`
      at any later poll in the same process (pinned by store-level test
      simulating the exercise ordering: override then clean reclaim).
- [ ] Fully-evicted range resolution failure names the oldest retained
      boundary and in-session trimming in message/recovery; pinned by test.
- [ ] Surviving-range trimmed-through note and operation-bound override
      warning behavior unchanged.
- [ ] Schemas regenerated if shape changed; full workspace gate green.
