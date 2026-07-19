---
id: feature-temporal-range-artifact-economy
kind: feature
stage: done
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

## Explorer map (verified file:line)

- Resolver: `crates/krometrail-core/src/timeline/range.rs` —
  `TemporalRangeResolver::resolve` L965-975 (`seed` L977-1181, `finalize`
  L1203-1308); `ResolvedRange::validate` L662-750 (empty-frames rejection
  L667-671; custom Deserialize revalidates L753-792 — hand-built ranges cannot
  bypass it). `InteractionWindow` (before_ms/after_ms, u64 whole ms, ≤120 s)
  L82-150; implicit window 150/250 ms `RangeResolutionOptions::DEFAULT`
  L171-179.
- Handles: `ProcessResolvedRangeHandles` `/src/range_handles.rs` L12-110
  (register L55-91 with value dedup + 4096/16 MiB budget from
  `krometrail-core/src/range_handle.rs`; resolve_available L93-110). MCP:
  `resolve_range_argument` `krometrail-mcp/src/registry.rs:697-723`,
  `range_handle_for_request` L725-734, handle-capable tool list
  `progressive_accepts_range_handle` L348-359; `temporal_debug_bundle` accepts
  only the natural-anchor query and mints the handle in `call_bundle`
  L486-540 (register at L516-517).
- Bundle service is a strict 7-step sequence `/src/debug_bundle/service.rs`
  L85-315: step 2 `resolve_range` (L120-125, via the `TemporalQuery` port,
  `krometrail-core/src/timeline/query.rs:70-71`) is independent of artifact
  steps 4-7 — a resolve-only path can stop after capture-quality/context
  assembly.
- Default generators `/src/debug_bundle/policy.rs`: `default_generators`
  L71-79 (storyboard + difference_map), `storyboard_request` L81-100
  (tile 8, noise 512, FitLimits, 1920×2048/16 MiB), `difference_map_request`
  L102-118 (8192×8192/64 MiB), `default_artifact_request` L54-65
  (AllowPartial).
- FitLimits: `/src/artifacts/generators.rs` — `materialize_effective_scales`
  L311-330 (RegionFilmstrip excluded L321), `fit_scale` L332-363 tries factors
  [1,2,4,8] only, budget = `max_combined_request_bytes − epoch.decoded_bytes −
  reserved_output_bytes`; `validate_normalized_limit_with_budget` L373-396
  charges `pixels × 6 × epoch.frames.len()` — **all in-range frames**, while
  storyboard tile selection happens later in temporal-vision
  (`filmstrip.rs:462` `select_indices(..., tile_limit)`). Range-wide gate
  "resolved range exceeds the source-frame limit" `/src/artifacts/epoch.rs`
  L126-128 (`AdaptationLimits.max_source_frames`), charged before any
  selection; `decoded_bytes` for all frames L238-242.
- Source listing: `crates/krometrail-core/src/progressive.rs` — ceilings
  L28-30 (`MAX_SOURCE_READ_FRAMES=64`, 32 MiB item, 256 MiB total);
  "exceed runtime ceilings" L396-401; "selected source frame count exceeds
  the request limit" L629-633; `SourceFrameSelection` L535-576 offers only
  ResolvedOrder | Ids — **no offset/cursor machinery exists for frames**.
- Region filmstrip: `/src/progressive/region.rs` `prepare_region` L18-163;
  plan `crates/temporal-vision/src/filmstrip.rs:434-487`. The explorer read
  says the normalization budget (`normalize.rs` L256-324, error L397-402)
  charges cropped output pixels and that RegionFilmstrip bypasses
  `normalize_sequence` — yet the live run saw "normalization result exceeds
  configured processing limits" from an 87-frame 293×70-region request. This
  discrepancy is unresolved; Unit 2 must reproduce and locate the actual
  full-frame charge.
- Timing: `InteractionTiming` (`krometrail-core/src/browser/control.rs`
  L207-251) serializes `SessionTime` = u64 nanoseconds
  (`krometrail-core/src/time.rs:30-40`).
- Schemas: MCP tool schemas are generated live from wire types
  (`registry.rs`/`schema.rs`, integrity gate `validate_route_registry`
  L258-334) — no checked-in MCP schema artifacts to regenerate; the
  digest-verified canonical artifacts belong to temporal-evaluation only.
- SPEC roll-forward targets: "Temporal Ranges" L350-369, "Temporal Queries"
  L371-391.

## Design decisions

- **New `resolve_temporal_range` tool** with the bundle's natural-anchor query
  shape (anchor + retention + capture_gaps), returning the range summary,
  minted `range_handle`, and capture-quality block — no artifacts, no browser
  events. Modeled as call_bundle stopping after resolution + capture quality;
  the bundle keeps minting handles too (no behavior change there).
- **Storyboard budget follows selection**: fit budgeting and decode for the
  storyboard are bounded by its tile selection (min(frames, tile_limit)),
  keeping `omitted_frame_count` provenance; the difference map remains
  exhaustive by semantics — on busy ranges it may stay unavailable with an
  honest sized error (message quality handled by
  feature-actionable-failure-surface). The epoch-level `max_source_frames`
  gate moves per-generator so an exhaustive generator's refusal cannot zero
  out a selection-bounded one.
