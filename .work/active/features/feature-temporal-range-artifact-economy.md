---
id: feature-temporal-range-artifact-economy
kind: feature
stage: drafting
tags: [temporal, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Make temporal ranges and artifacts economical on busy captures

## Brief

The 2026-07-19 motion workload (dev build at v1.2.3-19, ~51 fps capture, zero gaps)
showed the temporal surface breaking down exactly on motion-heavy evidence — the
captures it exists for:

- **Default bundle artifacts refuse busy ranges.** A 7.6 s / 367-frame
  interaction-anchored bundle returned no storyboard ("resolved range exceeds the
  source-frame limit"); a 0.9 s / 44-frame range still failed both default
  generators with "no exact integer analysis scale fits configured limits", while
  a 0.7 s / 34-frame range succeeded and an explicit `scale: {down, factor: 7}` +
  `tile_limit: 8` succeeded **on the same 44-frame handle** (viewport 1673×1288).
  `fit_limits` appears to size against the full in-range frame count instead of
  the tile-limit selection.
- **No cheap way to resolve a range.** The verbose `range` object cannot be
  hand-authored (validation requires resolved frame ids), so the only path to a
  `range_handle` is `temporal_debug_bundle`, which always runs artifact
  generators the caller may not want — and their failures add noise to a call
  made only for the handle.
- **Source-frame listing hard-fails instead of paginating.** On a 367-frame
  handle, `list_source_frames` failed with "source read limits exceed runtime
  ceilings" and then "selected source frame count exceeds the request limit" —
  no truncated page, so frame ids in a busy range cannot be discovered at all.
  Region/filmstrip tools require a `source_frame_id`, making this a catch-22.
- **Region filmstrips normalize before cropping.** An 87-frame range for a
  293×70 region failed with "normalization result exceeds configured processing
  limits"; the budget is charged for full 1673×1288 frames rather than the
  requested region.
- **Generator boilerplate.** Direct `generate_artifacts` / filmstrip calls
  require every knob (~15 fields); the bundle already owns good defaults but the
  direct tools do not expose them as optional.
- **Timing plumbing.** Interaction timing fields are unit-less u64 nanoseconds
  (`dispatched_at: 16635305742`); building session-time windows around an
  interaction requires hand arithmetic. Interaction-window anchors
  (`before_ms`/`after_ms`) exist only on the bundle query.

Deliverables: a lightweight range-resolution path that returns a handle plus
capture quality without artifact generation; `fit_limits` sized to the actual
tile selection (or best exact-divisor fallback); paginated/truncated source-frame
listing with explicit omission accounting; region processing budgeted on the
cropped region; optional generator fields defaulting to the bundle's effective
defaults; and unit-explicit timing (field naming and/or ms-window ergonomics on
range-taking tools).

Absorbed backlog: `idea-temporal-artifacts-busy-range-limits`. Implementation via
peeragent Codex `gpt-5.6-luna` per operator decision (2026-07-19).

## Simplification opportunity

The bundle currently doubles as the de-facto range resolver; a first-class
resolve path may let its artifact stage stay strictly optional and shed the
"artifact failure on a handle-only call" noise. Registry-declared-surfaces means
one new tool declaration; check whether existing per-tool range plumbing can be
shared rather than duplicated across the six range-taking tools.