- **Listing paginates instead of refusing**: `resolved_order` selection gains
  an optional `offset`; over-limit listings truncate to `max_frames` with an
  explicit omitted count and `next_offset`. `fetch_source_frames` stays
  strict (explicit ids or in-limit resolved order) — fetching bytes is the
  expensive path; listing metadata is the discovery path.
- **Generator knobs become optional** with serde defaults equal to the bundle
  policy values; the default constants move to one shared home so policy and
  wire defaults cannot drift (single source of truth).
- **Timing stays u64 nanos on the wire**; `SessionTime`'s generated schema
  gains an explicit "session-relative monotonic nanoseconds" description and
  SPEC says it plainly. No field renames (code economy; ms ergonomics arrive
  via the resolve tool's before_ms/after_ms windows).

## Implementation Units

### Unit 1: resolve_temporal_range
**Files**: `crates/krometrail-mcp/src/registry.rs`, `/src/debug_bundle/`
(shared resolve + capture-quality assembly), core operation declaration
alongside the bundle/video operations, `docs/SPEC.md`
**Story**: `story-temporal-resolve-range`

**Acceptance Criteria**:
- [ ] Tool resolves all anchor kinds, returns handle + resolved range +
      capture quality (frame count, cadence, gaps, retention warnings), no
      artifact outcomes, no events.
- [ ] Returned handle works in every handle-capable tool (existing tests
      extended with one round-trip).
- [ ] SPEC Temporal Ranges/Queries sections describe the tool.

### Unit 2: Artifact budgets follow generator consumption
**Files**: `/src/artifacts/{generators.rs,epoch.rs}`,
`crates/temporal-vision/src/{filmstrip.rs,normalize.rs}`
**Story**: `story-temporal-artifact-budgets`

**Acceptance Criteria**:
- [ ] Regression: 44-frame 1673×1288 epoch, storyboard tile_limit 8,
      FitLimits → storyboard generates (the live failure case).
- [ ] 367-frame range: bundle default returns an available storyboard;
      difference map may be unavailable with a sized error.
- [ ] Region filmstrip on an 87-frame range with a small region succeeds;
      the observed full-frame normalization charge is reproduced first, then
      fixed where it actually lives.

### Unit 3: Listing pagination, generator defaults, timing description
**Files**: `crates/krometrail-core/src/progressive.rs` (+ its wire tests),
`/src/debug_bundle/policy.rs` + shared defaults home,
`crates/krometrail-core/src/time.rs`, `docs/SPEC.md`
**Story**: `story-temporal-listing-and-defaults`

**Acceptance Criteria**:
- [ ] `list_source_frames` on a 367-frame handle returns the first page with
      omitted count and `next_offset`; offset paging reaches the tail.
- [ ] Direct storyboard request with only `generator` specified uses the
      bundle defaults; explicit values still win.
- [ ] SessionTime schema description states nanoseconds.

## Implementation Order
1. Unit 1 (resolve tool — unlocks workflow value immediately)
2. Unit 2 (budgets)
3. Unit 3 (pagination/defaults)

## Testing
- Regression tests mirror the live failures (44-frame fit, 367-frame listing,
  87-frame region filmstrip) as deterministic fixtures with synthetic frame
  metadata; no real-chrome tier required.

## Risks
- The region-filmstrip discrepancy (explorer read vs observed failure) may
  reveal a second budget site; Unit 2's reproduce-first ordering contains it.
- Moving default constants must not change bundle behavior byte-for-byte
  (existing bundle tests are the guard).

## Implementation Notes

- Unit 1 delivered inline with the standard implementation capability: added the
  registry-declared `resolve_temporal_range` operation, reused range resolution
  and capture-quality assembly, minted a range handle, and deliberately skipped
  artifact and browser-event work. Existing range-handle routing remains the
  authority for follow-up tools.
- Unit 2 delivered: storyboard and region-filmstrip plans are bounded before
  fit budgeting, normalization, decode, and generation; manifests retain full
  source-frame provenance and omitted counts; exhaustive generators retain
  their own source/decode limits. Reproducing the region-filmstrip discrepancy
  showed the full-frame charge came from `generate_region_filmstrip` passing no
  crop to `normalize_sequence`; the fix normalizes the visible crop and
  separately normalizes only the one full locator frame.
- Unit 3 delivered: resolved-order listing uses offset pages with explicit
  omission and continuation metadata, fetch remains strict, direct generator
  fields have serde defaults sourced from shared artifact constants, and
  `SessionTime` documents session-relative monotonic nanoseconds in schema and
  SPEC.
- Verification completed with workspace tests and the targeted high-DPI,
  resource-limit, progressive-listing, bundle, and registry coverage. Full
  release gates are run before commit with `CARGO_TARGET_DIR` pointed at the
  writable temporary target because the configured default target is read-only.

## Review-fix note (2026-07-19)

Planning failures now honor `AllowPartial` per generator, preserving bounded
storyboards when exhaustive generators reject oversized ranges. Source listings
advertise continuation only when returned pages leave selected frames, and the
concise projection retains every advertised page frame. Storyboard manifests
record bounded pre-selection provenance, including analyzed source indices;
source-fetch recovery text now directs callers to listing pagination. Explicit
zero anchors are distinguished from omitted defaults at the wire boundary,
fully-outside filmstrips normalize a 1×1 crop, and both artifact planning and
filmstrip planning use the shared temporal-vision index selector.
